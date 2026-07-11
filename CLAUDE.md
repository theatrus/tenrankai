# Tenrankai Project Documentation

## Project Overview
Tenrankai is a Rust/Axum photo gallery server with React/TypeScript frontend. Features include image resizing, metadata extraction, watermarking, caching, multiple galleries, markdown-based posts, and email-based authentication.

## Quick Reference

### Essential Commands
```bash
# Full builds (frontend + backend via Makefile)
make                   # Dev build (frontend + backend, with AVIF)
make dev-no-avif       # Dev build without AVIF (faster compile)
make release           # Production frontend + release binary
make check             # Lint + test (pre-commit)

# Individual steps
cargo run -- serve                                 # Start server
npm run dev                                        # Frontend hot-reload (port 5173)
npm run build                                      # Build frontend assets
cargo build                                        # Build backend only
cargo build --no-default-features                  # Build backend without AVIF (faster)

# Testing
cargo test                                         # Full tests with AVIF (~235)
cargo test --no-default-features                   # Fast tests without AVIF (~180)
make lint                                          # Lint frontend + backend
```

**Important:** Frontend assets are served from disk, not embedded in the binary.
Changing frontend code does NOT require a Rust recompile. Use `npm run build` or
`make frontend` to rebuild frontend assets independently.

### Development Workflow
1. `make` (first time — builds frontend + backend)
2. Terminal 1: `cargo run -- serve` (port 3000)
3. Terminal 2: `npm run dev` (port 5173 with hot-reload, proxies to 3000)
4. Visit http://localhost:5173/

### Pre-Commit Checklist
- `make check` (or individually:)
- `npm run lint`
- `cargo clippy -- -D warnings && cargo fmt --check`
- `cargo test`

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

### CLI Cache Commands
```bash
cargo run -- cache report -g main                 # Report format coverage
cargo run -- cache cleanup -g main                # Clean up outdated entries
cargo run -- cache list-composites -g main         # List composite preview images

# Invalidate cache (force regeneration) - recursive by default
cargo run -- cache invalidate -g main                              # Entire gallery, all sizes
cargo run -- cache invalidate -g main -p "vacation"                # Folder and subfolders
cargo run -- cache invalidate -g main -t image -p "folder/img.jpg" # Single image
cargo run -- cache invalidate -g main -t composite -p ""           # Composite previews

# Invalidate only specific size classes (thumbnail, gallery, medium, large)
cargo run -- cache invalidate -g main --size gallery --size medium
cargo run -- cache invalidate -g main -t image -p "img.jpg" --size thumbnail --dry-run
```

### Gallery Configuration (in ConfigStorage)
```toml
# config.d/sites/default/galleries/main.toml
name = "main"
url_prefix = "/gallery"
source_directory = "photos"          # Relative to storage_prefix
cache_directory = "cache/main"
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
Applies to gallery folders and posts source directories alike (`_folder.md` in the
directory; the nearest ancestor's file wins for posts). Post viewing requires
`can_view`; the post editor/creator UI and `/api/posts/{name}/source` API require
`can_edit_content`.
```toml
# In _folder.md frontmatter
+++
[permissions]
public_role = "none"  # Require authentication (no role for anonymous users)

# Roles require an explicit `name` matching the table key.
[permissions.roles.viewer]
name = "viewer"
permissions = { can_view = true }

# user_roles is an array of tables: { username, roles = [...] }.
[[permissions.user_roles]]
username = "alice"
roles = ["viewer"]
+++
```

## Docker

```bash
docker build -t tenrankai:latest .                    # With AVIF (~168 MB)
```

Multi-arch support (amd64/arm64), runs as non-root user (UID 1001).

## Troubleshooting

| Issue | Solution |
|-------|----------|
| Build issues | `make clean-all && make` |
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
