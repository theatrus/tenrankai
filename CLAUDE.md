# Tenrankai Project Documentation

## Project Overview
Tenrankai is a Rust/Axum photo gallery server with React/TypeScript frontend. Features include image resizing, metadata extraction, watermarking, caching, multiple galleries, markdown-based posts, and email-based authentication.

## Quick Reference

### Essential Commands
```bash
# Development (use --no-default-features for faster builds)
cargo run --no-default-features -- serve          # Start server
cargo run --no-default-features -- serve --quit-after 5  # Auto-shutdown test
npm run dev                                        # Frontend hot-reload (port 5173)

# Testing
cargo test --no-default-features                   # Fast tests (~180)
cargo test                                         # Full tests with AVIF (~235)
cargo clippy --no-default-features -- -D warnings  # Linting
npm run type-check && npm run build               # Frontend checks

# Production
cargo build --release --no-default-features       # Release build
npm run build:prod                                 # Production frontend
```

### Development Workflow
1. Terminal 1: `cargo run --no-default-features -- serve` (port 3000)
2. Terminal 2: `npm run dev` (port 5173 with hot-reload, proxies to 3000)
3. Visit http://localhost:5173/

### Pre-Commit Checklist
- `npm run type-check && npm run build`
- `cargo clippy --no-default-features -- -D warnings && cargo fmt --check`
- `cargo test --no-default-features`

## Code Style Guidelines
- No comments unless explicitly requested
- Prefer editing existing files over creating new ones
- Use `thiserror` crate for error types
- Run `cargo fmt` and `cargo clippy` before finalizing
- **Always use `ResolvedState` in handlers** (never `State<AppState>` directly)
- **Frontend code must be TypeScript and React** - never write vanilla JS in templates or HTML files

## Architecture

### Handler Pattern (Multi-Site Support)
```rust
// CORRECT - handles multi-site resolution
pub async fn my_handler(ResolvedState(app_state): ResolvedState) -> impl IntoResponse {
    let secret = app_state.cookie_secret();    // Use accessors
    let url = app_state.base_url();
}

// WRONG - bypasses per-site values
pub async fn my_handler(State(app_state): State<AppState>) -> impl IntoResponse {
    let secret = app_state.config.app.cookie_secret;  // Don't access config directly
}
```

### Key Modules
| Module | Purpose |
|--------|---------|
| `src/gallery/` | Image serving, processing, caching, metadata |
| `src/posts/` | Markdown blog system |
| `src/login/` | Email-based auth, WebAuthn/passkeys |
| `src/storage/` | Pluggable storage (filesystem, S3) |
| `src/user_storage/` | User backends (TOML, SQLite, PostgreSQL, DynamoDB) |
| `src/email/` | Email providers (SES) |
| `src/config/` | Configuration loading and multi-site support |
| `src/site/` | Site management, routing, and reload |
| `src/admin/` | Admin API handlers |
| `tenrankai-config-storage/` | ConfigStorage backends (FileDir, S3) |

### Storage Abstraction
URL-based configuration for flexible backends:
```toml
# Local filesystem (default)
source_directory = "photos"
cache_directory = "cache/main"

# S3 storage
source_directory = "s3://bucket/photos?region=us-west-2"
cache_directory = "s3://bucket/cache?region=us-west-2"

# Multiple template directories (first match wins)
[templates]
directories = ["templates-custom", "templates"]
```

### User Storage Backends
```bash
# CLI with different backends
cargo run -- user list --database users.toml
cargo run -- user list --database sqlite://users.db
cargo run -- user add alice alice@example.com --database postgresql://...

# Migration
cargo run -- user export --database users.toml --output users.json
cargo run -- user import --database sqlite://users.db --input users.json
```

Feature flags: `user-storage-sql`, `user-storage-dynamodb`, `user-storage-all`

## AVIF Support

Optional HDR AVIF support (disabled with `--no-default-features` for faster builds):

**With AVIF enabled:**
- Full HDR encoding/decoding with gain map preservation
- 10-bit encoding, ICC profile preservation
- `avif-debug` CLI command for analysis

**Without AVIF:**
- AVIF files ignored, faster builds
- No complex dependencies (easier on Windows)

```bash
cargo run -- avif-debug photos/image.avif --verbose  # Analyze AVIF
```

## Configuration

Tenrankai uses a two-tier configuration:
1. **Bootstrap config** (`config.toml`): Server settings, email, OpenAI
2. **ConfigStorage** (`config.d/`): Site-specific configuration (galleries, posts, permissions)

### CLI Config Commands
```bash
cargo run -- config init config.d              # Initialize ConfigStorage directory
cargo run -- config list-sites                 # List all sites
cargo run -- config add-site mysite --hostname example.com
cargo run -- config list-galleries default     # List galleries for a site
cargo run -- config add-gallery photos --site default --source photos --url-prefix /gallery
cargo run -- config list-posts default         # List posts configs
cargo run -- config add-posts blog --site default --source posts/blog --url-prefix /blog
```

### Gallery Configuration (in ConfigStorage)
```toml
# config.d/sites/default/galleries/main.toml
name = "main"
url_prefix = "/gallery"
source_directory = "photos"          # Relative to storage_prefix
cache_directory = "cache/main"
images_per_page = 50
jpeg_quality = 85
webp_quality = 85.0
copyright_holder = "Your Name"
image_indexing = "filename"  # or "sequence", "unique_id"
new_threshold_days = 7

[thumbnail]
width = 300
height = 300

[medium]
width = 1200
height = 1200
```

### Image Indexing Modes
- **filename** (default): `/gallery/image/IMG_1234.jpg` - predictable, exposes filenames
- **sequence**: `/gallery/image/1` - clean URLs, unstable across changes
- **unique_id**: `/gallery/image/a8k3m9` - stable, privacy-friendly

### Email Configuration
```toml
[email]
from_address = "noreply@domain.com"
from_name = "Tenrankai"
provider = "ses"
region = "us-east-1"
```

### Access Control
```toml
# In _folder.md frontmatter
+++
[permissions]
public_role = "none"  # Require authentication
[permissions.roles.viewer]
permissions = { can_view = true }
[permissions.user_roles]
alice = "viewer"
+++
```

## Docker

```bash
docker build -t tenrankai:latest .                    # With AVIF (~168 MB)
docker build -f Dockerfile.no-avif -t tenrankai .     # Without AVIF (~130 MB)
```

Multi-arch support (amd64/arm64), runs as non-root user (UID 1001).

## Troubleshooting

| Issue | Solution |
|-------|----------|
| Build issues | `npm run clean && cargo clean && npm run build && cargo build --no-default-features` |
| Port conflict | `cargo run --no-default-features -- serve --port 3001` |
| Email not sending | Check `config.toml`, verify AWS credentials, check SES sender verification |
| Login links not working | Verify `base_url` matches actual URL, check token expiry (10 min) |

## Image Processing

### Sizes
- **Thumbnail**: Small previews (300x300 default)
- **Gallery**: Grid display (800x800 default)
- **Medium**: Detail view with watermark (1200x1200 default)
- **Large**: Full quality, requires auth (1600x1600 default)
- All sizes have @2x variants for retina displays

### Format Handling
- WebP served to supporting browsers (Accept header detection)
- JPEG fallback for others
- PNG preserved for transparency
- AVIF with HDR/gain map preservation (when enabled)
- ICC profiles preserved through entire pipeline

### Caching
- In-memory metadata cache with JSON persistence
- Auto-saves every 5 min or 100 updates
- Version-based refresh on app updates
- Background refresh (default: 60 min interval)

## Authentication

### Flow
1. User enters username at `/_login`
2. Email sent with 10-minute token
3. Click link → session cookie (7-day expiry)

### WebAuthn/Passkeys
- Biometric and hardware key support
- Manage at `/_login/profile`
- Fallback to email authentication

### Security
- Tokens: 32-byte cryptographically random
- Rate limiting: 5 attempts per 5 minutes per IP
- Cookies: HTTPOnly, signed values
