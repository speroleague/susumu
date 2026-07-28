use axum::{
    Json,
    extract::State,
    http::{HeaderMap, header},
    response::IntoResponse,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    AppState,
    db::{Database, SessionUser, verify_password},
    error::ServerError,
};

const SESSION_COOKIE: &str = "susumu_session";
const CSRF_COOKIE: &str = "susumu_csrf";
const CSRF_HEADER: &str = "x-susumu-csrf";
const SESSION_MAX_AGE: &str = "2592000";

#[derive(Debug, Deserialize)]
pub(crate) struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct UserResponse {
    id: String,
    email: String,
    display_name: String,
    roles: Vec<String>,
}

pub(crate) async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<impl IntoResponse, ServerError> {
    let email = request.email.trim().to_lowercase();
    let Some(user) = state.database.find_user(&email).await? else {
        return Err(ServerError::Unauthorized);
    };
    if !verify_password(&request.password, &user.password_hash)? {
        return Err(ServerError::Unauthorized);
    }
    let token = session_token();
    let csrf_token = session_token();
    state
        .database
        .create_session(&user.id, &token_hash(&token))
        .await?;
    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, cookie_header(&state, &token));
    headers.append(header::SET_COOKIE, csrf_cookie_header(&state, &csrf_token));
    Ok((headers, Json(UserResponse::from(user))))
}

pub(crate) async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ServerError> {
    if let Some(token) = cookie_value(&headers) {
        state.database.delete_session(&token_hash(token)).await?;
    }
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::SET_COOKIE,
        "susumu_session=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax"
            .parse()
            .unwrap(),
    );
    Ok((
        response_headers,
        Json(serde_json::json!({ "status": "logged_out" })),
    ))
}

pub(crate) async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<UserResponse>, ServerError> {
    let Some(token) = cookie_value(&headers) else {
        return Err(ServerError::Unauthorized);
    };
    let Some(user) = state.database.session_user(&token_hash(token)).await? else {
        return Err(ServerError::Unauthorized);
    };
    Ok(Json(UserResponse::from(user)))
}

pub(crate) async fn authenticated_user(
    database: &Database,
    headers: &HeaderMap,
) -> Result<SessionUser, ServerError> {
    let Some(token) = cookie_value(headers) else {
        return Err(ServerError::Unauthorized);
    };
    database
        .session_user(&token_hash(token))
        .await?
        .ok_or(ServerError::Unauthorized)
}

pub(crate) fn require_csrf(headers: &HeaderMap) -> Result<(), ServerError> {
    let Some(cookie) = cookie_value_named(headers, CSRF_COOKIE) else {
        return Err(ServerError::Forbidden);
    };
    let Some(header) = headers
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
    else {
        return Err(ServerError::Forbidden);
    };
    if cookie != header {
        return Err(ServerError::Forbidden);
    }
    Ok(())
}

impl From<super::db::UserRecord> for UserResponse {
    fn from(user: super::db::UserRecord) -> Self {
        Self {
            id: user.id,
            email: user.email,
            display_name: user.display_name,
            roles: user.roles,
        }
    }
}

impl From<SessionUser> for UserResponse {
    fn from(user: SessionUser) -> Self {
        Self {
            id: user.id,
            email: user.email,
            display_name: user.display_name,
            roles: user.roles,
        }
    }
}

fn session_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn cookie_value(headers: &HeaderMap) -> Option<&str> {
    cookie_value_named(headers, SESSION_COOKIE)
}

fn cookie_value_named<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let (cookie_name, value) = cookie.trim().split_once('=')?;
                (cookie_name == name).then_some(value)
            })
        })
}

fn cookie_header(state: &AppState, token: &str) -> axum::http::HeaderValue {
    let secure = if state.config.cookie_secure {
        "; Secure"
    } else {
        ""
    };
    format!("{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={SESSION_MAX_AGE}{secure}")
        .parse()
        .expect("session cookie header is valid")
}

fn csrf_cookie_header(state: &AppState, token: &str) -> axum::http::HeaderValue {
    let secure = if state.config.cookie_secure {
        "; Secure"
    } else {
        ""
    };
    format!("{CSRF_COOKIE}={token}; Path=/; SameSite=Lax; Max-Age={SESSION_MAX_AGE}{secure}")
        .parse()
        .expect("CSRF cookie header is valid")
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue, header};

    use super::{require_csrf, session_token, token_hash};

    #[test]
    fn session_tokens_are_opaque_and_hashed() {
        let token = session_token();
        assert!(token.len() >= 40);
        assert_ne!(token, token_hash(&token));
        assert_eq!(token_hash(&token), token_hash(&token));
    }

    #[test]
    fn csrf_requires_matching_cookie_and_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("susumu_csrf=token"),
        );
        headers.insert("x-susumu-csrf", HeaderValue::from_static("token"));
        assert!(require_csrf(&headers).is_ok());
        headers.insert("x-susumu-csrf", HeaderValue::from_static("wrong"));
        assert!(require_csrf(&headers).is_err());
    }
}
