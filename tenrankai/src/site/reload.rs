use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::config::ConfigStorageLoader;
use crate::gallery::Gallery;
use crate::site::{SiteBuilder, SiteManager};
use tenrankai_config_storage::DynConfigStorage;

/// Result of a config reload operation
#[derive(Debug)]
pub struct ReloadResult {
    /// Sites that were successfully added
    pub added: Vec<String>,
    /// Sites that were successfully updated
    pub updated: Vec<String>,
    /// Sites that were removed
    pub removed: Vec<String>,
    /// Sites that failed to reload (site name, error message)
    pub failed: Vec<(String, String)>,
}

impl ReloadResult {
    pub fn new() -> Self {
        Self {
            added: Vec::new(),
            updated: Vec::new(),
            removed: Vec::new(),
            failed: Vec::new(),
        }
    }

    pub fn is_success(&self) -> bool {
        self.failed.is_empty()
    }

    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.added.is_empty() {
            parts.push(format!("added: {}", self.added.join(", ")));
        }
        if !self.updated.is_empty() {
            parts.push(format!("updated: {}", self.updated.join(", ")));
        }
        if !self.removed.is_empty() {
            parts.push(format!("removed: {}", self.removed.join(", ")));
        }
        if !self.failed.is_empty() {
            let failed_names: Vec<_> = self.failed.iter().map(|(n, _)| n.as_str()).collect();
            parts.push(format!("failed: {}", failed_names.join(", ")));
        }
        if parts.is_empty() {
            "no changes".to_string()
        } else {
            parts.join("; ")
        }
    }
}

impl Default for ReloadResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Handles hot reloading of site configurations from ConfigStorage
pub struct ConfigReloader {
    reload_lock: Mutex<()>,
    storage: DynConfigStorage,
    cookie_secret: String,
    config_storage_url: String,
    hosted_mode: bool,
    cached_versions: Mutex<HashMap<String, u64>>,
}

impl ConfigReloader {
    pub fn new(
        storage: DynConfigStorage,
        cookie_secret: String,
        config_storage_url: String,
    ) -> Self {
        Self {
            reload_lock: Mutex::new(()),
            storage,
            cookie_secret,
            config_storage_url,
            hosted_mode: false,
            cached_versions: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_hosted_mode(mut self, hosted_mode: bool) -> Self {
        self.hosted_mode = hosted_mode;
        self
    }

    /// Check config versions and only reload if something changed.
    ///
    /// Returns `Some(result)` if a reload was performed, `None` if skipped.
    pub async fn poll_and_reload_if_changed(
        &self,
        site_manager: &Arc<SiteManager>,
    ) -> Option<ReloadResult> {
        let current_versions = match self.storage.get_config_versions().await {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to check config versions: {}", e);
                return Some(self.reload(site_manager).await);
            }
        };

        if current_versions.is_empty() {
            debug!("Backend does not support versioning, falling back to full reload");
            return Some(self.reload(site_manager).await);
        }

        {
            let cached = self.cached_versions.lock().await;
            if *cached == current_versions {
                debug!("Config versions unchanged, skipping reload");
                return None;
            }

            debug!(
                "Config versions changed (cached={:?}, current={:?}), reloading",
                *cached, current_versions
            );
        }

        let result = self.reload(site_manager).await;

        let mut cached = self.cached_versions.lock().await;
        *cached = current_versions;

        Some(result)
    }

    /// Reload configuration from ConfigStorage and update the SiteManager
    ///
    /// This method:
    /// 1. Acquires the reload lock (prevents concurrent reloads)
    /// 2. Loads sites from ConfigStorage
    /// 3. Determines which sites need to be added, updated, or removed
    /// 4. Rebuilds sites one at a time, keeping old sites on failure
    /// 5. Returns a summary of what changed
    pub async fn reload(&self, site_manager: &Arc<SiteManager>) -> ReloadResult {
        // Acquire reload lock to prevent concurrent reloads
        let _lock = self.reload_lock.lock().await;

        info!("Starting configuration reload from ConfigStorage");

        let mut result = ReloadResult::new();

        // Step 1: Load sites from ConfigStorage
        let loader = ConfigStorageLoader::new(self.storage.clone(), self.cookie_secret.clone());
        let loaded_sites = match loader.load_all_sites().await {
            Ok(sites) => sites,
            Err(e) => {
                error!("Failed to load sites from ConfigStorage: {}", e);
                result
                    .failed
                    .push(("*".to_string(), format!("Failed to load sites: {}", e)));
                return result;
            }
        };

        // Step 2: Get current and new site configurations
        let current_sites: HashSet<String> = site_manager.site_names().await.into_iter().collect();
        let new_sites: HashSet<String> = loaded_sites.keys().cloned().collect();

        // Determine what changed
        let sites_to_add: Vec<_> = new_sites.difference(&current_sites).cloned().collect();
        let sites_to_remove: Vec<_> = current_sites.difference(&new_sites).cloned().collect();
        let sites_to_check: Vec<_> = current_sites.intersection(&new_sites).cloned().collect();

        info!(
            "Reload diff: {} to add, {} to remove, {} to check",
            sites_to_add.len(),
            sites_to_remove.len(),
            sites_to_check.len()
        );

        // Step 3: Remove old sites
        for site_name in sites_to_remove {
            info!("Removing site '{}'", site_name);
            site_manager.remove_site(&site_name).await;
            result.removed.push(site_name);
        }

        // Step 4: Add new sites
        for site_name in sites_to_add {
            let mut site_config = match loaded_sites.get(&site_name) {
                Some(c) => c.clone(),
                None => continue,
            };

            info!("Adding new site '{}'", site_name);

            site_config.config_storage = Some(self.config_storage_url.clone());
            site_config.hosted_mode = self.hosted_mode;

            // Get hostnames from storage
            let hostnames = match self.storage.get_site_config(&site_name).await {
                Ok(Some(stored)) => stored.hostnames,
                _ => vec!["*".to_string()],
            };

            match self
                .build_site_with_galleries(&site_name, site_config)
                .await
            {
                Ok(site) => {
                    site_manager.add_site(site, hostnames.clone()).await;
                    result.added.push(site_name);
                }
                Err(e) => {
                    error!("Failed to build new site '{}': {}", site_name, e);
                    result.failed.push((site_name, e));
                }
            }
        }

        // Step 5: Update existing sites (rebuild if config changed)
        for site_name in sites_to_check {
            let mut site_config = match loaded_sites.get(&site_name) {
                Some(c) => c.clone(),
                None => continue,
            };

            info!("Updating site '{}'", site_name);

            site_config.config_storage = Some(self.config_storage_url.clone());
            site_config.hosted_mode = self.hosted_mode;

            // Get hostnames from storage
            let hostnames = match self.storage.get_site_config(&site_name).await {
                Ok(Some(stored)) => stored.hostnames,
                _ => vec!["*".to_string()],
            };

            match self
                .build_site_with_galleries(&site_name, site_config)
                .await
            {
                Ok(new_site) => {
                    site_manager
                        .replace_site(&site_name, new_site, hostnames.clone())
                        .await;
                    result.updated.push(site_name);
                }
                Err(e) => {
                    error!(
                        "Failed to rebuild site '{}', keeping old version: {}",
                        site_name, e
                    );
                    result.failed.push((site_name, e));
                }
            }
        }

        info!("Configuration reload complete: {}", result.summary());
        result
    }

    /// Build a site and initialize its galleries
    async fn build_site_with_galleries(
        &self,
        site_name: &str,
        site_config: crate::site::SiteConfig,
    ) -> Result<Arc<crate::site::Site>, String> {
        let site_builder = SiteBuilder::new(site_config);

        let site = site_builder.build().await.map_err(|e| e.to_string())?;
        let site = Arc::new(site);

        // Initialize galleries for this site
        for (gallery_name, gallery) in site.galleries().iter() {
            if let Err(e) = Self::initialize_gallery(site_name, gallery).await {
                warn!(
                    "Failed to initialize gallery '{}' for site '{}': {}",
                    gallery_name, site_name, e
                );
            }
        }

        site.rebuild_shortcode_index().await;
        site.start_shortcode_index_refresh();

        Ok(site)
    }

    /// Initialize a gallery (version check, metadata refresh, background tasks)
    async fn initialize_gallery(site_name: &str, gallery: &Arc<Gallery>) -> Result<(), String> {
        let gallery_config = gallery.get_config();
        let gallery_name = &gallery_config.name;
        let gallery_key = format!("{}:{}", site_name, gallery_name);

        // Initialize gallery and check for version changes
        if let Err(e) = gallery.initialize_and_check_version().await {
            warn!(
                "Failed to initialize gallery '{}' metadata cache: {}",
                gallery_name, e
            );
        }

        // Trigger refresh and/or pre-generation on startup
        let metadata_empty = gallery.is_metadata_cache_empty().await;
        let pregenerate = gallery_config.pregenerate.is_some();

        // Refresh metadata only; pre-generation is routed through the process-global
        // queue below rather than the legacy unqueued parallel path.
        if (metadata_empty || pregenerate)
            && let Err(e) = gallery
                .clone()
                .refresh_metadata_and_pregenerate_cache(false)
                .await
        {
            return Err(format!(
                "Failed to refresh metadata for gallery '{}': {}",
                gallery_name, e
            ));
        }

        if pregenerate {
            match crate::generation::global_manager() {
                Some(manager) => {
                    let queued = manager
                        .enqueue_gallery_pregeneration(gallery_key.clone(), gallery.clone())
                        .await;
                    info!(
                        "Queued {} background pre-generation job(s) for gallery '{}'",
                        queued, gallery_name
                    );
                }
                None => warn!(
                    "Generation manager not installed; skipping pre-generation for '{}'",
                    gallery_name
                ),
            }
        }

        // Start background cache refresh if configured
        if let Some(interval_minutes) = gallery_config.cache_refresh_interval_minutes
            && interval_minutes > 0
        {
            info!(
                "Starting background cache refresh for gallery '{}' every {} minutes",
                gallery_name, interval_minutes
            );
            Gallery::start_background_cache_refresh(
                gallery.clone(),
                interval_minutes,
                gallery_key.clone(),
            );
        }

        // Start periodic cache save (every 5 minutes)
        Gallery::start_periodic_cache_save(gallery.clone(), 5);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reload_result_summary() {
        let mut result = ReloadResult::new();
        assert_eq!(result.summary(), "no changes");

        result.added.push("site1".to_string());
        assert_eq!(result.summary(), "added: site1");

        result.updated.push("site2".to_string());
        assert!(result.summary().contains("added: site1"));
        assert!(result.summary().contains("updated: site2"));

        result
            .failed
            .push(("site3".to_string(), "error".to_string()));
        assert!(result.summary().contains("failed: site3"));
    }

    #[test]
    fn test_reload_result_is_success() {
        let mut result = ReloadResult::new();
        assert!(result.is_success());

        result.added.push("site1".to_string());
        assert!(result.is_success());

        result
            .failed
            .push(("site2".to_string(), "error".to_string()));
        assert!(!result.is_success());
    }
}
