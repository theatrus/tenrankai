# Tenrankai API Reference

This document provides detailed information about Tenrankai's HTTP API endpoints.

## Base URL

All API endpoints are relative to your server's base URL:
```
http://localhost:3000  # Development
https://yourdomain.com # Production
```

## Authentication

Tenrankai uses session-based authentication with HTTPOnly cookies:

- Sessions are established via email login or WebAuthn
- API endpoints requiring authentication return `401 Unauthorized` if not logged in
- Sessions expire after inactivity or explicit logout

## Content Types

- **Request**: `application/json` for POST/PUT requests
- **Response**: `application/json` for API endpoints, `text/html` for pages

## Gallery API

### Get Gallery Preview Images

Retrieve random preview images from all galleries for homepage display.

**Endpoint**: `GET /api/gallery/preview`

**Parameters**:
- `count` (optional): Number of images to return (default: 6)

**Response**:
```json
{
  "images": [
    {
      "gallery_name": "photos",
      "path": "vacation/beach.jpg",
      "url": "/gallery/image/vacation/beach.jpg?size=thumbnail",
      "detail_url": "/gallery/detail/vacation/beach.jpg",
      "title": "Beach Sunset",
      "width": 800,
      "height": 600
    }
  ]
}
```

### Get Gallery Composite Preview

Generate composite preview image for a specific gallery folder.

**Endpoint**: `GET /api/gallery/{name}/composite/{path}`

**Parameters**:
- `name`: Gallery name (from configuration)
- `path`: Folder path within gallery

**Response**: Image binary data (JPEG)

**Headers**:
- `Content-Type`: `image/jpeg`
- `Cache-Control`: Long-term caching headers

## Posts API

### Refresh Posts Cache

Manually refresh the posts cache to pick up new or modified posts.

**Endpoint**: `POST /api/posts/{name}/refresh`

**Authentication**: Required

**Parameters**:
- `name`: Posts system name (from configuration)

**Response**:
```json
{
  "success": true,
  "message": "Posts refreshed successfully",
  "count": 25
}
```

**Error Response**:
```json
{
  "error": "Posts system 'invalid_name' not found"
}
```

## Authentication API

### Check Authentication Status

Check if the current session is authenticated.

**Endpoint**: `GET /api/verify`

**Response** (Authenticated):
```json
{
  "authenticated": true,
  "username": "alice",
  "email": "alice@example.com"
}
```

**Response** (Not Authenticated):
```json
{
  "authenticated": false
}
```

### Request Login Email

Send login email to user's registered address.

**Endpoint**: `POST /_login/request`

**Content-Type**: `application/x-www-form-urlencoded`

**Body**:
```
identifier=alice@example.com
```

**Response**:
```json
{
  "success": true,
  "message": "Login email sent to your registered address"
}
```

**Error Response**:
```json
{
  "error": "User not found or rate limit exceeded"
}
```

### Verify Login Token

Authenticate using email login token.

**Endpoint**: `GET /_login/verify`

**Parameters**:
- `token`: Login token from email
- `return_url` (optional): URL to redirect after successful login

**Response**: HTTP redirect to return URL or home page

## WebAuthn API

### Check User Passkeys

Check if a user has registered passkeys for WebAuthn authentication.

**Endpoint**: `POST /api/webauthn/check-passkeys`

**Body**:
```json
{
  "username": "alice"
}
```

**Response**:
```json
{
  "has_passkeys": true,
  "count": 2
}
```

### Start Passkey Registration

Begin WebAuthn passkey registration flow.

**Endpoint**: `POST /api/webauthn/register/start`

**Authentication**: Required

**Body**:
```json
{
  "passkey_name": "My iPhone"
}
```

**Response**:
```json
{
  "registration_id": "reg_123456789",
  "challenge": {
    "publicKey": {
      "challenge": "base64-encoded-challenge",
      "rp": {
        "name": "Tenrankai",
        "id": "yourdomain.com"
      },
      "user": {
        "id": "base64-user-id",
        "name": "alice",
        "displayName": "Alice Smith"
      },
      "pubKeyCredParams": [...],
      "authenticatorSelection": {...},
      "timeout": 60000,
      "attestation": "none"
    }
  }
}
```

### Complete Passkey Registration

Complete WebAuthn passkey registration.

**Endpoint**: `POST /api/webauthn/register/finish/{reg_id}`

**Authentication**: Required

**Parameters**:
- `reg_id`: Registration ID from start call

**Body**: WebAuthn credential creation response
```json
{
  "id": "credential-id",
  "rawId": "base64-raw-id",
  "response": {
    "clientDataJSON": "base64-client-data",
    "attestationObject": "base64-attestation"
  },
  "type": "public-key"
}
```

**Response**:
```json
{
  "success": true,
  "passkey_id": "pk_987654321",
  "name": "My iPhone"
}
```

### Start Passkey Authentication

Begin WebAuthn passkey authentication flow.

**Endpoint**: `POST /api/webauthn/authenticate/start`

**Body**:
```json
{
  "username": "alice"
}
```

**Response**:
```json
{
  "authentication_id": "auth_123456789",
  "challenge": {
    "publicKey": {
      "challenge": "base64-encoded-challenge",
      "timeout": 60000,
      "rpId": "yourdomain.com",
      "allowCredentials": [
        {
          "id": "base64-credential-id",
          "type": "public-key"
        }
      ],
      "userVerification": "preferred"
    }
  }
}
```

### Complete Passkey Authentication

Complete WebAuthn passkey authentication.

**Endpoint**: `POST /api/webauthn/authenticate/finish/{auth_id}`

**Parameters**:
- `auth_id`: Authentication ID from start call

**Body**: WebAuthn authentication response
```json
{
  "id": "credential-id",
  "rawId": "base64-raw-id",
  "response": {
    "clientDataJSON": "base64-client-data",
    "authenticatorData": "base64-authenticator-data",
    "signature": "base64-signature"
  },
  "type": "public-key"
}
```

**Response**:
```json
{
  "success": true,
  "redirect_url": "/dashboard"
}
```

### List User Passkeys

Get list of user's registered passkeys.

**Endpoint**: `GET /api/webauthn/passkeys`

**Authentication**: Required

**Response**:
```json
{
  "passkeys": [
    {
      "id": "pk_123456789",
      "name": "My iPhone",
      "created_at": "2024-08-25T10:30:00Z",
      "last_used": "2024-08-25T10:30:00Z"
    },
    {
      "id": "pk_987654321",
      "name": "Hardware Key",
      "created_at": "2024-08-20T15:45:00Z",
      "last_used": "2024-08-24T09:15:00Z"
    }
  ]
}
```

### Delete Passkey

Remove a registered passkey.

**Endpoint**: `DELETE /api/webauthn/passkeys/{passkey_id}`

**Authentication**: Required

**Parameters**:
- `passkey_id`: ID of passkey to delete

**Response**:
```json
{
  "success": true,
  "message": "Passkey deleted successfully"
}
```

### Rename Passkey

Update the display name of a passkey.

**Endpoint**: `PUT /api/webauthn/passkeys/{passkey_id}/name`

**Authentication**: Required

**Parameters**:
- `passkey_id`: ID of passkey to rename

**Body**:
```json
{
  "name": "New Passkey Name"
}
```

**Response**:
```json
{
  "success": true,
  "name": "New Passkey Name"
}
```

## Utility API

### Refresh Static File Versions

Manually refresh the static file version cache for cache busting.

**Endpoint**: `POST /api/refresh-static-versions`

**Authentication**: Required

**Response**:
```json
{
  "success": true,
  "message": "Static file versions refreshed",
  "versions": {
    "style.css": 1724567890,
    "login.js": 1724567891,
    "login.css": 1724567892
  }
}
```

## Error Responses

All API endpoints may return these common error responses:

### 400 Bad Request
```json
{
  "error": "Invalid request format or missing required fields"
}
```

### 401 Unauthorized
```json
{
  "error": "Authentication required"
}
```

### 403 Forbidden
```json
{
  "error": "Insufficient permissions"
}
```

### 404 Not Found
```json
{
  "error": "Resource not found"
}
```

### 429 Too Many Requests
```json
{
  "error": "Rate limit exceeded. Please try again later."
}
```

### 500 Internal Server Error
```json
{
  "error": "Internal server error"
}
```

## Rate Limiting

- **Login attempts**: 5 attempts per 5 minutes per IP address
- **API requests**: No explicit limits, but abuse may be throttled
- **WebAuthn operations**: No specific limits

## CORS

- CORS is not explicitly configured
- Same-origin policy applies to browser requests
- API is designed for same-origin frontend usage

## WebSocket Support

Tenrankai does not currently support WebSocket connections. All communication is via HTTP requests.

## Versioning

The API does not currently use versioning. Breaking changes will be documented in release notes and migration guides provided when necessary.