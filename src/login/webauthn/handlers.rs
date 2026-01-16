use axum::{
    extract::Path,
    http::{HeaderMap, StatusCode},
    response::Json,
};
use tracing::{error, info};
use uuid::Uuid;
use webauthn_rs::prelude::*;

use super::{
    PasskeyAuthenticationState, PasskeyInfo, PasskeyRegistrationState, RegisterPasskeyRequest,
    StartAuthenticationRequest, UserPasskey,
};
use crate::{api::is_https_request, login::AuthScope, site::ResolvedState};

#[derive(Debug, serde::Serialize)]
pub struct HasPasskeysResponse {
    pub has_passkeys: bool,
    pub count: usize,
}

pub async fn start_passkey_registration(
    ResolvedState(app_state): ResolvedState,
    auth: crate::login::RequireAuth,
    Json(_request): Json<RegisterPasskeyRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    info!("Starting passkey registration");

    // Get authenticated username
    let username = auth.username().to_string();

    info!("Passkey registration for user: {}", username);

    // Get WebAuthn instance
    let webauthn = app_state
        .webauthn
        .as_ref()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;

    // Get user storage
    let user_storage = app_state
        .user_storage()
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get user
    let user = user_storage
        .get_user(&username)
        .await
        .map_err(|e| {
            error!("Failed to get user: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Get existing passkeys for exclusion
    let exclude_credentials: Vec<CredentialID> = user
        .passkeys
        .iter()
        .map(|pk| pk.credential.cred_id().clone())
        .collect();

    // Start registration
    let user_unique_id = Uuid::new_v5(&Uuid::NAMESPACE_DNS, username.as_bytes());

    // Use email as display name since it's more likely to be valid
    // WebAuthn has strict requirements for display names
    let display_name = user.email.clone();

    // Sanitize username for WebAuthn (remove any special characters)
    // WebAuthn is very strict about what characters are allowed
    let sanitized_username = username
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect::<String>();

    info!(
        "Starting WebAuthn registration - username: {}, sanitized: {}, display_name: {}",
        username, sanitized_username, display_name
    );

    let (challenge, registration_state) = webauthn
        .start_passkey_registration(
            user_unique_id,
            &sanitized_username,
            &display_name,
            Some(exclude_credentials),
        )
        .map_err(|e| {
            error!("Failed to start passkey registration: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Store registration state
    {
        let mut login_state = app_state.login_state().write().await;
        let reg_id = Uuid::new_v4().to_string();
        login_state.pending_registrations.insert(
            reg_id.clone(),
            PasskeyRegistrationState {
                username: username.clone(),
                state: registration_state,
                expires_at: chrono::Utc::now().timestamp() + 300, // 5 minutes
            },
        );

        // Convert challenge to JSON value
        let challenge_value =
            serde_json::to_value(&challenge).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // Create response with both publicKey and reg_id at top level
        let response = serde_json::json!({
            "publicKey": challenge_value.get("publicKey").unwrap_or(&challenge_value),
            "reg_id": reg_id
        });

        Ok(Json(response))
    }
}

pub async fn finish_passkey_registration(
    ResolvedState(app_state): ResolvedState,
    auth: crate::login::RequireAuth,
    Path(reg_id): Path<String>,
    body: String,
) -> Result<StatusCode, StatusCode> {
    info!("Finishing passkey registration for reg_id: {}", reg_id);
    info!("Raw request body: {}", body);

    // Parse the JSON manually to get better error messages
    let auth_data: RegisterPublicKeyCredential = serde_json::from_str(&body).map_err(|e| {
        error!("Failed to parse registration data: {:?}", e);
        error!("Raw body was: {}", body);
        StatusCode::BAD_REQUEST
    })?;

    // Get authenticated username
    let username = auth.username().to_string();
    info!("Finishing registration for user: {}", username);

    // Get WebAuthn instance
    let webauthn = app_state.webauthn.as_ref().ok_or_else(|| {
        error!("WebAuthn not configured");
        StatusCode::NOT_IMPLEMENTED
    })?;

    // Get registration state
    let registration_state = {
        let mut login_state = app_state.login_state().write().await;
        login_state
            .pending_registrations
            .remove(&reg_id)
            .ok_or_else(|| {
                error!("Registration state not found for reg_id: {}", reg_id);
                StatusCode::BAD_REQUEST
            })?
    };

    info!(
        "Found registration state for user: {}",
        registration_state.username
    );

    // Verify username matches
    if registration_state.username != username {
        error!(
            "Username mismatch: {} != {}",
            registration_state.username, username
        );
        return Err(StatusCode::FORBIDDEN);
    }

    // Complete registration
    info!("Completing WebAuthn registration");
    let passkey = webauthn
        .finish_passkey_registration(&auth_data, &registration_state.state)
        .map_err(|e| {
            error!("Failed to finish passkey registration: {:?}", e);
            StatusCode::BAD_REQUEST
        })?;

    // Get user storage
    let user_storage = app_state.user_storage().as_ref().ok_or_else(|| {
        error!("User storage not configured");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Adding passkey to user database");

    // Add passkey to user
    let user_passkey = UserPasskey::new("New Passkey".to_string(), passkey);
    info!(
        "Adding passkey with ID: {} for user: {}",
        user_passkey.id, username
    );

    user_storage
        .add_passkey(&username, user_passkey)
        .await
        .map_err(|e| {
            error!("Failed to add passkey: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    info!("Passkey registered successfully for user: {}", username);
    Ok(StatusCode::OK)
}

pub async fn start_passkey_authentication(
    ResolvedState(app_state): ResolvedState,
    Json(request): Json<StartAuthenticationRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Get WebAuthn instance
    let webauthn = app_state
        .webauthn
        .as_ref()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;

    // Get user storage
    let user_storage = app_state
        .user_storage()
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get user and passkeys by username or email
    let identifier = request.username.trim().to_lowercase();
    let (username, user) = match user_storage.get_user(&identifier).await {
        Ok(Some(user)) => (identifier.clone(), user),
        Ok(None) => {
            // Try by email
            match user_storage.get_user_by_email(&identifier).await {
                Ok(Some((username, user))) => (username, user),
                Ok(None) => return Err(StatusCode::NOT_FOUND),
                Err(e) => {
                    error!("Failed to lookup user by email: {}", e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
        }
        Err(e) => {
            error!("Failed to lookup user: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Check if user has passkeys
    if user.passkeys.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }

    // Get passkeys for authentication
    let allow_credentials: Vec<Passkey> = user
        .passkeys
        .iter()
        .map(|pk| pk.credential.clone())
        .collect();

    // Start authentication
    let (challenge, authentication_state) = webauthn
        .start_passkey_authentication(&allow_credentials)
        .map_err(|e| {
            error!("Failed to start passkey authentication: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Store authentication state
    {
        let mut login_state = app_state.login_state().write().await;
        let auth_id = Uuid::new_v4().to_string();
        login_state.pending_authentications.insert(
            auth_id.clone(),
            PasskeyAuthenticationState {
                state: authentication_state,
                expires_at: chrono::Utc::now().timestamp() + 300, // 5 minutes
            },
        );

        // Convert challenge to JSON value
        let challenge_value =
            serde_json::to_value(&challenge).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // Create response with publicKey, auth_id, and username at top level
        let response = serde_json::json!({
            "publicKey": challenge_value.get("publicKey").unwrap_or(&challenge_value),
            "auth_id": auth_id,
            "username": username
        });

        Ok(Json(response))
    }
}

pub async fn finish_passkey_authentication(
    ResolvedState(app_state): ResolvedState,
    Path(auth_id): Path<String>,
    request_headers: HeaderMap,
    Json(auth_data): Json<PublicKeyCredential>,
) -> Result<(HeaderMap, StatusCode), StatusCode> {
    // Get WebAuthn instance
    let webauthn = app_state
        .webauthn
        .as_ref()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;

    // Get authentication state
    let authentication_state = {
        let mut login_state = app_state.login_state().write().await;
        login_state
            .pending_authentications
            .remove(&auth_id)
            .ok_or(StatusCode::BAD_REQUEST)?
    };

    // Complete authentication
    let authentication_result = webauthn
        .finish_passkey_authentication(&auth_data, &authentication_state.state)
        .map_err(|e| {
            error!("Failed to finish passkey authentication: {}", e);
            StatusCode::UNAUTHORIZED
        })?;

    // Get user storage
    let user_storage = app_state
        .user_storage()
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Find user by credential ID
    let (username, passkey) = user_storage
        .get_passkey_by_credential_id(&auth_data.raw_id)
        .await
        .map_err(|e| {
            error!("Failed to find passkey by credential ID: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Update the passkey after successful authentication
    user_storage
        .update_passkey_after_auth(&username, &passkey.id, &authentication_result)
        .await
        .map_err(|e| {
            error!("Failed to update passkey after auth: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Create session cookie
    let is_https = is_https_request(&request_headers);
    let signed_value = crate::api::create_signed_cookie(app_state.cookie_secret(), &username)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let cookie = AuthScope::Session.format_cookie(&signed_value, is_https);

    let mut headers = HeaderMap::new();
    headers.insert("Set-Cookie", cookie.parse().unwrap());

    info!("User {} authenticated via passkey", username);

    Ok((headers, StatusCode::OK))
}

pub async fn list_passkeys(
    ResolvedState(app_state): ResolvedState,
    auth: crate::login::RequireAuth,
) -> Result<Json<Vec<PasskeyInfo>>, StatusCode> {
    // Get authenticated username
    let username = auth.username();

    // Get user storage
    let user_storage = app_state
        .user_storage()
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get user and map passkeys to info
    let user = user_storage
        .get_user(username)
        .await
        .map_err(|e| {
            error!("Failed to get user: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Map passkeys to info
    let passkey_info: Vec<PasskeyInfo> = user
        .passkeys
        .iter()
        .map(|pk| PasskeyInfo {
            id: pk.id,
            name: pk.name.clone(),
            created_at: pk.created_at,
            last_used_at: pk.last_used_at,
        })
        .collect();

    Ok(Json(passkey_info))
}

pub async fn delete_passkey(
    ResolvedState(app_state): ResolvedState,
    auth: crate::login::RequireAuth,
    Path(passkey_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    // Get authenticated username
    let username = auth.username();

    // Get user storage
    let user_storage = app_state
        .user_storage()
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Remove passkey from user
    let removed = user_storage
        .remove_passkey(username, &passkey_id)
        .await
        .map_err(|e| {
            error!("Failed to remove passkey: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if removed {
        info!("Passkey {} deleted for user: {}", passkey_id, username);
        Ok(StatusCode::OK)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

pub async fn check_user_has_passkeys(
    ResolvedState(app_state): ResolvedState,
    Json(request): Json<StartAuthenticationRequest>,
) -> Result<Json<HasPasskeysResponse>, StatusCode> {
    // Get user storage
    let user_storage = app_state
        .user_storage()
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Try to find user by username or email
    let identifier = request.username.trim().to_lowercase();
    let user = match user_storage.get_user(&identifier).await {
        Ok(Some(user)) => Some(user),
        Ok(None) => {
            // Try by email
            match user_storage.get_user_by_email(&identifier).await {
                Ok(Some((_, user))) => Some(user),
                Ok(None) => None,
                Err(e) => {
                    error!("Failed to lookup user by email: {}", e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
        }
        Err(e) => {
            error!("Failed to lookup user: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Check if user has passkeys
    if let Some(user) = user {
        Ok(Json(HasPasskeysResponse {
            has_passkeys: !user.passkeys.is_empty(),
            count: user.passkeys.len(),
        }))
    } else {
        // Don't reveal if user exists
        Ok(Json(HasPasskeysResponse {
            has_passkeys: false,
            count: 0,
        }))
    }
}

pub async fn update_passkey_name(
    ResolvedState(app_state): ResolvedState,
    auth: crate::login::RequireAuth,
    Path(passkey_id): Path<Uuid>,
    Json(name): Json<String>,
) -> Result<StatusCode, StatusCode> {
    // Get authenticated username
    let username = auth.username();

    // Get user storage
    let user_storage = app_state
        .user_storage()
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get user and update passkey name
    let mut user = user_storage
        .get_user(username)
        .await
        .map_err(|e| {
            error!("Failed to get user: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Find and update the passkey name
    let updated = if let Some(passkey) = user.get_passkey_mut(&passkey_id) {
        passkey.name = name;
        true
    } else {
        false
    };

    if updated {
        // Save the updated user
        user_storage
            .update_user(username, &user)
            .await
            .map_err(|e| {
                error!("Failed to update user: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        info!("Passkey {} name updated for user: {}", passkey_id, username);
        Ok(StatusCode::OK)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
