# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **AVIF Browser Fallback**: AVIF sources can now be served as WebP or JPEG for browsers without AVIF support
  - Original AVIF images are always served as AVIF (preserving HDR and gain maps)
  - Resized images fall back to WebP or JPEG when browser doesn't support AVIF
  - Automatic format negotiation based on Accept headers
  - Helps ensure compatibility while maintaining AVIF as primary format

- **AVIF Container Parsing Module**: Refactored AVIF container parsing into separate module
  - New `avif_container.rs` module for ISOBMFF box parsing
  - Cleaner separation of concerns between decoding and container inspection
  - Support for extracting ICC profiles and dimensions without full decode

- **Build Dependencies**: CI/CD now installs required AVIF build dependencies
  - nasm, ninja, meson, and cmake installed on all platforms
  - Ensures reliable builds across Ubuntu, macOS, and Windows

### Changed
- **AVIF Code Simplifications**: Streamlined AVIF implementation
  - Simplified error handling by removing verbose error code mapping
  - Extracted helper functions for fraction conversions
  - Added `browser_supports_avif()` utility function
  - Improved code organization and reduced duplication

### Breaking Changes
- **Copyright Watermark Configuration**: Moved copyright holder configuration from global `[app]` section to per-gallery basis
  - Remove `copyright_holder` from `[app]` section in config.toml
  - Add `copyright_holder` to each `[[galleries]]` section that needs watermarking
  - This allows different copyright holders for different galleries (e.g., personal vs. portfolio)
  - Gallery struct no longer requires AppConfig parameter in constructor

### Changed
- **Image Processing Refactoring**: Reorganized image processing into focused submodules
  - Created separate modules for each image format (JPEG, PNG, WebP)
  - Extracted ICC profile handling into dedicated module
  - Separated resize logic from serving logic
  - Moved cache pregeneration methods to cache module where they belong
  - Reduced method sizes and improved code organization

### Added
- **Cascading Static Directories**: Support for multiple static file directories with precedence ordering
  - Configure multiple directories in `[static_files]` section: `directories = ["static-custom", "static"]`
  - Files in earlier directories override files in later directories
  - Backward compatible with single directory configuration
  - All components (favicon, robots.txt, template engine) support cascading directories

- **Null Email Provider**: Development/testing email provider that logs emails instead of sending them
  - Configure with `provider = "null"` in email configuration
  - Perfect for local development and testing environments
  - Logs full email content including recipients, subject, and body preview

- **WebAuthn/Passkey Support**: Modern passwordless authentication with biometric login
  - Secure passkey registration and authentication
  - Biometric authentication support (fingerprint, face recognition, hardware keys)
  - Cross-device passkey synchronization
  - Fallback to email-based login when WebAuthn unavailable
  - Multiple passkeys per user account

- **User Profile Page**: New centralized account management interface at `/_login/profile`
  - View username and email address
  - Manage registered passkeys (remove, view creation dates)
  - Link to enroll new passkeys
  - Logout functionality
  - Accessible from user menu in navigation

- **Gallery Access Control**: Folder-level authentication and user restrictions
  - `require_auth = true` in `_folder.md` TOML to require authentication
  - `allowed_users = ["user1", "user2"]` to restrict access to specific users
  - Hierarchical access control (parent folder restrictions apply to children)
  - Access control applies to gallery views, previews, image viewing, and API endpoints
  - Comprehensive checking throughout the gallery pipeline

- **Enhanced Login System**: Improved user experience and security
  - Consolidated JavaScript utilities in `/static/login.js` with cache busting
  - Interstitial passkey enrollment page after email login
  - Return URL functionality to redirect users after successful login
  - Rate limiting protection against brute force attacks
  - Fixed timing issue with asset_url filter not encoding versions

- **Improved Asset Management**: Better cache busting and static file handling
  - Custom Liquid filter `asset_url` for automatic cache busting
  - Thread-safe file version caching with modification time tracking
  - Support for CSS and JS file versioning across cascading directories
  - Fixed race condition where template engine loaded before static file versions

### Changed
- **TOML Library Migration**: Switched from `toml` crate to `toml_edit` throughout codebase
  - Better serialization support with structure preservation
  - Improved error handling for configuration parsing
  - Maintains formatting and comments when saving user database

- **User Database Format**: Simplified TOML serialization using serde directly
  - Removed complex JSON-to-TOML conversion code (300+ lines reduced to ~10 lines)
  - Users stored as proper TOML tables: `[users.username]`
  - Passkeys stored as array of tables: `[[users.username.passkeys]]`
  - All fields properly serialized as TOML values (not JSON strings)
  - Clean, human-readable format with proper indentation

- **CSS Architecture Improvements**: Consolidated color definitions and theming
  - Moved hardcoded colors from `login.css` to CSS variables in `style.css`
  - Added comprehensive message color variables for consistent theming
  - Centralized all theme colors in root CSS variables for easier customization
  - Improved contrast on login pages for better accessibility
  - Added button disabled states and danger button colors

- **Template Organization**: Restructured login templates for better maintainability
  - Moved login-related templates to `templates/modules/` directory
  - Separated inline styles and JavaScript to external files
  - Fixed LoginUtils availability issues with proper DOMContentLoaded handling

- **Database Operations**: Enhanced thread safety and async support
  - Implemented `Arc<RwLock<T>>` for thread-safe user database operations
  - Improved error handling and logging for database operations
  - Better concurrent access patterns for user management

- **Build System**: Improved cross-platform build support
  - Windows builds now use vcpkg for OpenSSL installation instead of chocolatey
  - Re-enabled Windows and macOS builds in CI/CD workflows
  - Added `OPENSSL_STATIC=1` for better Rust compilation support
  - Improved build reliability across all platforms

### Technical Improvements
- **Testing Coverage**: Added comprehensive integration tests
  - Cascading static directories functionality tests
  - Asset URL filter unit and integration tests
  - Null email provider integration tests
  - Template rendering with cache busting tests

- **Error Handling**: Improved error messages and logging
  - Better error reporting for configuration issues
  - Enhanced debugging information for static file operations
  - Clearer error messages for authentication failures

- **Performance Optimizations**: Various performance improvements
  - Optimized file version caching with background refresh
  - Improved directory scanning for static files
  - Better memory usage patterns for large file collections

### Dependencies
- Added `liquid-core` for custom Liquid filter implementation
- Migrated from `toml` to `toml_edit` with serde support features
- All existing dependencies updated to latest compatible versions

### Backward Compatibility
- All changes maintain backward compatibility with existing configurations
- Single static directory configuration still supported: `directories = "static"`
- Existing email configuration continues to work unchanged
- Template structure and API endpoints remain the same

## Previous Versions

### [0.1.0] - Initial Release
- Basic photo gallery functionality
- Multiple gallery support
- Image processing with WebP support
- Markdown-based posts system
- Email-based authentication
- Responsive web interface
- ICC color profile preservation
- Copyright watermarking