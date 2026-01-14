use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::config::MultiSiteConfig;
use crate::gallery::Gallery;
use crate::site::{SiteBuilder, SiteManager};

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

/// Handles hot reloading of site configurations
pub struct ConfigReloader {
    /// Lock to prevent concurrent reloads
    reload_lock: Mutex<()>,
    /// Path to the config file
    config_path: std::path::PathBuf,
}

impl ConfigReloader {
    pub fn new<P: AsRef<Path>>(config_path: P) -> Self {
        Self {
            reload_lock: Mutex::new(()),
            config_path: config_path.as_ref().to_path_buf(),
        }
    }

    /// Reload configuration and update the SiteManager
    ///
    /// This method:
    /// 1. Acquires the reload lock (prevents concurrent reloads)
    /// 2. Parses the new configuration file
    /// 3. Determines which sites need to be added, updated, or removed
    /// 4. Rebuilds sites one at a time, keeping old sites on failure
    /// 5. Returns a summary of what changed
    pub async fn reload(&self, site_manager: &Arc<SiteManager>) -> ReloadResult {
        // Acquire reload lock to prevent concurrent reloads
        let _lock = self.reload_lock.lock().await;

        info!("Starting configuration reload from {:?}", self.config_path);

        let mut result = ReloadResult::new();

        // Step 1: Parse new configuration
        let config_content = match std::fs::read_to_string(&self.config_path) {
            Ok(content) => content,
            Err(e) => {
                error!("Failed to read config file: {}", e);
                result
                    .failed
                    .push(("*".to_string(), format!("Failed to read config: {}", e)));
                return result;
            }
        };

        let multi_config: MultiSiteConfig = match toml_edit::de::from_str(&config_content) {
            Ok(config) => config,
            Err(e) => {
                error!("Failed to parse config file: {}", e);
                result
                    .failed
                    .push(("*".to_string(), format!("Failed to parse config: {}", e)));
                return result;
            }
        };

        // Check if this is multi-site mode
        if !multi_config.is_multi_site() {
            warn!("Config reload only supported in multi-site mode");
            result.failed.push((
                "*".to_string(),
                "Config reload only supported in multi-site mode".to_string(),
            ));
            return result;
        }

        // Step 2: Get current and new site configurations
        let current_sites: HashSet<String> = site_manager.site_names().await.into_iter().collect();
        let new_site_configs = multi_config.get_site_configs();
        let new_sites: HashSet<String> = new_site_configs.keys().cloned().collect();

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
            let site_section = match new_site_configs.get(&site_name) {
                Some(s) => s,
                None => continue,
            };

            info!("Adding new site '{}'", site_name);

            match Self::build_site_with_galleries(&site_name, site_section, &multi_config).await {
                Ok(site) => {
                    site_manager
                        .add_site(site, site_section.hostnames.clone())
                        .await;
                    result.added.push(site_name);
                }
                Err(e) => {
                    error!("Failed to build new site '{}': {}", site_name, e);
                    result.failed.push((site_name, e));
                }
            }
        }

        // Step 5: Update existing sites (rebuild if config changed)
        // For now, we always rebuild sites that exist in both old and new config
        // A more sophisticated implementation could compare configs and skip unchanged sites
        for site_name in sites_to_check {
            let site_section = match new_site_configs.get(&site_name) {
                Some(s) => s,
                None => continue,
            };

            info!("Updating site '{}'", site_name);

            match Self::build_site_with_galleries(&site_name, site_section, &multi_config).await {
                Ok(new_site) => {
                    site_manager
                        .replace_site(&site_name, new_site, site_section.hostnames.clone())
                        .await;
                    result.updated.push(site_name);
                }
                Err(e) => {
                    error!(
                        "Failed to rebuild site '{}', keeping old version: {}",
                        site_name, e
                    );
                    result.failed.push((site_name, e));
                    // Old site remains in place - this is the resilient behavior
                }
            }
        }

        info!("Configuration reload complete: {}", result.summary());
        result
    }

    /// Build a site and initialize its galleries
    async fn build_site_with_galleries(
        site_name: &str,
        site_section: &crate::config::multi_site::SiteConfigSection,
        multi_config: &MultiSiteConfig,
    ) -> Result<Arc<crate::site::Site>, String> {
        let site_config =
            site_section.to_site_config(site_name, &multi_config.app, multi_config.email.as_ref());
        let site_builder = SiteBuilder::new(site_config);

        let site = site_builder.build().await.map_err(|e| e.to_string())?;
        let site = Arc::new(site);

        // Initialize galleries for this site
        for (gallery_name, gallery) in site.galleries().iter() {
            if let Err(e) = Self::initialize_gallery(gallery).await {
                warn!(
                    "Failed to initialize gallery '{}' for site '{}': {}",
                    gallery_name, site_name, e
                );
            }
        }

        Ok(site)
    }

    /// Initialize a gallery (version check, metadata refresh, background tasks)
    async fn initialize_gallery(gallery: &Arc<Gallery>) -> Result<(), String> {
        let gallery_config = gallery.get_config();
        let gallery_name = &gallery_config.name;

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

        if (metadata_empty || pregenerate)
            && let Err(e) = gallery
                .clone()
                .refresh_metadata_and_pregenerate_cache(pregenerate)
                .await
        {
            return Err(format!(
                "Failed to refresh metadata for gallery '{}': {}",
                gallery_name, e
            ));
        }

        // Start background cache refresh if configured
        if let Some(interval_minutes) = gallery_config.cache_refresh_interval_minutes
            && interval_minutes > 0
        {
            info!(
                "Starting background cache refresh for gallery '{}' every {} minutes",
                gallery_name, interval_minutes
            );
            Gallery::start_background_cache_refresh(gallery.clone(), interval_minutes);
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
