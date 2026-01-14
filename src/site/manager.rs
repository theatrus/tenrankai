use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use super::Site;

/// SiteManager holds all active sites and handles routing dispatch by hostname
pub struct SiteManager {
    /// Map of site name to Site instance
    sites: RwLock<HashMap<String, Arc<Site>>>,
    /// Map of exact hostname to site name for O(1) lookup
    hostname_index: RwLock<HashMap<String, String>>,
    /// Ordered list of glob patterns (pattern, site_name) for wildcard matching
    glob_patterns: RwLock<Vec<(String, String)>>,
    /// Name of the default site (matches "*")
    default_site: RwLock<Option<String>>,
}

impl SiteManager {
    /// Create a new empty SiteManager
    pub fn new() -> Self {
        Self {
            sites: RwLock::new(HashMap::new()),
            hostname_index: RwLock::new(HashMap::new()),
            glob_patterns: RwLock::new(Vec::new()),
            default_site: RwLock::new(None),
        }
    }

    /// Create a SiteManager with a single default site (backward compatibility)
    pub fn with_default_site(site: Arc<Site>) -> Self {
        let manager = Self::new();

        // We need to use blocking operations since this is a sync constructor
        // In practice, this is only called once at startup
        let mut sites = HashMap::new();
        let site_name = site.name.clone();
        sites.insert(site_name.clone(), site);

        // Use try_write since we know we have exclusive access at construction
        *manager.sites.try_write().unwrap() = sites;
        *manager.default_site.try_write().unwrap() = Some(site_name);

        manager
    }

    /// Add a site with its hostname mappings
    pub async fn add_site(&self, site: Arc<Site>, hostnames: Vec<String>) {
        let site_name = site.name.clone();

        // Add site to sites map
        {
            let mut sites = self.sites.write().await;
            sites.insert(site_name.clone(), site);
        }

        // Process hostnames
        let mut hostname_index = self.hostname_index.write().await;
        let mut glob_patterns = self.glob_patterns.write().await;
        let mut default_site = self.default_site.write().await;

        for hostname in hostnames {
            if hostname == "*" {
                // Catch-all default site
                *default_site = Some(site_name.clone());
                info!("Site '{}' registered as default (catch-all)", site_name);
            } else if hostname.starts_with("*.") {
                // Glob pattern
                glob_patterns.push((hostname.clone(), site_name.clone()));
                info!(
                    "Site '{}' registered for glob pattern '{}'",
                    site_name, hostname
                );
            } else {
                // Exact hostname
                hostname_index.insert(hostname.clone(), site_name.clone());
                info!(
                    "Site '{}' registered for hostname '{}'",
                    site_name, hostname
                );
            }
        }
    }

    /// Get site for a given hostname (supports exact match, glob patterns, and default)
    ///
    /// Matching order:
    /// 1. Exact match (highest priority)
    /// 2. Glob pattern match (in registration order)
    /// 3. Default site (*)
    pub async fn get_site(&self, hostname: &str) -> Option<Arc<Site>> {
        // Strip port if present (e.g., "localhost:3000" -> "localhost")
        let hostname_no_port = hostname.split(':').next().unwrap_or(hostname);

        debug!("Looking up site for hostname: {}", hostname_no_port);

        // 1. Try exact match first (highest priority)
        {
            let hostname_index = self.hostname_index.read().await;
            if let Some(site_name) = hostname_index.get(hostname_no_port) {
                let sites = self.sites.read().await;
                if let Some(site) = sites.get(site_name) {
                    debug!(
                        "Exact match found: hostname '{}' -> site '{}'",
                        hostname_no_port, site_name
                    );
                    return Some(site.clone());
                }
            }
        }

        // 2. Try glob pattern matches (in registration order)
        {
            let glob_patterns = self.glob_patterns.read().await;
            for (pattern, site_name) in glob_patterns.iter() {
                if hostname_matches_glob(pattern, hostname_no_port) {
                    let sites = self.sites.read().await;
                    if let Some(site) = sites.get(site_name) {
                        debug!(
                            "Glob match found: hostname '{}' matches '{}' -> site '{}'",
                            hostname_no_port, pattern, site_name
                        );
                        return Some(site.clone());
                    }
                }
            }
        }

        // 3. Fall back to default site (*)
        {
            let default_site = self.default_site.read().await;
            if let Some(default_name) = default_site.as_ref() {
                let sites = self.sites.read().await;
                if let Some(site) = sites.get(default_name) {
                    debug!(
                        "Using default site '{}' for hostname '{}'",
                        default_name, hostname_no_port
                    );
                    return Some(site.clone());
                }
            }
        }

        debug!("No site found for hostname: {}", hostname_no_port);
        None
    }

    /// Get the default site (if any)
    pub async fn get_default_site(&self) -> Option<Arc<Site>> {
        let default_site = self.default_site.read().await;
        if let Some(default_name) = default_site.as_ref() {
            let sites = self.sites.read().await;
            return sites.get(default_name).cloned();
        }
        None
    }

    /// Get a site by name
    pub async fn get_site_by_name(&self, name: &str) -> Option<Arc<Site>> {
        let sites = self.sites.read().await;
        sites.get(name).cloned()
    }

    /// Get all site names
    pub async fn site_names(&self) -> Vec<String> {
        let sites = self.sites.read().await;
        sites.keys().cloned().collect()
    }

    /// Get all sites
    pub async fn sites(&self) -> Vec<Arc<Site>> {
        let sites = self.sites.read().await;
        sites.values().cloned().collect()
    }

    /// Remove a site by name
    pub async fn remove_site(&self, name: &str) {
        // Remove from sites map
        let old_site = {
            let mut sites = self.sites.write().await;
            sites.remove(name)
        };

        if old_site.is_none() {
            return;
        }

        // Clean up hostname mappings
        {
            let mut hostname_index = self.hostname_index.write().await;
            hostname_index.retain(|_, site_name| site_name != name);
        }

        {
            let mut glob_patterns = self.glob_patterns.write().await;
            glob_patterns.retain(|(_, site_name)| site_name != name);
        }

        {
            let mut default_site = self.default_site.write().await;
            if default_site.as_ref() == Some(&name.to_string()) {
                *default_site = None;
            }
        }

        info!("Removed site '{}'", name);
    }

    /// Replace a site with a new version (for hot-swapping)
    pub async fn replace_site(&self, name: &str, new_site: Arc<Site>, hostnames: Vec<String>) {
        // Remove old site mappings
        self.remove_site(name).await;

        // Add new site with its hostname mappings
        self.add_site(new_site, hostnames).await;
    }

    /// Check if a site exists
    pub async fn has_site(&self, name: &str) -> bool {
        let sites = self.sites.read().await;
        sites.contains_key(name)
    }

    /// Get the number of sites
    pub async fn site_count(&self) -> usize {
        let sites = self.sites.read().await;
        sites.len()
    }
}

impl Default for SiteManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a hostname matches a glob pattern
///
/// Supports:
/// - `*` - matches everything (catch-all)
/// - `*.example.com` - matches any subdomain of example.com
fn hostname_matches_glob(pattern: &str, hostname: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if pattern.starts_with("*.") {
        // Wildcard subdomain: *.example.com matches foo.example.com
        // but not example.com or foo.bar.example.com (single level only)
        let suffix = &pattern[1..]; // .example.com
        if hostname.ends_with(suffix) && hostname.len() > suffix.len() {
            // Check that it's a single-level subdomain
            let prefix = &hostname[..hostname.len() - suffix.len()];
            return !prefix.contains('.');
        }
        return false;
    }

    pattern == hostname
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::site::SiteResources;

    fn create_test_site(name: &str) -> Arc<Site> {
        let resources = SiteResources {
            template_engine: Arc::new(crate::templating::TemplateEngine::new(vec![])),
            static_handler: crate::static_files::StaticFileHandler::new(vec![]),
            favicon_renderer: crate::favicon::FaviconRenderer::new(vec![]),
            galleries: Arc::new(std::collections::HashMap::new()),
            posts_managers: Arc::new(std::collections::HashMap::new()),
            login_state: Arc::new(tokio::sync::RwLock::new(crate::login::LoginState::new())),
            user_database_manager: None,
            email_config: None,
        };
        Arc::new(Site::new(name.to_string(), resources))
    }

    #[test]
    fn test_hostname_matches_glob_exact() {
        assert!(hostname_matches_glob("example.com", "example.com"));
        assert!(!hostname_matches_glob("example.com", "other.com"));
        assert!(!hostname_matches_glob("example.com", "sub.example.com"));
    }

    #[test]
    fn test_hostname_matches_glob_wildcard() {
        assert!(hostname_matches_glob("*.example.com", "foo.example.com"));
        assert!(hostname_matches_glob("*.example.com", "bar.example.com"));
        assert!(!hostname_matches_glob("*.example.com", "example.com"));
        // Multi-level subdomains should not match single wildcard
        assert!(!hostname_matches_glob(
            "*.example.com",
            "foo.bar.example.com"
        ));
    }

    #[test]
    fn test_hostname_matches_glob_catchall() {
        assert!(hostname_matches_glob("*", "anything.com"));
        assert!(hostname_matches_glob("*", "localhost"));
        assert!(hostname_matches_glob("*", "sub.domain.example.com"));
    }

    #[tokio::test]
    async fn test_site_manager_exact_match() {
        let manager = SiteManager::new();
        let site = create_test_site("photos");

        manager
            .add_site(site.clone(), vec!["photos.example.com".to_string()])
            .await;

        let found = manager.get_site("photos.example.com").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "photos");

        let not_found = manager.get_site("other.example.com").await;
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_site_manager_glob_match() {
        let manager = SiteManager::new();
        let site = create_test_site("wildcard");

        manager
            .add_site(site.clone(), vec!["*.example.com".to_string()])
            .await;

        let found = manager.get_site("foo.example.com").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "wildcard");

        let found2 = manager.get_site("bar.example.com").await;
        assert!(found2.is_some());
    }

    #[tokio::test]
    async fn test_site_manager_default_fallback() {
        let manager = SiteManager::new();
        let site = create_test_site("default");

        manager.add_site(site.clone(), vec!["*".to_string()]).await;

        // Any hostname should match the default
        let found = manager.get_site("random.hostname.com").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "default");
    }

    #[tokio::test]
    async fn test_site_manager_priority() {
        let manager = SiteManager::new();

        let exact_site = create_test_site("exact");
        let glob_site = create_test_site("glob");
        let default_site = create_test_site("default");

        // Add in reverse priority order to test matching priority
        manager.add_site(default_site, vec!["*".to_string()]).await;
        manager
            .add_site(glob_site, vec!["*.example.com".to_string()])
            .await;
        manager
            .add_site(exact_site, vec!["specific.example.com".to_string()])
            .await;

        // Exact match should have highest priority
        let found = manager.get_site("specific.example.com").await;
        assert_eq!(found.unwrap().name, "exact");

        // Glob match should be next
        let found = manager.get_site("other.example.com").await;
        assert_eq!(found.unwrap().name, "glob");

        // Default should be fallback
        let found = manager.get_site("completely.different.com").await;
        assert_eq!(found.unwrap().name, "default");
    }

    #[tokio::test]
    async fn test_site_manager_port_stripping() {
        let manager = SiteManager::new();
        let site = create_test_site("local");

        manager
            .add_site(site.clone(), vec!["localhost".to_string()])
            .await;

        // Should match even with port
        let found = manager.get_site("localhost:3000").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "local");
    }

    #[tokio::test]
    async fn test_site_manager_remove_site() {
        let manager = SiteManager::new();
        let site = create_test_site("temp");

        manager
            .add_site(
                site.clone(),
                vec!["temp.example.com".to_string(), "*.temp.local".to_string()],
            )
            .await;

        // Verify it exists
        assert!(manager.has_site("temp").await);
        assert!(manager.get_site("temp.example.com").await.is_some());

        // Remove it
        manager.remove_site("temp").await;

        // Verify it's gone
        assert!(!manager.has_site("temp").await);
        assert!(manager.get_site("temp.example.com").await.is_none());
    }
}
