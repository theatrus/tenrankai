# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

- **Enhanced Login System**: Improved user experience and security
  - Consolidated JavaScript utilities in `/static/login.js` with cache busting
  - Interstitial passkey enrollment page after email login
  - Return URL functionality to redirect users after successful login
  - Rate limiting protection against brute force attacks

- **Improved Asset Management**: Better cache busting and static file handling
  - Custom Liquid filter `asset_url` for automatic cache busting
  - Thread-safe file version caching with modification time tracking
  - Support for CSS and JS file versioning across cascading directories

### Changed
- **TOML Library Migration**: Switched from `toml` crate to `toml_edit` throughout codebase
  - Better serialization support with structure preservation
  - Improved error handling for configuration parsing
  - Maintains formatting and comments when saving user database

- **CSS Architecture Improvements**: Consolidated color definitions and theming
  - Moved hardcoded colors from `login.css` to CSS variables in `style.css`
  - Added comprehensive message color variables for consistent theming
  - Centralized all theme colors in root CSS variables for easier customization

- **Database Operations**: Enhanced thread safety and async support
  - Implemented `Arc<RwLock<T>>` for thread-safe user database operations
  - Improved error handling and logging for database operations
  - Better concurrent access patterns for user management

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