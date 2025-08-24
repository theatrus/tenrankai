# Cookie Secret Migration Summary

## Changes Made:

### 1. Configuration Changes
- **Removed**: `download_password` field (no longer used)
- **Renamed**: `download_secret` → `cookie_secret` (more descriptive name)
- **Purpose**: The secret is used for signing authentication cookies with HMAC-SHA256

### 2. Updated Files:
- **Core Configuration** (`src/lib.rs`):
  - Removed `download_password` from `AppConfig`
  - Renamed `download_secret` to `cookie_secret`
  
- **All References Updated**:
  - `src/api.rs`
  - `src/login/handlers.rs`
  - `src/gallery/handlers.rs`
  - Test files (gallery, posts, integration tests)
  
- **Configuration Files**:
  - `config.toml`
  - `config.example.toml`
  - `config-no-auth.toml`
  
- **Documentation**:
  - `README.md`
  - `debian/README.Debian`

### 3. Migration Instructions

For existing installations:

1. Update your `config.toml`:
   ```toml
   # Old format:
   download_secret = "your-secret"
   download_password = "not-used"
   
   # New format:
   cookie_secret = "your-secret-key-use-a-long-random-string"
   ```

2. Remove the `download_password` line entirely

3. Keep the same secret value, just rename the field

### 4. Security Notes

- The `cookie_secret` should be a long, random string
- It's used to sign authentication cookies preventing tampering
- Generate a secure secret with: `openssl rand -base64 32`
- Never commit the actual secret to version control

### 5. Backward Compatibility

- The old password-based authentication system has been completely removed
- Only cookie-based authentication via the user system is supported
- Sites without `user_database` configured run in no-auth mode