# Contributing to Tenrankai

Thank you for your interest in contributing to Tenrankai! This document provides guidelines and information for contributors.

## Development Setup

### Prerequisites

- Rust 1.89.0 or later (managed automatically by `rust-toolchain.toml`)
- Git for version control

### Getting Started

1. **Clone the repository**
   ```bash
   git clone https://github.com/theatrus/tenrankai.git
   cd tenrankai
   ```

2. **Build the project**
   ```bash
   cargo build
   ```

3. **Run tests**
   ```bash
   cargo test
   ```

4. **Run the development server**
   ```bash
   cargo run
   ```

### Required Files for Development

Place these files in a `static/` directory for full functionality:

- `DejaVuSans.ttf` - Required for copyright watermarking
- `favicon.svg` - Optional, for favicon generation

## Project Structure

```
src/
├── main.rs              # Application entry point and CLI
├── lib.rs               # Core configuration and app creation
├── api/                 # API endpoint handlers
├── composite/           # Image composite generation
├── copyright/           # Copyright watermarking
├── email/               # Email provider system
│   └── providers/       # Email provider implementations (SES, null)
├── favicon/             # Favicon generation from SVG
├── gallery/             # Gallery system (core functionality)
├── login/               # Authentication system
│   ├── types.rs         # User database and types
│   └── webauthn/        # WebAuthn/Passkey implementation
├── posts/               # Blog/posts system
├── robots/              # robots.txt generation
├── startup_checks/      # Application startup validation
├── static_files/        # Static file serving with cascading directories
├── templating/          # Liquid template engine and filters
└── webp_encoder/        # WebP image encoding with ICC profiles

templates/               # Liquid templates
├── pages/              # Static page templates
├── modules/            # Feature-specific templates
└── partials/           # Reusable template components

static/                 # Static assets
├── style.css          # Main CSS with theme variables
├── login.css          # Login-specific styles
├── login.js           # Login JavaScript utilities
└── DejaVuSans.ttf     # Font for watermarking

tests/                  # Integration tests
```

## Code Organization

### Key Components

1. **Gallery System** (`src/gallery/`):
   - Image processing and caching
   - Directory scanning and metadata extraction
   - Multiple gallery instance support

2. **Authentication** (`src/login/`):
   - Email-based authentication
   - WebAuthn/Passkey support
   - User management and database operations
   - Session management and rate limiting

3. **Static Files** (`src/static_files.rs`):
   - Cascading directory support
   - Cache busting with file versioning
   - Automatic asset URL generation

4. **Email System** (`src/email/`):
   - Provider abstraction for different email services
   - Amazon SES and null (development) providers
   - Template-based email composition

5. **Template Engine** (`src/templating.rs`):
   - Liquid template processing
   - Custom filters (asset_url)
   - Gallery integration and previews

### Configuration

- `Config` struct in `src/lib.rs` defines all application settings
- TOML-based configuration with serde support
- Custom deserializers for backward compatibility (e.g., static directories)

### Database Operations

- User database stored in TOML format using `toml_edit`
- Thread-safe operations with `Arc<RwLock<T>>`
- Passkey data serialization with WebAuthn types

## Testing

### Test Structure

```
tests/
├── asset_url_integration.rs      # Asset URL filter tests
├── cascading_static_directories.rs # Static directory precedence tests
├── gallery_integration_tests.rs   # Gallery functionality tests
├── null_email_provider.rs        # Email provider tests
├── posts_integration.rs          # Posts/blog system tests
├── template_example.rs           # Template rendering tests
└── template_integration.rs       # Template integration tests
```

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test file
cargo test cascading_static_directories

# Run with output
cargo test -- --nocapture

# Run tests in parallel
cargo test -- --test-threads=4
```

### Test Categories

1. **Unit Tests**: Located in `src/` files using `#[cfg(test)]`
2. **Integration Tests**: Located in `tests/` directory
3. **Feature Tests**: Test complete workflows (login, gallery browsing, etc.)

## Code Style

### Rust Guidelines

- Follow standard Rust formatting with `rustfmt`
- Use meaningful variable and function names
- Add documentation comments for public APIs
- Handle errors properly with `Result<T, E>` types

### CSS Guidelines

- Use CSS custom properties (variables) defined in `style.css`
- Follow BEM naming convention where appropriate
- Maintain responsive design principles
- Test on mobile and desktop viewports

### JavaScript Guidelines

- Use modern JavaScript (ES6+) features
- Export utilities via `window` object for template access
- Handle errors gracefully with user-friendly messages
- Follow async/await patterns for API calls

## Submitting Changes

### Pull Request Process

1. **Fork** the repository
2. **Create** a feature branch from `main`
3. **Make** your changes with appropriate tests
4. **Test** thoroughly including edge cases
5. **Document** changes in commit messages
6. **Submit** a pull request with description

### Commit Messages

Use conventional commit format:

```
type(scope): description

feat(auth): add WebAuthn passkey support
fix(gallery): resolve image caching issue
docs(readme): update installation instructions
test(login): add integration tests for email flow
```

### Code Review

- All changes require review before merging
- Address feedback constructively
- Ensure CI/CD checks pass
- Update documentation as needed

## Development Workflow

### Adding New Features

1. **Design**: Consider impact on existing functionality
2. **Configuration**: Add configuration options if needed
3. **Implementation**: Write core functionality with error handling
4. **Testing**: Add comprehensive tests
5. **Documentation**: Update README and other docs
6. **Integration**: Ensure compatibility with existing features

### Bug Fixes

1. **Reproduce**: Create test case that demonstrates the issue
2. **Fix**: Implement minimal fix addressing root cause
3. **Verify**: Ensure fix doesn't break existing functionality
4. **Document**: Update relevant documentation

### Email Provider Implementation

To add a new email provider:

1. Create new module in `src/email/providers/`
2. Implement `EmailProvider` trait
3. Add provider registration in `src/email/mod.rs`
4. Add configuration options to provider enum
5. Write integration tests
6. Update documentation

### Template System

Templates use Liquid syntax with custom filters:

- `asset_url`: Adds cache-busting version parameters
- Gallery integration variables available in all templates
- Partials for reusable components

## Getting Help

- **Issues**: Open GitHub issues for bugs or feature requests
- **Discussions**: Use GitHub Discussions for questions
- **Documentation**: Check README.md and code comments

## License

By contributing, you agree that your contributions will be licensed under the Apache License 2.0.