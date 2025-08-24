# Login Module Documentation

## Overview
The login module provides passwordless email-based authentication for Tenrankai. Users are managed by administrators through a TOML file, and login is performed via email links.

## Features
- Passwordless authentication via email links
- User management through TOML file
- Secure cookie-based sessions (7 days)
- No user registration (admin-managed only)
- Login tokens expire after 10 minutes

## Configuration

### User Database
Users are stored in `users.toml` (see `users.toml.example` for format):
```toml
[users.username]
username = "username"
email = "user@example.com"
```

### Routes
- `/login` - Login page
- `/login/request` - POST endpoint to request login link
- `/login/verify?token=...` - Verify login token and create session
- `/logout` - Clear session and redirect to home

## User Management

Use the `user_admin` CLI tool to manage users:

```bash
# List all users
cargo run --bin user_admin -- list

# Add a new user
cargo run --bin user_admin -- add username user@example.com

# Remove a user
cargo run --bin user_admin -- remove username

# Update user's email
cargo run --bin user_admin -- update username newemail@example.com
```

## Authentication Flow

1. User visits `/login` and enters their username
2. System generates a secure token and logs the login URL (email sending not implemented)
3. User clicks the login link with token
4. System verifies token and creates secure session cookie
5. User is redirected to `/gallery`

## Security

- Login tokens are 32-byte random values
- Tokens expire after 10 minutes
- Session cookies are:
  - HTTPOnly (not accessible via JavaScript)
  - Signed with HMAC-SHA256
  - Valid for 7 days
  - SameSite=Lax to prevent CSRF

## Integration with Gallery

To check if a user is authenticated in other modules:

```rust
use tenrankai::login::{get_authenticated_user, is_authenticated};

// Check if authenticated
if is_authenticated(&headers, &app_state.config.app.download_secret) {
    // User is logged in
}

// Get username
if let Some(username) = get_authenticated_user(&headers, &app_state.config.app.download_secret) {
    // Use the username
}
```

## Current Limitations

- Email sending is not implemented (URLs are logged to console)
- No password reset functionality
- No two-factor authentication
- Single session per user (no concurrent session management)

## Future Enhancements

1. Implement actual email sending via SMTP
2. Add email templates with customizable branding
3. Add login history/audit log
4. Add IP-based rate limiting
5. Support for multiple sessions per user
6. Optional two-factor authentication