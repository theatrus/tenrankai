# Multi-Site Virtual Host Architecture with Hot Reloading

## Problem Statement

Currently, Tenrankai runs as a single server instance with a single configuration file. This limits deployment flexibility:

1. **Single domain only**: Cannot serve multiple domains/subdomains from one instance
2. **No runtime changes**: Adding galleries, changing settings, or modifying routes requires server restart
3. **No isolation**: All galleries share the same URL namespace, templates, and static files

We want to support:
- Multiple "sites" distinguished by `Host` header (virtual hosting)
- Runtime configuration reloading without downtime
- Hot-swapping of galleries, blogs, and virtual hosts when config changes
- Per-site isolation of templates, static files, and authentication

## Proposed Solution

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         Axum Router                              │
│                    (Host-based dispatch)                         │
└─────────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        ▼                     ▼                     ▼
┌───────────────┐     ┌───────────────┐     ┌───────────────┐
│  Site: *      │     │Site: photos.  │     │Site: blog.    │
│  (default)    │     │  example.com  │     │  example.com  │
├───────────────┤     ├───────────────┤     ├───────────────┤
│ Galleries     │     │ Galleries     │     │ Posts         │
│ Posts         │     │ Static Files  │     │ Static Files  │
│ Static Files  │     │ Templates     │     │ Templates     │
│ Templates     │     │ Auth          │     │ Auth          │
└───────────────┘     └───────────────┘     └───────────────┘
        │                     │                     │
        └─────────────────────┴─────────────────────┘
                              │
                    ┌─────────┴─────────┐
                    │  Shared Resources │
                    │  - Email Provider │
                    │  - Global Config  │
                    └───────────────────┘
```

### Key Components

1. **SiteManager**: Holds all active sites, handles routing dispatch
2. **Site**: Contains all resources for a single virtual host (galleries, posts, templates, etc.)
3. **ConfigWatcher**: Monitors config file for changes, triggers reloads
4. **HotSwapper**: Handles atomic replacement of site components

## Configuration Structure

### Current Config (Single Site)
```toml
[server]
host = "0.0.0.0"
port = 3000

[[galleries]]
name = "main"
url_prefix = "/gallery"
source_directory = "photos"
# ...

[[posts]]
name = "blog"
url_prefix = "/blog"
# ...
```

### Proposed Config (Multi-Site)

```toml
# Global server configuration
[server]
host = "0.0.0.0"
port = 3000
config_reload = true          # Enable SIGHUP config reload
reload_endpoint = "/_reload"  # Optional API endpoint (requires auth)

# Shared email configuration (used by all sites)
[email]
provider = "ses"
from_address = "noreply@example.com"
# ...

# Default site (matches when no other site matches)
# Can use "*" or omit hostname entirely
[sites.default]
hostnames = ["*"]  # Catch-all, or could use ["localhost", "127.0.0.1"]

[sites.default.templates]
directories = ["templates"]

[sites.default.static_files]
directories = ["static"]

[[sites.default.galleries]]
name = "main"
url_prefix = "/gallery"
source_directory = "photos"
cache_directory = "cache/main"
# ... all existing gallery config options

[[sites.default.posts]]
name = "blog"
url_prefix = "/blog"
source_directory = "posts"
# ... all existing posts config options

# Additional virtual host
[sites.photos]
hostnames = ["photos.example.com", "photos.local"]

[sites.photos.templates]
directories = ["templates-photos", "templates"]  # Fallback to default

[sites.photos.static_files]
directories = ["static-photos"]

[[sites.photos.galleries]]
name = "portfolio"
url_prefix = "/"  # Root of this virtual host
source_directory = "portfolio-photos"
cache_directory = "cache/portfolio"
# ...

# Blog-only subdomain
[sites.blog]
hostnames = ["blog.example.com"]

[sites.blog.templates]
directories = ["templates-blog", "templates"]

[[sites.blog.posts]]
name = "articles"
url_prefix = "/"
source_directory = "blog-posts"
# ...
```

### Configuration Inheritance

Sites can inherit from other sites to reduce duplication:

```toml
[sites.staging]
hostnames = ["staging.example.com"]
inherit_from = "default"  # Inherit all settings from default site

# Override only specific settings
[[sites.staging.galleries]]
name = "main"
url_prefix = "/gallery"
source_directory = "staging-photos"  # Different source
cache_directory = "cache/staging"
```

## Implementation Design

### 1. Site Structure

```rust
// src/site/mod.rs
pub struct Site {
    pub name: String,
    pub hostnames: Vec<String>,
    pub galleries: HashMap<String, SharedGallery>,
    pub posts: HashMap<String, Arc<PostsManager>>,
    pub templates: Arc<TemplateEngine>,
    pub static_files: StaticFilesConfig,
    pub login_manager: Option<Arc<LoginManager>>,
    pub router: Router,
}

impl Site {
    pub fn new(config: SiteConfig) -> Result<Self, ConfigError> {
        // Build all components for this site
        // Create router with all routes
    }

    pub fn router(&self) -> Router {
        self.router.clone()
    }
}
```

### 2. SiteManager

```rust
// src/site/manager.rs
pub struct SiteManager {
    sites: Arc<RwLock<HashMap<String, Arc<Site>>>>,
    hostname_index: Arc<RwLock<HashMap<String, String>>>,  // hostname/pattern -> site name
    glob_patterns: Arc<RwLock<Vec<(String, String)>>>,     // Ordered glob patterns for matching
    default_site: Arc<RwLock<Option<String>>>,
    reload_lock: Arc<Mutex<()>>,  // Ensures sequential reload processing
}

impl SiteManager {
    pub fn new() -> Self { ... }

    /// Load all sites from configuration
    pub async fn load_config(&self, config: &MultiSiteConfig) -> Result<(), ConfigError> {
        let mut sites = HashMap::new();
        let mut hostname_index = HashMap::new();

        for (name, site_config) in &config.sites {
            let site = Arc::new(Site::new(site_config.clone())?);

            for hostname in &site_config.hostnames {
                if hostname == "*" {
                    *self.default_site.write().await = Some(name.clone());
                } else {
                    hostname_index.insert(hostname.clone(), name.clone());
                }
            }

            sites.insert(name.clone(), site);
        }

        *self.sites.write().await = sites;
        *self.hostname_index.write().await = hostname_index;
        Ok(())
    }

    /// Get site for a given hostname (supports glob patterns like *.example.com)
    pub async fn get_site(&self, hostname: &str) -> Option<Arc<Site>> {
        let hostname_index = self.hostname_index.read().await;
        let sites = self.sites.read().await;

        // Strip port if present
        let hostname_no_port = hostname.split(':').next().unwrap_or(hostname);

        // 1. Try exact match first (highest priority)
        if let Some(site_name) = hostname_index.get(hostname_no_port) {
            return sites.get(site_name).cloned();
        }

        // 2. Try glob pattern matches (in registration order)
        for (pattern, site_name) in hostname_index.iter() {
            if pattern.starts_with("*.") {
                let suffix = &pattern[1..];  // .example.com
                if hostname_no_port.ends_with(suffix) && hostname_no_port.len() > suffix.len() {
                    return sites.get(site_name).cloned();
                }
            }
        }

        // 3. Fall back to default site (*)
        if let Some(default_name) = self.default_site.read().await.as_ref() {
            return sites.get(default_name).cloned();
        }

        None
    }

    /// Hot-swap a single site
    pub async fn replace_site(&self, name: &str, new_site: Arc<Site>) {
        let mut sites = self.sites.write().await;
        let mut hostname_index = self.hostname_index.write().await;

        // Remove old hostname mappings
        if let Some(old_site) = sites.get(name) {
            for hostname in &old_site.hostnames {
                hostname_index.remove(hostname);
            }
        }

        // Add new hostname mappings
        for hostname in &new_site.hostnames {
            if hostname != "*" {
                hostname_index.insert(hostname.clone(), name.to_string());
            }
        }

        sites.insert(name.to_string(), new_site);
    }
}
```

### 3. Host-Based Routing

```rust
// src/routing.rs
use axum::{
    extract::Host,
    middleware::{self, Next},
    response::Response,
};

pub fn create_dispatch_router(site_manager: Arc<SiteManager>) -> Router {
    Router::new()
        .fallback(dispatch_to_site)
        .layer(Extension(site_manager))
}

async fn dispatch_to_site(
    Host(hostname): Host,
    Extension(site_manager): Extension<Arc<SiteManager>>,
    request: Request,
) -> Response {
    match site_manager.get_site(&hostname).await {
        Some(site) => {
            // Forward request to site's router
            site.router().oneshot(request).await.unwrap_or_else(|_| {
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            })
        }
        None => {
            // No matching site
            (StatusCode::NOT_FOUND, "Site not found").into_response()
        }
    }
}
```

### 4. Configuration Reloading

#### 4.1 Signal-Based Reload (SIGHUP)

```rust
// src/config/reload.rs
use tokio::signal::unix::{signal, SignalKind};

pub async fn setup_config_reload(
    site_manager: Arc<SiteManager>,
    config_path: PathBuf,
) {
    let mut sighup = signal(SignalKind::hangup())
        .expect("Failed to register SIGHUP handler");

    tokio::spawn(async move {
        loop {
            sighup.recv().await;
            info!("Received SIGHUP, reloading configuration...");

            match site_manager.reload_config(&config_path).await {
                Ok(result) => {
                    if result.failed_sites.is_empty() {
                        info!("Configuration reloaded successfully: {} sites updated",
                            result.successful_sites.len());
                    } else {
                        warn!("Configuration reload completed with errors: {} succeeded, {} failed",
                            result.successful_sites.len(), result.failed_sites.len());
                        for (site, error) in &result.failed_sites {
                            error!("  Site '{}': {}", site, error);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to parse configuration: {} (no changes applied)", e);
                }
            }
        }
    });
}

impl SiteManager {
    /// Reload configuration - sequential, resilient to per-site failures
    pub async fn reload_config(&self, config_path: &Path) -> Result<ReloadResult, ConfigError> {
        // Acquire reload lock - only one reload at a time
        // If another reload is in progress, this will wait
        let _guard = self.reload_lock.lock().await;

        // 1. Parse new config (fail fast on syntax errors)
        let new_config = parse_config(config_path)?;

        // 2. Validate new config structure
        validate_config(&new_config)?;

        // 3. Diff against current config
        let changes = diff_config(self, &new_config).await;

        // 4. Apply changes (continues through errors, preserves old state on failure)
        let result = apply_changes(self, &new_config, &changes).await;

        Ok(result)
    }
}
```

#### 4.2 API Endpoint Reload

```rust
// src/api.rs

#[derive(Serialize)]
pub struct ReloadResponse {
    pub success: bool,
    pub successful_sites: Vec<String>,
    pub failed_sites: Vec<FailedSite>,
    pub removed_sites: Vec<String>,
}

#[derive(Serialize)]
pub struct FailedSite {
    pub name: String,
    pub error: String,
}

pub async fn reload_config_handler(
    Extension(site_manager): Extension<Arc<SiteManager>>,
    Extension(config_path): Extension<PathBuf>,
    auth: AuthenticatedUser,
) -> Result<Json<ReloadResponse>, AppError> {
    // Check admin permission
    if !auth.is_admin() {
        return Err(AppError::Forbidden);
    }

    let result = site_manager.reload_config(&config_path).await?;

    Ok(Json(ReloadResponse {
        success: result.failed_sites.is_empty(),
        successful_sites: result.successful_sites,
        failed_sites: result.failed_sites.into_iter()
            .map(|(name, error)| FailedSite { name, error })
            .collect(),
        removed_sites: result.removed_sites,
    }))
}
```

### 5. Hot-Swap Strategy

The hot-swap approach uses Arc reference counting for zero-downtime updates:

1. **Build new Site**: Create new Site instance with updated configuration
2. **Atomic swap**: Replace Arc pointer in SiteManager
3. **Graceful drain**: Old Site continues serving in-flight requests until Arc refcount reaches 0
4. **Resilient processing**: Continue through errors, preserve old state on failure (see [Design Decisions](#4-resilient-reload-with-partial-success))

```rust
async fn build_and_swap_site(
    site_manager: &SiteManager,
    new_config: &MultiSiteConfig,
    site_name: &str,
) -> Result<(), ConfigError> {
    let site_config = new_config.sites.get(site_name)
        .ok_or_else(|| ConfigError::SiteNotFound(site_name.to_string()))?;

    // Build new site (this may take time for gallery initialization)
    // If this fails, the old site remains active
    let new_site = Arc::new(Site::new(site_config.clone())?);

    // Atomic swap - old site continues serving until last request completes
    site_manager.replace_site(site_name, new_site).await;

    Ok(())
}
```

See [Design Decisions: Resilient Reload with Partial Success](#4-resilient-reload-with-partial-success) for the full `apply_changes` implementation that handles errors gracefully.

## Change Detection

### What Can Be Hot-Reloaded

| Change Type | Hot-Reload | Notes |
|-------------|------------|-------|
| Add new site | ✅ | New router added |
| Remove site | ✅ | Existing requests complete first |
| Modify site hostnames | ✅ | Hostname index updated |
| Add gallery to site | ✅ | Site rebuilt |
| Remove gallery | ✅ | Site rebuilt |
| Modify gallery config | ✅ | Gallery rebuilt |
| Add/remove posts | ✅ | Site rebuilt |
| Change templates | ✅ | Site rebuilt |
| Change static files | ✅ | Site rebuilt |
| Change server port | ❌ | Requires restart |
| Change server host | ❌ | Requires restart |
| Change email provider | ✅ | Shared resource updated |

### Config Diff Algorithm

```rust
#[derive(Debug)]
pub struct ReloadChanges {
    pub added_sites: Vec<String>,
    pub removed_sites: Vec<String>,
    pub modified_sites: Vec<String>,
}

pub async fn diff_config(
    site_manager: &SiteManager,
    new_config: &MultiSiteConfig,
) -> ReloadChanges {
    let current_sites = site_manager.site_names().await;
    let new_sites: HashSet<_> = new_config.sites.keys().cloned().collect();

    let added: Vec<_> = new_sites.difference(&current_sites).cloned().collect();
    let removed: Vec<_> = current_sites.difference(&new_sites).cloned().collect();

    let mut modified = Vec::new();
    for name in current_sites.intersection(&new_sites) {
        if site_config_changed(site_manager, name, new_config).await {
            modified.push(name.clone());
        }
    }

    ReloadChanges { added_sites: added, removed_sites: removed, modified_sites: modified }
}
```

## Migration Path

### Phase 1: Internal Refactoring
1. Create `Site` struct that encapsulates current single-site functionality
2. Move galleries, posts, templates, static_files into Site
3. No config changes yet - maintain backward compatibility

### Phase 2: Multi-Site Support
1. Add `SiteManager` for managing multiple sites
2. Implement host-based routing dispatch
3. Support new multi-site config format
4. Maintain backward compatibility with old config format

### Phase 3: Hot Reloading
1. Add SIGHUP handler for config reload
2. Implement config diffing
3. Add hot-swap logic for sites
4. Optional: Add reload API endpoint

### Backward Compatibility

Old config format (no `[sites]` section) automatically creates a default site:

```rust
fn migrate_legacy_config(config: LegacyConfig) -> MultiSiteConfig {
    MultiSiteConfig {
        server: config.server,
        email: config.email,
        sites: hashmap! {
            "default".to_string() => SiteConfig {
                hostnames: vec!["*".to_string()],
                galleries: config.galleries,
                posts: config.posts,
                templates: config.templates,
                static_files: config.static_files,
                login: config.login,
            }
        }
    }
}
```

## Files to Modify/Create

| File | Action | Description |
|------|--------|-------------|
| `src/site/mod.rs` | Create | Site struct and builder |
| `src/site/manager.rs` | Create | SiteManager for multi-site routing |
| `src/site/reload.rs` | Create | Config reload logic |
| `src/config/multi_site.rs` | Create | Multi-site config types |
| `src/config/migration.rs` | Create | Legacy config migration |
| `src/routing.rs` | Modify | Add host-based dispatch |
| `src/main.rs` | Modify | Initialize SiteManager, setup reload |
| `src/lib.rs` | Modify | Export new modules |

## Testing Strategy

### Unit Tests
- Config parsing for multi-site format
- Legacy config migration
- Config diff algorithm
- Hostname matching (with/without port, wildcards)

### Integration Tests
- Multiple sites responding to different Host headers
- Hot-swap doesn't drop in-flight requests
- SIGHUP triggers reload
- Invalid config doesn't crash server

### Manual Testing
```bash
# Test host-based routing
curl -H "Host: photos.example.com" http://localhost:3000/
curl -H "Host: blog.example.com" http://localhost:3000/

# Test config reload
kill -HUP $(pgrep tenrankai)

# Test API reload (if enabled)
curl -X POST http://localhost:3000/_reload -H "Authorization: Bearer $TOKEN"
```

## Security Considerations

1. **Reload API Authentication**: Reload endpoint must require admin authentication
2. **Config Validation**: Validate config before applying to prevent DoS
3. **Resource Limits**: Limit number of sites/galleries to prevent resource exhaustion
4. **Hostname Validation**: Prevent hostname hijacking via malformed config
5. **Graceful Degradation**: Invalid config should not affect running sites

## Performance Considerations

1. **Hostname Lookup**: O(1) HashMap lookup for hostname -> site mapping
2. **Hot-Swap Overhead**: Building new Site may take seconds for large galleries
3. **Memory**: Each site has its own template engine, increasing memory usage
4. **Connection Draining**: Old sites continue serving until Arc refcount reaches 0

## Design Decisions

These decisions were made during design review:

### 1. No Gallery Sharing Between Sites
Each site has its own isolated set of galleries. Galleries cannot be shared across sites.

**Rationale**: Simplifies the architecture and avoids complex reference counting issues. If the same content needs to be served on multiple domains, configure multiple sites pointing to the same source directory (they will have separate caches).

### 2. Glob Patterns for Hostnames
Support glob patterns in addition to explicit hostname lists:

```toml
[sites.wildcard]
hostnames = ["*.example.com", "example.com"]  # Glob pattern + explicit
```

**Matching order**:
1. Exact match (highest priority)
2. Glob pattern match (in config order)
3. Default site (`*`)

**Implementation**:
```rust
/// Check if hostname matches a pattern (supports * wildcards)
fn hostname_matches(pattern: &str, hostname: &str) -> bool {
    if pattern == "*" {
        return true;  // Catch-all
    }
    if pattern.starts_with("*.") {
        // Wildcard subdomain: *.example.com matches foo.example.com
        let suffix = &pattern[1..];  // .example.com
        return hostname.ends_with(suffix) && hostname.len() > suffix.len();
    }
    pattern == hostname
}
```

### 3. Sequential Reload Processing
Reloads are processed sequentially - only one reload can be in progress at a time. Subsequent SIGHUP signals or API calls while a reload is in progress are queued or ignored.

**Implementation**:
```rust
pub struct SiteManager {
    // ... existing fields
    reload_lock: Arc<Mutex<()>>,  // Ensures sequential reload processing
}

pub async fn reload_config(&self, config_path: &Path) -> Result<ReloadResult, ConfigError> {
    // Acquire reload lock - only one reload at a time
    let _guard = self.reload_lock.lock().await;

    // Process reload...
}
```

### 4. Resilient Reload with Partial Success
Reloads continue through errors, preserving the old state for sites that fail to load. This ensures a single misconfigured site doesn't bring down the entire server.

**Behavior**:
- Parse entire config first (fail fast on syntax errors)
- Attempt to build each changed site
- On site build failure: log error, keep old site, continue to next
- Return summary of successes and failures

**Implementation**:
```rust
pub struct ReloadResult {
    pub successful_sites: Vec<String>,
    pub failed_sites: Vec<(String, String)>,  // (site_name, error_message)
    pub removed_sites: Vec<String>,
}

pub async fn apply_changes(
    site_manager: &SiteManager,
    new_config: &MultiSiteConfig,
    changes: &ReloadChanges,
) -> ReloadResult {
    let mut result = ReloadResult::default();

    // Process modified sites - continue on error
    for site_name in &changes.modified_sites {
        match build_and_swap_site(site_manager, new_config, site_name).await {
            Ok(()) => {
                info!("Hot-swapped site '{}'", site_name);
                result.successful_sites.push(site_name.clone());
            }
            Err(e) => {
                error!("Failed to reload site '{}': {} (keeping old version)", site_name, e);
                result.failed_sites.push((site_name.clone(), e.to_string()));
                // Continue to next site - don't abort
            }
        }
    }

    // Process added sites - continue on error
    for site_name in &changes.added_sites {
        match build_and_add_site(site_manager, new_config, site_name).await {
            Ok(()) => {
                info!("Added new site '{}'", site_name);
                result.successful_sites.push(site_name.clone());
            }
            Err(e) => {
                error!("Failed to add site '{}': {}", site_name, e);
                result.failed_sites.push((site_name.clone(), e.to_string()));
            }
        }
    }

    // Remove sites (these shouldn't fail)
    for site_name in &changes.removed_sites {
        site_manager.remove_site(site_name).await;
        info!("Removed site '{}'", site_name);
        result.removed_sites.push(site_name.clone());
    }

    result
}
```

### 5. Config Source Abstraction (Future)
The config loading system should be designed with future remote config sources in mind (S3, DynamoDB). This doesn't need to be implemented immediately but the architecture should support it.

**Planned abstraction**:
```rust
#[async_trait]
pub trait ConfigSource: Send + Sync {
    /// Load configuration from this source
    async fn load(&self) -> Result<MultiSiteConfig, ConfigError>;

    /// Watch for changes (optional - not all sources support this)
    async fn watch(&self) -> Option<tokio::sync::watch::Receiver<()>> {
        None
    }
}

// Implementations
pub struct FileConfigSource { path: PathBuf }
pub struct S3ConfigSource { bucket: String, key: String, client: S3Client }
pub struct DynamoConfigSource { table: String, key: String, client: DynamoClient }
```

## Future Enhancements

### Near-Term (Config Source Abstraction)
1. **Config from S3**: Load config from S3 bucket with optional polling for changes
2. **Config from DynamoDB**: Load config from DynamoDB with change streams
3. **Config Caching**: Cache remote config locally for faster startup

### Medium-Term
4. **Webhook Notifications**: Notify external services on config change
5. **Health Checks per Site**: Individual health endpoints for each site
6. **Metrics per Site**: Prometheus metrics labeled by site

### Long-Term
7. **Rate Limiting per Site**: Different rate limits for different sites
8. **Site-Specific Logging**: Separate log files per site
9. **Dynamic Site Discovery**: Auto-discover sites from directory structure or database
