# Login Email Support

## Overview
The login system now accepts both username and email address for authentication. This provides users with more flexibility when logging in.

## Changes Made

### 1. Backend Changes

#### UserDatabase (`src/login/types.rs`)
- Added `get_user_by_username_or_email()` method that:
  - First tries direct username lookup
  - Falls back to searching by email (case-insensitive)
  - Returns the User object if found

#### Login Handler (`src/login/handlers.rs`)
- Updated `login_request()` to:
  - Accept the input as either username or email
  - Use `get_user_by_username_or_email()` for lookup
  - Always use the actual username for token creation
  - Updated messages to say "account" instead of "username"

### 2. Frontend Changes

#### Login Template (`templates/login.html.liquid`)
- Updated form to accept username or email:
  - Changed label to "Username or Email:"
  - Added placeholder text showing both options
  - Updated validation message
- Improved UX after submission:
  - Hides the login form upon successful submission
  - Shows success message with "Try again" link
  - "Try again" link simply reloads the page to reset the form
  - Error messages appear below the form without hiding it

### 3. Security Considerations
- Login endpoint still prevents user enumeration by always returning success
- Rate limiting applies regardless of whether username or email is used
- Email lookups are case-insensitive for better user experience

## Testing
Added unit test `test_user_lookup_by_email` that verifies:
- Lookup by username works
- Lookup by email works (exact case)
- Lookup by email works (case-insensitive)
- Non-existent users return None

## Usage Examples

Users can now log in with either:
```
Username: alice
Email: alice@example.com
Email: Alice@Example.com (case-insensitive)
```

All will authenticate the same user account.