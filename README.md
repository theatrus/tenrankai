# Tenrankai

[![CI](https://github.com/theatrus/tenrankai/actions/workflows/ci.yml/badge.svg)](https://github.com/theatrus/tenrankai/actions/workflows/ci.yml)
[![Security Audit](https://github.com/theatrus/tenrankai/actions/workflows/security.yml/badge.svg)](https://github.com/theatrus/tenrankai/actions/workflows/security.yml)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.89.0%2B-orange.svg)](https://www.rust-lang.org)
[![Dependencies](https://deps.rs/repo/github/theatrus/tenrankai/status.svg)](https://deps.rs/repo/github/theatrus/tenrankai)
[![GitHub release](https://img.shields.io/github/release/theatrus/tenrankai.svg)](https://github.com/theatrus/tenrankai/releases)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/theatrus/tenrankai/pulls)

A high-performance web-based photo gallery server written in Rust using the Axum web framework. Tenrankai provides a responsive gallery interface with automatic image resizing, metadata extraction, and intelligent caching.

It's a gallery, CMS, and blog platform relying on nothing more than folders and files. Simply drop files in, or even use SyncThing to keep your gallery or website up to date.

The name "Tenrankai" (展覧会) is Japanese for "exhibition" or "gallery show", reflecting the project's purpose as a platform for displaying photographic collections.

## Features

- **Responsive Web Gallery**: Mobile-friendly masonry layout that adapts to different screen sizes
- **Automatic Image Processing**: On-the-fly image resizing with caching for multiple sizes
- **High-DPI Support**: Automatic @2x image generation for retina displays
- **Metadata Extraction**: EXIF data parsing including camera info, GPS coordinates, and capture dates
- **Smart Caching**: Persistent metadata caching and image cache with background refresh
- **Multiple Format Support**: Automatic WebP delivery for supported browsers with JPEG fallback, PNG support with transparency preservation
- **Color Profile Preservation**: Full ICC profile support for JPEG, PNG, and WebP, including Display P3
- **Copyright Watermarking**: Intelligent watermark placement with automatic text color selection
- **Markdown Support**: Folder descriptions and image captions via markdown files
- **Hidden Folders**: Hide folders from listings while keeping them accessible via direct URL
- **New Image Highlighting**: Configurable highlighting of recently modified images
- **Multiple Blog Systems**: Support for multiple independent blog/posts systems with markdown
- **Dark Theme Code Blocks**: Optimized code block styling for readability in dark theme
- **Email-based Authentication**: Secure passwordless login system with email verification links
- **User Authentication**: Optional user authentication system with rate limiting
- **Email Provider Support**: Pluggable email provider system with Amazon SES and null providers
- **Cascading Static Files**: Support for multiple static directories with file overlay precedence
- **WebAuthn/Passkey Support**: Modern passwordless authentication with biometric login

## Installation

### Prerequisites

- Rust 1.89.0 or later (automatically managed by rust-toolchain.toml)
- DejaVuSans.ttf font file (required for watermarking)

### Building from Source

```bash
git clone https://github.com/yourusername/tenrankai.git
cd tenrankai
cargo build --release
```

The project includes a `rust-toolchain.toml` file that will automatically download and use Rust 1.89.0 when you run cargo commands. This ensures consistent builds across all development environments.

## Configuration

Create a `config.toml` file in the project root. See `config.example.toml` for a complete example:

```toml
[server]
host = "127.0.0.1"
port = 3000

[app]
name = "My Gallery"
cookie_secret = "change-me-in-production-use-a-long-random-string"  # Required: Used for signing auth cookies
base_url = "https://yourdomain.com"
user_database = "users.toml"  # Optional: Enable user authentication

# Gallery configuration (multiple galleries supported)
[[galleries]]
name = "main"
url_prefix = "/gallery"
source_directory = "photos/main"
cache_directory = "cache/main"
images_per_page = 50
jpeg_quality = 85
webp_quality = 85.0
copyright_holder = "Your Name"  # Optional: Add watermark to medium-sized images

[[galleries]]
name = "portfolio"
url_prefix = "/portfolio"
source_directory = "photos/portfolio"
cache_directory = "cache/portfolio"
images_per_page = 20
jpeg_quality = 90
webp_quality = 90.0
copyright_holder = "Your Portfolio Name"

[templates]
directory = "templates"

[static_files]
# Single directory (backward compatible)
directories = "static"
# OR multiple directories with precedence (first overrides later)
# directories = ["static-custom", "static"]

# Email configuration (required for login emails)
[email]
from_address = "noreply@yourdomain.com"
from_name = "My Gallery"
provider = "ses"  # Options: "ses" for production, "null" for development
region = "us-east-1"
# Optional: specify AWS credentials (otherwise uses AWS SDK default chain)
# access_key_id = "your-access-key"
# secret_access_key = "your-secret-key"
```

### Key Configuration Options

**Gallery Configuration:**
- `name`: Unique identifier for the gallery (required for multiple galleries)
- `url_prefix`: URL path where the gallery will be accessible (e.g., `/gallery`, `/portfolio`)
- `source_directory`: Path to your photo directory
- `cache_directory`: Where processed images and metadata are cached
- `images_per_page`: Number of images to display per page
- `new_threshold_days`: Days to consider an image "new" (remove to disable)
- `pregenerate_cache`: Pre-generate all image sizes on startup/refresh
- `jpeg_quality`: JPEG compression quality (1-100)
- `webp_quality`: WebP compression quality (0.0-100.0)
- `approximate_dates_for_public`: Show only month/year capture dates to non-authenticated users
- `gallery_template`: Custom template for gallery pages (default: "modules/gallery.html.liquid")
- `image_detail_template`: Custom template for image detail pages (default: "modules/image_detail.html.liquid")
- `copyright_holder`: Copyright holder name for watermarking medium-sized images (optional)

**Static Files Configuration:**
- `directories`: Static file directories (string or array)
  - Single directory: `directories = "static"`
  - Multiple directories: `directories = ["static-custom", "static"]`
  - Files in earlier directories override files in later directories

**Email Configuration:**
- `from_address`: Email address to send from (required)
- `from_name`: Display name for the sender (optional)
- `reply_to`: Reply-to address (optional)
- `provider`: Email provider to use ("ses" or "null")
- **Amazon SES Options:**
  - `region`: AWS region where SES is configured (optional, uses SDK default)
  - `access_key_id`: AWS access key (optional, uses SDK default chain)
  - `secret_access_key`: AWS secret key (optional, uses SDK default chain)
- **Null Provider Options:**
  - For development/testing: logs emails instead of sending them
  - No additional configuration required

## Usage

### Running the Server

```bash
# Using default config.toml
cargo run --release

# With custom configuration
cargo run --release -- --config /path/to/config.toml

# Specify host and port
cargo run --release -- --host 0.0.0.0 --port 8080

# Enable debug logging
cargo run --release -- --log-level debug
```

### Command Line Options

- `--config <path>`: Path to configuration file (default: config.toml)
- `--host <address>`: Override configured host address
- `--port <number>`: Override configured port
- `--log-level <level>`: Set logging level (trace, debug, info, warn, error)
- `--quit-after <seconds>`: Auto-shutdown after specified seconds (useful for testing)

## Gallery Features

### Multiple Galleries

Tenrankai supports multiple independent gallery instances, each with its own:
- Source directory for photos
- URL prefix for web access
- Cache directory and settings
- Templates (customizable per gallery)
- Image quality and pagination settings

Example URLs for different galleries:
- Main gallery: `http://localhost:8080/gallery/`
- Portfolio: `http://localhost:8080/portfolio/`
- Archive: `http://localhost:8080/photos/archive/`

### Gallery Organization

#### Directory Structure

```
photos/
├── vacation-2024/
│   ├── _folder.md          # Folder description (markdown)
│   ├── IMG_001.jpg
│   ├── IMG_001.md          # Image caption (markdown)
│   └── IMG_002.jpg
└── landscapes/
    ├── _folder.md
    └── sunset.jpg
```

### Markdown Support

- `_folder.md`: Place in any directory to add a description that appears at the top of the gallery page
- `<imagename>.md`: Create alongside any image to add a caption (e.g., `sunset.jpg` → `sunset.md`)

#### Advanced Folder Configuration

Folders can use TOML front matter in `_folder.md` files for advanced configuration:

```markdown
+++
hidden = true
title = "Private Collection"
require_auth = true
allowed_users = ["alice", "bob"]
+++

# Optional Markdown Content

This folder is hidden from gallery listings but remains accessible via direct URL.
```

**Configuration Options:**
- `hidden = true`: Hides the folder from gallery listings, previews, and counts (but allows direct access)
- `title = "Custom Name"`: Override the folder display name
- `require_auth = true`: Require user authentication to access this folder
- `allowed_users = ["user1", "user2"]`: Restrict access to specific users (implies require_auth)

**Hidden Folders:**
- Do not appear in gallery navigation or listings
- Are excluded from gallery preview images and image counts
- Remain fully accessible if you know the direct URL
- Perfect for private collections or work-in-progress galleries

**Access Control:**
- Access restrictions are hierarchical (parent folder restrictions apply to children)
- Users must be authenticated to access folders with `require_auth = true`
- Only listed users can access folders with `allowed_users` specified
- Access control applies to folder browsing, image viewing, and API endpoints

## Posts System

Tenrankai includes a flexible posts/blog system that supports multiple independent collections:

### Post Format

Posts are markdown files with TOML front matter:

```markdown
+++
title = "My Post Title"
summary = "A brief summary of the post"
date = "2024-08-24"
+++

# Post Content

Your markdown content here...
```

### Multiple Post Systems

Configure multiple independent post systems in your `config.toml`:

```toml
[[posts]]
name = "blog"
source_directory = "posts/blog"
url_prefix = "/blog"
posts_per_page = 20
refresh_interval_minutes = 30  # Auto-refresh posts every 30 minutes

[[posts]]
name = "stories"
source_directory = "posts/stories"
url_prefix = "/stories"
posts_per_page = 10
```

Each system has its own:
- Source directory for markdown files
- URL prefix for web access
- Templates (customizable)
- Posts per page setting
- Optional automatic refresh interval for detecting new/changed posts

### Features

- Full CommonMark support with extensions (tables, strikethrough, footnotes)
- Automatic HTML generation from markdown
- Chronological sorting (newest first)
- Pagination support
- Subdirectory organization (URL reflects directory structure)
- Dynamic refresh via API
- Automatic periodic refresh (configurable interval)
- Individual post reloading when files change
- Dark theme optimized code blocks with syntax highlighting
- Responsive post layout for mobile and desktop

## Image Sizes

Tenrankai automatically generates multiple sizes for each image:

- **Thumbnail**: Small preview images for gallery grid
- **Gallery**: Standard viewing size used in the gallery layout
- **Medium**: Larger size with optional copyright watermark
- **Large**: Full quality (requires authentication)

All sizes support @2x variants for high-DPI displays.

### Color Profile Support

Tenrankai preserves ICC color profiles throughout the image processing pipeline:

- **JPEG**: Extracts and preserves ICC profiles from source images
- **PNG**: Extracts ICC profiles from iCCP chunks and preserves transparency
- **WebP**: Embeds ICC profiles using libwebp-sys WebPMux API
- **Wide Gamut**: Full support for Display P3, Adobe RGB, and other color spaces
- **Watermarking**: Color profiles maintained even when adding copyright notices

This ensures accurate color reproduction across all devices and browsers that support color management. PNG images are always served as PNG to preserve transparency and avoid quality loss.

## Authentication

Tenrankai supports both email-based and WebAuthn/Passkey authentication for secure access:

1. **User Management**: Users are managed via a TOML file (`users.toml`)
   - Copy `users.toml.example` to `users.toml`
   - Add users with their username and email address
   - No self-registration - admin manages all users
   - When `user_database` is not configured, the system runs without authentication

2. **Login Flow**:
   - User visits `/_login` and enters their username or email address
   - System sends an email with a secure login link
   - User clicks the link to authenticate
   - Session is maintained via secure HTTPOnly cookies
   - Rate limiting prevents brute force attacks (5 attempts per 5 minutes per IP)

3. **User Administration**:
   ```bash
   # List all users
   cargo run -- user list
   
   # Add a new user
   cargo run -- user add --username alice --email alice@example.com
   
   # Remove a user
   cargo run -- user remove --username alice
   
   # Update user email
   cargo run -- user update --username alice --email newemail@example.com
   ```

### WebAuthn/Passkey Authentication

Tenrankai supports modern WebAuthn/Passkey authentication for passwordless login:

**Prerequisites for WebAuthn**:
- Configure `base_url` in your `config.toml` (required for WebAuthn to work)
- HTTPS connection (required by WebAuthn specification, except for localhost)

**Features**:
- **Biometric Authentication**: Fingerprint, face recognition, or hardware security keys
- **Cross-Device Sync**: Passkeys sync across devices via platform providers (iCloud, Google, etc.)
- **Fallback Support**: Email-based login remains available when WebAuthn is unavailable
- **Multiple Passkeys**: Users can register multiple passkeys per account

**Passkey Management**:
- After email login, users are prompted to enroll a passkey for faster future logins
- Users can view their profile and manage passkeys at `/_login/profile`
- Profile page shows username, email, and registered passkeys
- Passkeys can be removed through the profile interface
- New passkeys can be enrolled from the profile page

**Login Flow with WebAuthn**:
1. User visits `/_login` and enters their username
2. If passkeys are available, user can choose passkey authentication or email fallback
3. For passkey login: Browser prompts for biometric/hardware authentication
4. For email login: Traditional email link is sent
5. After successful email login, user is offered passkey enrollment

**Email Configuration**: Configure an email provider in your `config.toml`:
- **Production**: Use `provider = "ses"` with Amazon SES for reliable email delivery
- **Development/Testing**: Use `provider = "null"` to log emails to console instead of sending them
- **No Configuration**: Without email configuration, login URLs will be logged to the server console

### Running Without Authentication

To run Tenrankai without user authentication:
1. Remove or comment out the `user_database` line in your config.toml
2. The system will allow access to all features without login
3. The user menu will not appear in the interface

This is useful for:
- Personal use on a private network
- Development and testing
- Public galleries where authentication isn't needed

## API Endpoints

### Gallery Endpoints
- `GET /gallery` - Gallery root
- `GET /gallery/{path}` - Browse specific folder
- `GET /gallery/image/{path}?size={size}` - Get resized image
- `GET /gallery/detail/{path}` - View image details page
- `GET /api/gallery/preview` - Get random gallery preview images

### Posts Endpoints (configurable prefix)
- `GET /{prefix}` - List posts with pagination
- `GET /{prefix}/{slug}` - View individual post
- `POST /api/posts/{name}/refresh` - Refresh posts cache

### Authentication Endpoints
- `GET /_login` - Login page
- `POST /_login/request` - Request login email (accepts username or email)
- `GET /_login/verify?token={token}` - Verify login token
- `GET /_login/logout` - Logout and clear session
- `GET /_login/profile` - User profile and passkey management page
- `GET /api/verify` - Check authentication status (JSON)

### WebAuthn/Passkey Endpoints
- `GET /_login/passkey-enrollment` - Passkey enrollment page (post-login)
- `POST /api/webauthn/check-passkeys` - Check if user has registered passkeys
- `POST /api/webauthn/register/start` - Start passkey registration
- `POST /api/webauthn/register/finish/{reg_id}` - Complete passkey registration
- `POST /api/webauthn/authenticate/start` - Start passkey authentication
- `POST /api/webauthn/authenticate/finish/{auth_id}` - Complete passkey authentication
- `GET /api/webauthn/passkeys` - List user's registered passkeys
- `DELETE /api/webauthn/passkeys/{passkey_id}` - Delete a passkey
- `PUT /api/webauthn/passkeys/{passkey_id}/name` - Rename a passkey

### Utility Endpoints
- `POST /api/refresh-static-versions` - Refresh static file version cache (authenticated)

## Performance

Tenrankai includes several performance optimizations:

- Persistent metadata caching reduces file system access
- Background cache refresh keeps data fresh without blocking requests
- Concurrent image processing with rate limiting
- Automatic cache pre-generation option for instant loading
- Browser-based caching headers for processed images

## Template Structure

Templates are organized into three directories:

```
templates/
├── pages/              # Regular page templates
│   ├── index.html.liquid
│   ├── about.html.liquid
│   ├── contact.html.liquid
│   └── 404.html.liquid
├── modules/            # Module-specific templates
│   ├── gallery.html.liquid
│   ├── image_detail.html.liquid
│   ├── posts_index.html.liquid
│   └── post_detail.html.liquid
└── partials/           # Reusable template components
    ├── _header.html.liquid
    ├── _footer.html.liquid
    └── _gallery_preview.html.liquid
```

All templates use the Liquid templating language and support includes for reusable components.

## Static Files

Tenrankai supports cascading static directories, allowing you to overlay custom files over default ones:

### Configuration

```toml
[static_files]
# Single directory (backward compatible)
directories = "static"

# OR multiple directories with precedence
directories = ["static-custom", "static-default"]
```

### File Precedence

When multiple directories are configured:
- Files in earlier directories take precedence over files in later directories
- If `logo.png` exists in both `static-custom` and `static-default`, the one from `static-custom` is served
- Files unique to any directory are accessible normally
- Useful for:
  - Custom themes that override default assets
  - Environment-specific configurations
  - Gradual migrations between asset sets

### Required Files

Place the following in one of your static directories:

- `DejaVuSans.ttf` - Required for copyright watermarking
- `favicon.svg` - Used to generate favicon.ico and PNG variants (optional)
- `robots.txt` - Custom robots file (optional, defaults provided)
- Any other static assets referenced in templates

The system will search all configured directories in order to find these files.

## Logging

Control logging verbosity with the `RUST_LOG` environment variable or `--log-level` flag:

```bash
# Examples
RUST_LOG=debug cargo run
cargo run -- --log-level trace
```

## Development

Tenrankai is under active development with a comprehensive codebase and documentation.

### Documentation

- **[CONTRIBUTING.md](CONTRIBUTING.md)**: Development setup, code organization, and contribution guidelines
- **[API.md](API.md)**: Complete API reference with examples
- **[CHANGELOG.md](CHANGELOG.md)**: Detailed changelog of recent improvements
- **[README.md](README.md)**: This file - user guide and configuration reference

### Recent Major Features

- ✅ **WebAuthn/Passkey Authentication**: Biometric and hardware key login support
- ✅ **Gallery Access Control**: Folder-level authentication and user restrictions
- ✅ **User Profile Page**: Centralized passkey management interface
- ✅ **Cascading Static Directories**: Multi-directory asset management with precedence
- ✅ **Null Email Provider**: Development-friendly email logging
- ✅ **Enhanced Asset Management**: Cache busting with automatic versioning
- ✅ **Improved Authentication Flow**: Return URL support and passkey enrollment
- ✅ **Simplified TOML Database**: Clean user database format using serde

### Planned Features

- Additional email providers (SendGrid, SMTP, etc.)
- Full-text search across galleries and posts
- Video file support with thumbnail generation
- Tag-based filtering and organization
- User roles and permissions system
- Gallery-specific access controls

### Contributing

Contributions are welcome! Please:

1. Read [CONTRIBUTING.md](CONTRIBUTING.md) for development setup
2. Check existing issues or create new ones for bugs/features
3. Follow the established code style and testing practices
4. Submit pull requests with clear descriptions

### Architecture Highlights

- **Async Rust**: Built on Tokio with Axum web framework
- **Thread-Safe Operations**: Arc<RwLock<T>> for concurrent access
- **Comprehensive Testing**: 60+ unit tests and integration tests
- **Modular Design**: Clean separation of concerns across modules
- **Configuration-Driven**: Flexible TOML-based configuration system

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.
