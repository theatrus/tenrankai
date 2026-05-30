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
- **Optional AVIF Support**: Full HDR AVIF encoding/decoding with gain map preservation for HDR tone mapping (when built with AVIF feature)
- **Color Profile Preservation**: Full ICC profile support for JPEG, PNG, WebP, and AVIF, including Display P3
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
- **Pluggable User Storage**: Store users in TOML files, SQLite, PostgreSQL, or DynamoDB
- **S3 Storage Support**: Store galleries, caches, templates, posts, and static files on Amazon S3

## Installation

### Prerequisites

- Rust 1.89.0 or later (automatically managed by rust-toolchain.toml)
- DejaVuSans.ttf font file (required for watermarking)

### Building from Source

```bash
git clone https://github.com/yourusername/tenrankai.git
cd tenrankai

# Default build with AVIF support
cargo build --release

# Build without AVIF for easier compilation (especially on Windows)
cargo build --release --no-default-features
```

The project includes a `rust-toolchain.toml` file that will automatically download and use Rust 1.89.0 when you run cargo commands. This ensures consistent builds across all development environments.

### Docker

Tenrankai includes production-ready Docker support with optimized multi-stage builds.

#### Quick Start

```bash
# Pull from GitHub Container Registry (when available)
docker pull ghcr.io/theatrus/tenrankai:latest

# Or build locally
docker build -t tenrankai:latest .

# Run the container
docker run -d \
  --name tenrankai \
  -p 8080:8080 \
  -v ./config.toml:/app/config.toml:ro \
  -v ./photos:/app/photos:ro \
  -v ./cache:/app/cache \
  tenrankai:latest

# Or use docker-compose (recommended for complex setups)
# See docker-compose.example.yml for a complete example with all options
docker-compose up -d
```

#### Docker Image

The Docker image (~168 MB) includes full AVIF support with HDR and gain maps, using an optimized release build with a ~45 MB binary. The container runs as a non-root user for security.

#### Volume Mounts

The container expects these volumes:
- `/app/config.toml` - Main configuration file (read-only recommended)
- `/app/photos` - Photo directories (read-only recommended) 
- `/app/cache` - Image cache directory (read-write)
- `/app/users.toml` - Optional: User database for authentication
- `/app/static` - Optional: Custom static assets
- `/app/templates` - Optional: Custom templates (see below for override examples)

#### Environment Variables

```bash
# Set custom log level
docker run -e RUST_LOG=debug ...

# Override configuration
docker run -e TENRANKAI_HOST=0.0.0.0 -e TENRANKAI_PORT=3000 ...
```

#### Security Considerations

- Container runs as non-root user (UID 1001)
- Mount config and photos as read-only (`:ro`)
- Never include secrets in the image
- Use environment variables or mounted files for sensitive data

#### Template Overrides

Tenrankai supports multiple template directories with a precedence system, allowing you to override specific templates while using the built-in defaults.

**Basic Override Example:**
```bash
# Create custom templates directory
mkdir -p custom-templates/partials

# Create a custom header
cat > custom-templates/partials/_header.html.liquid << 'EOF'
<!DOCTYPE html>
<html>
<head>
    <title>My Custom Gallery</title>
    <style>
        header { background-color: #2c3e50; color: white; }
    </style>
</head>
<body>
    <header>
        <h1>My Photography Portfolio</h1>
    </header>
    <main>
EOF

# Update config.toml to use both directories
cat > config.toml << 'EOF'
[templates]
# Custom templates override built-in ones
directories = ["custom-templates", "templates"]
EOF

# Run with Docker
docker run -d \
  -p 3000:3000 \
  -v ./config.toml:/app/config.toml:ro \
  -v ./custom-templates:/app/custom-templates:ro \
  -v ./photos:/app/photos:ro \
  -v ./cache:/app/cache \
  tenrankai:latest
```

**Advanced Theme Example:**
```bash
# Create a complete theme override
mkdir -p themes/dark/{partials,pages,modules}

# Run with theme
docker run -d \
  -p 3000:3000 \
  -v ./config-with-theme.toml:/app/config.toml:ro \
  -v ./themes:/app/themes:ro \
  -v ./photos:/app/photos:ro \
  -v ./cache:/app/cache \
  tenrankai:latest
```

Where `config-with-theme.toml` contains:
```toml
[templates]
directories = ["themes/dark", "templates"]
```

**How It Works:**
- Templates are searched in order - first match wins
- You only need to override the specific templates you want to customize
- Missing templates fall back to the next directory
- Partials follow the same rules, allowing mix-and-match

**Common Override Patterns:**
- **Brand customization**: Override `_header.html.liquid` and `_footer.html.liquid`
- **Layout changes**: Override `modules/gallery.html.liquid`
- **Style updates**: Override partials while keeping page structure
- **A/B testing**: Switch between template sets via config

### Build Options

**AVIF Feature Flag**: Tenrankai includes optional AVIF support that can be disabled for easier builds on platforms where AVIF dependencies are difficult to compile.

- **With AVIF (Default)**: Full HDR AVIF support including gain maps, ICC profiles, and advanced color management
- **Without AVIF**: AVIF files are ignored, resulting in smaller binaries and simpler dependency requirements

**Platform Recommendations**:
- **Linux/macOS**: Use default build with AVIF support
- **Windows**: Consider using `--no-default-features` if you encounter build issues with AVIF dependencies

## Configuration

Tenrankai uses a two-tier configuration system:

1. **Bootstrap config** (`config.toml`): Server settings, email, OpenAI - static settings
2. **ConfigStorage** (`config.d/`): Site-specific configuration that can be managed via CLI or admin API

### Quick Setup

```bash
# Initialize ConfigStorage directory with a default site
cargo run -- config init config.d

# Add galleries and posts to your site
cargo run -- config add-gallery photos --site default --source photos --url-prefix /gallery
cargo run -- config add-posts blog --site default --source posts/blog --url-prefix /blog
```

### Bootstrap Configuration (config.toml)

```toml
[server]
host = "127.0.0.1"
port = 3000

[app]
name = "My Gallery"
config_storage = "config.d"  # Path to ConfigStorage directory

# Email configuration (required for login emails)
[email]
from_address = "noreply@yourdomain.com"
from_name = "My Gallery"
provider = "ses"  # Options: "ses" for production, "null" for development
region = "us-east-1"
```

### ConfigStorage Directory Structure

```
config.d/
  sites/
    default/
      site.toml              # Site settings (hostnames, templates, static dirs)
      galleries/
        main.toml            # Gallery configuration
        portfolio.toml
      posts/
        blog.toml            # Posts/blog configuration
      permissions.toml       # Roles and user permissions
```

### Site Configuration (site.toml)

```toml
hostnames = ["localhost", "example.com"]
templates = ["templates"]
static_files = ["static"]
base_url = "https://example.com"
cookie_secret = "change-me-in-production"
user_database = "users.toml"
storage_prefix = "/data/sites/default"  # Base path for galleries/posts
```

### Gallery Configuration

```toml
# config.d/sites/default/galleries/main.toml
name = "main"
url_prefix = "/gallery"
source_directory = "photos"       # Relative to storage_prefix
cache_directory = "cache/main"
jpeg_quality = 85
webp_quality = 85.0
copyright_holder = "Your Name"

[thumbnail]
width = 300
height = 300

[medium]
width = 1200
height = 1200
```

### Posts Configuration

```toml
# config.d/sites/default/posts/blog.toml
name = "blog"
source_directory = "posts/blog"   # Relative to storage_prefix
url_prefix = "/blog"
posts_per_page = 20
refresh_interval_minutes = 30
```

### CLI Config Commands

```bash
cargo run -- config init <path>                    # Initialize ConfigStorage
cargo run -- config list-sites                     # List all sites
cargo run -- config add-site <name> --hostname <host>
cargo run -- config list-galleries <site>          # List galleries
cargo run -- config add-gallery <name> --site <site> --source <dir> --url-prefix <prefix>
cargo run -- config list-posts <site>              # List posts configs
cargo run -- config add-posts <name> --site <site> --source <dir> --url-prefix <prefix>
```

### Key Configuration Options

**Site Configuration (site.toml):**
- `hostnames`: List of hostnames this site responds to
- `templates`: Template directory paths (first match wins)
- `static_files`: Static file directory paths
- `base_url`: Public URL for the site (required for WebAuthn)
- `cookie_secret`: Secret for signing session cookies
- `user_database`: Path to user database file
- `storage_prefix`: Base path for galleries/posts (security boundary)

**Gallery Configuration:**
- `name`: Unique identifier for the gallery
- `url_prefix`: URL path (e.g., `/gallery`, `/portfolio`)
- `source_directory`: Photo directory (relative to storage_prefix)
- `cache_directory`: Cache directory (relative to storage_prefix)
- `new_threshold_days`: Days to consider an image "new"
- `jpeg_quality`: JPEG compression quality (1-100)
- `webp_quality`: WebP compression quality (0.0-100.0)
- `copyright_holder`: Name for watermarking (optional)
- `gallery_template`: Custom template (default: "modules/gallery.html.liquid")
- `image_detail_template`: Custom template (default: "modules/image_detail.html.liquid")

**Email Configuration (config.toml):**
- `from_address`: Email address to send from (required)
- `from_name`: Display name for the sender (optional)
- `provider`: Email provider ("ses" or "null")
- **Amazon SES**: `region`, optional `access_key_id`, `secret_access_key`
- **Null Provider**: Logs emails to console for development

### S3 Storage Configuration

Tenrankai supports Amazon S3 for storing galleries, caches, templates, posts, and static files. Use URL-based configuration with the `s3://` scheme:

```toml
# Gallery with S3 source and cache
[[galleries]]
name = "main"
source_directory = "s3://mybucket/photos?region=us-west-2"
cache_directory = "s3://mybucket/cache/main?region=us-west-2"

# Static files from S3 with signed URL redirects
[static_files]
directories = ["s3://mybucket/static?region=us-west-2"]
use_redirects = true  # Redirect to presigned URLs for direct S3 download

# Templates from S3 with local override
[templates]
directories = ["templates-local", "s3://mybucket/templates?region=us-west-2"]

# Posts from S3
[[posts]]
name = "blog"
source_directory = "s3://mybucket/posts?region=us-west-2"
```

**S3 URL Format:**
- Basic: `s3://bucket/prefix`
- With region: `s3://bucket/prefix?region=us-west-2`
- With custom endpoint (MinIO): `s3://bucket/prefix?endpoint=http://localhost:9000`

**AWS Credentials:**
The S3 backend uses the AWS SDK default credential chain:
1. Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`)
2. AWS credentials file (`~/.aws/credentials`)
3. IAM role credentials (for EC2, ECS, Lambda)

**Hybrid Configurations:**
Mix local and S3 storage for optimal performance:
```toml
# Local source for fast processing, S3 cache for CDN distribution
[[galleries]]
source_directory = "photos"  # Local filesystem
cache_directory = "s3://mybucket/cache?region=us-west-2"  # S3 for cached images
```

## Usage

### Running the Server

```bash
# Using default config.toml with AVIF support
cargo run --release -- serve

# Run without AVIF support (faster builds, especially on Windows)
cargo run --release --no-default-features -- serve

# With custom configuration
cargo run --release -- --config /path/to/config.toml serve

# Specify host and port
cargo run --release -- serve --host 0.0.0.0 --port 8080

# Enable debug logging
cargo run --release -- --log-level debug serve
```

### Command Line Options

Global options (before the subcommand):
- `--config <path>`: Path to configuration file (default: config.toml)
- `--log-level <level>`: Set logging level (trace, debug, info, warn, error)

Serve command options:
- `--host <address>`: Override configured host address
- `--port <number>`: Override configured port
- `--quit-after <seconds>`: Auto-shutdown after specified seconds (useful for testing)

### Utility Commands

#### AVIF Debug Command

Analyze AVIF files to inspect their HDR properties, color spaces, and gain maps:

**Note**: This command is only available when building with AVIF support (default build).

```bash
# Basic analysis
cargo run -- avif-debug path/to/image.avif

# Detailed technical information
cargo run -- avif-debug path/to/image.avif --verbose
```

This command displays:
- Image dimensions and file size
- Color space properties (primaries, transfer characteristics, matrix coefficients)
- HDR detection results
- Gain map presence and parameters
- CLLI (Content Light Level Information) data
- ICC profile information
- Detailed HDR detection logic (with --verbose)

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

# Optional: Configure folder-specific permissions
[permissions]
# Remove public access for this folder
public_role = "none"

# Define a custom role for this folder
[permissions.roles.family]
name = "family"
permissions = { can_view = true, can_download_original = true, can_add_comments = true }

# Assign users to the family role
[[permissions.user_roles]]
username = "alice"
roles = ["family"]

[[permissions.user_roles]]
username = "bob"
roles = ["family"]
+++

# Optional Markdown Content

This folder is hidden from gallery listings but remains accessible via direct URL.
```

**Configuration Options:**
- `hidden = true`: Hides the folder from gallery listings, previews, and counts (but allows direct access)
- `title = "Custom Name"`: Override the folder display name
- `permissions`: Configure folder-specific access control with flexible roles

**Hidden Folders:**
- Do not appear in gallery navigation or listings
- Are excluded from gallery preview images and image counts
- Remain fully accessible if you know the direct URL
- Perfect for private collections or work-in-progress galleries

**Access Control:**
- Uses flexible role-based permissions (see Permission System below)
- Folder permissions override gallery-level permissions
- Access restrictions are hierarchical (parent folder restrictions apply to children)
- Fine-grained control over viewing, downloading, metadata, and more

## Permission System

Tenrankai uses a flexible role-based access control (RBAC) system that allows fine-grained control over who can view, download, and interact with your galleries.

### Key Features

- **Flexible Roles**: Define custom roles with specific permissions
- **Role Inheritance**: Roles can inherit permissions from other roles
- **Multi-Level Control**: Set permissions at both gallery and folder levels
- **Fine-Grained Permissions**: Control viewing, downloading, metadata, comments, and more
- **User-Friendly Configuration**: Simple TOML-based configuration

### Available Permissions

- **Viewing**: `can_view` - Basic viewing access
- **Information Display**:
  - `can_see_technical_details` - Camera settings, EXIF data
  - `can_see_exact_dates` - Exact capture dates (vs approximate)
  - `can_see_location` - GPS coordinates and map links
- **Downloading**:
  - `can_download_medium` - Medium-sized images
  - `can_download_large` - Large images
  - `can_download_original` - Original full-resolution files
- **Metadata & Interaction**:
  - `can_read_metadata` - View comments, tags, picks
  - `can_add_comments` - Add new comments
  - `can_edit_own_comments` - Edit their own comments
  - `can_delete_own_comments` - Delete their own comments
  - `can_set_picks` - Set pick/reject status
  - `can_add_tags` - Add tags to images
- **Moderation**:
  - `can_edit_any_comments` - Edit anyone's comments
  - `can_delete_any_comments` - Delete anyone's comments
- **Full Control**: `owner_access` - Bypass all restrictions

### Site-Level Permissions

Configure permissions in ConfigStorage:

```toml
# config.d/sites/default/permissions.toml

# Role for unauthenticated users (omit for no public access)
public_role = "viewer"

# Default role for authenticated users without specific assignments
default_authenticated_role = "contributor"

# Define custom roles
[roles.viewer]
permissions = { can_view = true, can_download_medium = true }

[roles.contributor]
inherits = "viewer"  # Inherit all viewer permissions
permissions = {
    can_see_technical_details = true,
    can_see_exact_dates = true,
    can_see_location = true,
    can_download_large = true,
    can_read_metadata = true,
    can_add_comments = true,
    can_edit_own_comments = true,
    can_set_picks = true
}

[roles.admin]
permissions = { owner_access = true }  # Full access

# Assign roles to users (array of tables: { username, roles = [...] })
[[user_roles]]
username = "alice"
roles = ["admin"]

[[user_roles]]
username = "bob"
roles = ["contributor"]
```

Permissions can also be managed via the admin API at `/_admin/api/sites/{site}/permissions`.

### Folder-Level Permissions

Override gallery permissions for specific folders in `_folder.md`:

```markdown
+++
title = "Private Collection"

[permissions]
# Remove public access for this folder
public_role = "none"

# Custom role for this folder
[permissions.roles.family]
name = "family"
permissions = { 
    can_view = true,
    can_download_original = true,
    can_add_comments = true
}

# Assign users
[[permissions.user_roles]]
username = "mom"
roles = ["family"]
+++
```

### Common Patterns

**Public Portfolio** - View only, no downloads:
```toml
public_role = "viewer"
[galleries.permissions.roles.viewer]
permissions = { can_view = true }
```

**Client Access** - View and download, no comments:
```toml
public_role = "none"  # No public access
[[galleries.permissions.user_roles]]
username = "client1"
roles = ["client"]

[galleries.permissions.roles.client]
permissions = { 
    can_view = true,
    can_download_medium = true,
    can_see_technical_details = true
}
```

**Team Collaboration** - Full interaction for team:
```toml
default_authenticated_role = "team_member"

[galleries.permissions.roles.team_member]
permissions = {
    can_view = true,
    can_download_original = true,
    can_read_metadata = true,
    can_add_comments = true,
    can_set_picks = true,
    can_add_tags = true
}
```

### Permission Resolution

1. **User-specific roles** are checked first (both folder and gallery level)
2. **Default authenticated role** applies if no specific roles found
3. **Public role** applies to unauthenticated users
4. **Folder permissions** override gallery permissions
5. **Multiple roles** are merged with OR logic (most permissive wins)
6. **Owner access** bypasses all restrictions

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

Configure multiple post systems in ConfigStorage:

```toml
# config.d/sites/default/posts/blog.toml
name = "blog"
source_directory = "posts/blog"    # Relative to storage_prefix
url_prefix = "/blog"
posts_per_page = 20
refresh_interval_minutes = 30

# config.d/sites/default/posts/stories.toml
name = "stories"
source_directory = "posts/stories"
url_prefix = "/stories"
posts_per_page = 10
```

Or use the CLI:
```bash
cargo run -- config add-posts blog --site default --source posts/blog --url-prefix /blog
cargo run -- config add-posts stories --site default --source posts/stories --url-prefix /stories
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

### Color Profile and HDR Support

Tenrankai preserves ICC color profiles and HDR metadata throughout the image processing pipeline:

- **JPEG**: Extracts and preserves ICC profiles from source images
- **PNG**: Extracts ICC profiles from iCCP chunks and preserves transparency
- **WebP**: Embeds ICC profiles using libwebp-sys WebPMux API
- **AVIF** (when built with AVIF feature): Full HDR support with advanced features:
  - Preserves ICC profiles and HDR metadata (color primaries, transfer characteristics, CLLI)
  - Supports gain maps for HDR/SDR tone mapping
  - Automatically detects and preserves HDR content (BT.2020, Display P3, PQ/HLG)
  - 10-bit encoding for HDR images
  - Gain map preservation during image resizing
- **Wide Gamut**: Full support for Display P3, Adobe RGB, BT.2020, and other color spaces
- **Watermarking**: Color profiles and HDR metadata maintained even when adding copyright notices

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
   # List all users (default: users.toml)
   cargo run -- user list

   # Add a new user
   cargo run -- user add alice alice@example.com

   # Remove a user
   cargo run -- user remove alice

   # Update user email
   cargo run -- user update alice newemail@example.com

   # Use a different database backend
   cargo run -- user list --database sqlite://users.db
   cargo run -- user list --database "postgresql://localhost/tenrankai"
   cargo run -- user list --database "dynamodb://users-table?region=us-west-2"

   # Export users to JSON (for migration)
   cargo run -- user export --database users.toml --output users.json

   # Import users from JSON
   cargo run -- user import --database sqlite://users.db --input users.json
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

### User Storage Backends

Tenrankai supports multiple storage backends for user data, enabling flexible deployment options from simple file-based storage to enterprise databases.

**Available Backends:**

| Backend | URL Format | Use Case |
|---------|------------|----------|
| TOML File | `users.toml` or `file:///path/to/users.toml` | Simple deployments, single server |
| SQLite | `sqlite:///path/to/users.db` | Local database, easy backup |
| PostgreSQL | `postgresql://user:pass@host/db` | Production, multi-server |
| DynamoDB | `dynamodb://table-name?region=us-west-2` | Serverless, AWS-native |

**Configuration:**

```toml
[app]
# TOML file (default, backward compatible)
user_database = "users.toml"

# SQLite database
user_database = "sqlite://data/users.db"

# PostgreSQL (shared across multiple servers)
user_database = "postgresql://user:pass@localhost/tenrankai"

# DynamoDB (serverless)
user_database = "dynamodb://tenrankai-users?region=us-west-2"
```

**Multi-Site Support:**

All database backends support multi-tenant isolation. Each site gets its own namespace:
- TOML: Separate file per site
- SQL: Site ID column for row-level isolation
- DynamoDB: Site ID in partition key

**Migration Between Backends:**

```bash
# Export from TOML to JSON
cargo run -- user export --database users.toml --output users.json

# Import to PostgreSQL
cargo run -- user import --database "postgresql://localhost/tenrankai" --input users.json

# Skip existing users during import
cargo run -- user import --database sqlite://users.db --input users.json --skip-existing
```

**Feature Flags:**

SQL and DynamoDB backends require feature flags:
```bash
# Build with SQL support (SQLite + PostgreSQL)
cargo build --features user-storage-sql

# Build with DynamoDB support
cargo build --features user-storage-dynamodb

# Build with all backends
cargo build --features user-storage-all
```

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

## Sitemap

A `sitemap.xml` is generated automatically (per site) at `/sitemap.xml`. It lists
all publicly visible resources: static pages (`pages/*.html.liquid`), publicly
viewable gallery folders and image detail pages, and posts. Folders that require
authentication — and the images inside them — are omitted, and hidden folders are
never listed.

When a site has more URLs than fit in a single sitemap file (the protocol limit is
50,000), `/sitemap.xml` becomes a sitemap index that references `/sitemap/<chunk>.xml`
files. The generated sitemap is cached per site and rebuilt at most every five minutes.
The default `robots.txt` advertises the sitemap when `base_url` is configured; the
sitemap endpoint is only served when `base_url` is set, since it requires absolute URLs.

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

- ✅ **ConfigStorage System**: Centralized site configuration via directory structure or S3
- ✅ **CLI Config Commands**: Manage sites, galleries, and posts from command line
- ✅ **Admin API**: REST API for site configuration and permissions management
- ✅ **Multi-Site Support**: Host multiple independent sites from one server
- ✅ **Pluggable User Storage**: TOML, SQLite, PostgreSQL, and DynamoDB backends for user data
- ✅ **User Migration Tools**: Export/import commands for migrating between storage backends
- ✅ **Pluggable Storage Abstraction**: S3 and filesystem backends for all components
- ✅ **S3 Storage Support**: Galleries, caches, templates, posts, and static files on S3
- ✅ **Signed URL Redirects**: Direct S3 downloads for reduced server bandwidth
- ✅ **WebAuthn/Passkey Authentication**: Biometric and hardware key login support
- ✅ **Gallery Access Control**: Folder-level authentication and user restrictions
- ✅ **User Profile Page**: Centralized passkey management interface
- ✅ **Cascading Static Directories**: Multi-directory asset management with precedence
- ✅ **Null Email Provider**: Development-friendly email logging
- ✅ **Enhanced Asset Management**: Cache busting with automatic versioning
- ✅ **Improved Authentication Flow**: Return URL support and passkey enrollment

### Planned Features

- Additional email providers (SendGrid, SMTP, etc.)
- Full-text search across galleries and posts
- Video file support with thumbnail generation
- Tag-based filtering and organization

### Contributing

Contributions are welcome! Please:

1. Read [CONTRIBUTING.md](CONTRIBUTING.md) for development setup
2. Check existing issues or create new ones for bugs/features
3. Follow the established code style and testing practices
4. Submit pull requests with clear descriptions

### Architecture Highlights

- **Async Rust**: Built on Tokio with Axum web framework
- **Thread-Safe Operations**: Arc<RwLock<T>> for concurrent access
- **Comprehensive Testing**: 243+ unit tests and integration tests (with AVIF), 180+ without AVIF
- **Modular Design**: Clean separation of concerns across modules
- **Configuration-Driven**: Flexible TOML-based configuration system
- **Cross-Platform CI**: Automated testing on Ubuntu, macOS, and Windows
  - **Ubuntu/macOS**: Full feature builds with AVIF support
  - **Windows**: Builds without AVIF for easier compilation

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.
