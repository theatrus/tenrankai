# Login System Changes Summary

## What was implemented:

### 1. Login URLs with `_login` prefix
- All login-related URLs now start with `_login`:
  - `/_login` - Login page
  - `/_login/request` - Login request endpoint
  - `/_login/verify` - Verify login token
  - `/_login/logout` - Logout endpoint
  - `/api/verify` - Check auth status (kept as API endpoint)

### 2. Configurable User Database
- Added `user_database` field to `AppConfig`
- When not specified, the system runs in "no-auth" mode:
  - No login routes are registered
  - All gallery downloads are allowed
  - No user menu appears in header
  - Date approximation setting is ignored

### 3. Modular User Menu
- User menu extracted to separate partial: `_user_menu.html.liquid`
- Only included in header when `has_user_auth` is true
- Template engine tracks whether user auth is enabled
- Clean separation of auth UI from main navigation

### 4. User Management Integration
- Removed separate `user_admin` binary
- User management now available as subcommands:
  ```bash
  tenrankai user list
  tenrankai user add <username> <email>
  tenrankai user remove <username>
  tenrankai user update <username> <email>
  ```
- Database path can be specified with `-d` flag

## Testing:

### With Authentication (default config):
```bash
# Run with user authentication enabled
cargo run --bin tenrankai serve

# Visit http://localhost:8080
# User menu should appear in header
# Login required for large image downloads
```

### Without Authentication:
```bash
# Run without user authentication
cargo run --bin tenrankai -- --config config-no-auth.toml serve

# Visit http://localhost:8080
# No user menu in header
# All images freely downloadable
# No login routes available
```

### User Management:
```bash
# List users
cargo run --bin tenrankai user list

# Add user
cargo run --bin tenrankai user add testuser test@example.com

# Custom database
cargo run --bin tenrankai user list -d custom-users.toml
```

## Benefits:

1. **Clean URL namespace**: `_login` prefix clearly indicates system URLs
2. **Optional authentication**: Sites can run with or without user system
3. **Modular design**: User menu only loaded when needed
4. **Unified CLI**: All functionality in single binary
5. **Flexible deployment**: Easy to enable/disable auth via config