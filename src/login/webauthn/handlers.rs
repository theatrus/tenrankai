use axum::{
    extract::{Path, State},
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
use crate::{AppState, login::get_authenticated_user};

#[derive(Debug, serde::Serialize)]
pub struct HasPasskeysResponse {
    pub has_passkeys: bool,
    pub count: usize,
}

pub async fn start_passkey_registration(
    State(app_state): State<AppState>,
    headers: HeaderMap,
    Json(_request): Json<RegisterPasskeyRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    info!("Starting passkey registration");

    // Check if user is authenticated
    let username = get_authenticated_user(&headers, &app_state.config.app.cookie_secret)
        .ok_or_else(|| {
            error!("Passkey registration failed: user not authenticated");
            StatusCode::UNAUTHORIZED
        })?;

    info!("Passkey registration for user: {}", username);

    // Get WebAuthn instance
    let webauthn = app_state
        .webauthn
        .as_ref()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;

    // Get user database manager
    let db_manager = app_state
        .user_database_manager
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get user
    let user = {
        let db = db_manager.database().read().await;
        db.get_user(&username).cloned()
    }
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
        let mut login_state = app_state.login_state.write().await;
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
    State(app_state): State<AppState>,
    headers: HeaderMap,
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

    // Check if user is authenticated
    let username = get_authenticated_user(&headers, &app_state.config.app.cookie_secret)
        .ok_or_else(|| {
            error!("Finish registration failed: user not authenticated");
            StatusCode::UNAUTHORIZED
        })?;

    info!("Finishing registration for user: {}", username);

    // Get WebAuthn instance
    let webauthn = app_state.webauthn.as_ref().ok_or_else(|| {
        error!("WebAuthn not configured");
        StatusCode::NOT_IMPLEMENTED
    })?;

    // Get registration state
    let registration_state = {
        let mut login_state = app_state.login_state.write().await;
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

    // Get user database manager
    let db_manager = app_state.user_database_manager.as_ref().ok_or_else(|| {
        error!("User database manager not configured");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Adding passkey to user database");

    // Add passkey to user
    {
        let mut db = db_manager.database().write().await;
        if let Some(user) = db.get_user_mut(&username) {
            let user_passkey = UserPasskey::new("New Passkey".to_string(), passkey);
            info!(
                "Adding passkey with ID: {} for user: {}",
                user_passkey.id, username
            );
            user.add_passkey(user_passkey);
        } else {
            error!("User not found in database: {}", username);
            return Err(StatusCode::NOT_FOUND);
        }
    }

    info!("Saving database changes");

    // Save database
    db_manager.save().await.map_err(|e| {
        error!("Failed to save database: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Passkey registered successfully for user: {}", username);
    Ok(StatusCode::OK)
}

pub async fn start_passkey_authentication(
    State(app_state): State<AppState>,
    Json(request): Json<StartAuthenticationRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Get WebAuthn instance
    let webauthn = app_state
        .webauthn
        .as_ref()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;

    // Get user database manager
    let db_manager = app_state
        .user_database_manager
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get user and passkeys
    let (username, allow_credentials) = {
        let db = db_manager.database().read().await;
        let (username, user) = db
            .get_user_by_username_or_email_with_username(&request.username)
            .ok_or(StatusCode::NOT_FOUND)?;

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

        (username, allow_credentials)
    };

    // Start authentication
    let (challenge, authentication_state) = webauthn
        .start_passkey_authentication(&allow_credentials)
        .map_err(|e| {
            error!("Failed to start passkey authentication: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Store authentication state
    {
        let mut login_state = app_state.login_state.write().await;
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
    State(app_state): State<AppState>,
    Path(auth_id): Path<String>,
    Json(auth_data): Json<PublicKeyCredential>,
) -> Result<(HeaderMap, StatusCode), StatusCode> {
    // Get WebAuthn instance
    let webauthn = app_state
        .webauthn
        .as_ref()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;

    // Get authentication state
    let authentication_state = {
        let mut login_state = app_state.login_state.write().await;
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

    // Get user database manager
    let db_manager = app_state
        .user_database_manager
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Find user by credential ID and update passkey
    let username = {
        let mut db = db_manager.database().write().await;
        let mut found_username = None;

        for (user_name, user) in db.users.iter_mut() {
            for passkey in user.passkeys.iter_mut() {
                if passkey.credential.cred_id() == &auth_data.raw_id {
                    // Update last used time
                    passkey.update_last_used();
                    // Update the credential with counter
                    passkey.credential.update_credential(&authentication_result);
                    found_username = Some(user_name.clone());
                    break;
                }
            }
            if found_username.is_some() {
                break;
            }
        }

        found_username
    };

    if let Some(username) = username {
        // Save updated database
        db_manager
            .save()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // Create session cookie
        let signed_value =
            crate::api::create_signed_cookie(&app_state.config.app.cookie_secret, &username)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let cookie = format!(
            "auth={}; Path=/; Max-Age=604800; HttpOnly; SameSite=Lax",
            signed_value
        );

        let mut headers = HeaderMap::new();
        headers.insert("Set-Cookie", cookie.parse().unwrap());

        info!("User {} authenticated via passkey", username);

        Ok((headers, StatusCode::OK))
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

pub async fn list_passkeys(
    State(app_state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<PasskeyInfo>>, StatusCode> {
    // Check if user is authenticated
    let username = get_authenticated_user(&headers, &app_state.config.app.cookie_secret)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Get user database manager
    let db_manager = app_state
        .user_database_manager
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get user and map passkeys to info
    let passkey_info = {
        let db = db_manager.database().read().await;
        let user = db.get_user(&username).ok_or(StatusCode::NOT_FOUND)?;

        // Map passkeys to info
        user.passkeys
            .iter()
            .map(|pk| PasskeyInfo {
                id: pk.id,
                name: pk.name.clone(),
                created_at: pk.created_at,
                last_used_at: pk.last_used_at,
            })
            .collect()
    };

    Ok(Json(passkey_info))
}

pub async fn delete_passkey(
    State(app_state): State<AppState>,
    headers: HeaderMap,
    Path(passkey_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    // Check if user is authenticated
    let username = get_authenticated_user(&headers, &app_state.config.app.cookie_secret)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Get user database manager
    let db_manager = app_state
        .user_database_manager
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Remove passkey from user
    let removed = {
        let mut db = db_manager.database().write().await;
        if let Some(user) = db.get_user_mut(&username) {
            user.remove_passkey(&passkey_id)
        } else {
            return Err(StatusCode::NOT_FOUND);
        }
    };

    if removed {
        // Save database
        db_manager
            .save()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        info!("Passkey {} deleted for user: {}", passkey_id, username);
        Ok(StatusCode::OK)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

pub async fn check_user_has_passkeys(
    State(app_state): State<AppState>,
    Json(request): Json<StartAuthenticationRequest>,
) -> Result<Json<HasPasskeysResponse>, StatusCode> {
    // Get user database manager
    let db_manager = app_state
        .user_database_manager
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Check if user has passkeys
    let db = db_manager.database().read().await;
    if let Some(user) = db.get_user_by_username_or_email(&request.username) {
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
    State(app_state): State<AppState>,
    headers: HeaderMap,
    Path(passkey_id): Path<Uuid>,
    Json(name): Json<String>,
) -> Result<StatusCode, StatusCode> {
    // Check if user is authenticated
    let username = get_authenticated_user(&headers, &app_state.config.app.cookie_secret)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Get user database manager
    let db_manager = app_state
        .user_database_manager
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Update passkey name
    let updated = {
        let mut db = db_manager.database().write().await;
        if let Some(user) = db.get_user_mut(&username) {
            if let Some(passkey) = user.get_passkey_mut(&passkey_id) {
                passkey.name = name;
                true
            } else {
                false
            }
        } else {
            return Err(StatusCode::NOT_FOUND);
        }
    };

    if updated {
        // Save database
        db_manager
            .save()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        info!("Passkey {} name updated for user: {}", passkey_id, username);
        Ok(StatusCode::OK)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
