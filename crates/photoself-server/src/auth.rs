use std::time::{Duration, Instant};

use argon2::password_hash::{PasswordHash, PasswordVerifier};
use argon2::Argon2;
use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use crate::state::AppState;

const AUTH_CACHE_TTL: Duration = Duration::from_secs(300);

pub async fn require_auth(State(state): State<AppState>, request: Request, next: Next) -> Response {
    match state.config.auth.mode.as_str() {
        "none" => next.run(request).await,
        "proxy" => {
            let has_user = request
                .headers()
                .get("x-forwarded-user")
                .and_then(|value| value.to_str().ok())
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false);
            if !has_user {
                return unauthorized(false);
            }
            let Some(peer) = request
                .extensions()
                .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                .map(|info| info.0)
            else {
                return unauthorized(false);
            };
            if !state.is_trusted_proxy(peer.ip()) {
                tracing::warn!(peer = %peer, "rejected X-Forwarded-User from untrusted peer");
                return unauthorized(false);
            }
            next.run(request).await
        }
        "basic" => {
            let Some(value) = request
                .headers()
                .get(header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
            else {
                return unauthorized(true);
            };
            let Some(encoded) = value.strip_prefix("Basic ") else {
                return unauthorized(true);
            };

            if is_cached(&state, encoded) {
                return next.run(request).await;
            }

            let decoded = match STANDARD.decode(encoded) {
                Ok(decoded) => decoded,
                Err(_) => return unauthorized(true),
            };
            let decoded = match String::from_utf8(decoded) {
                Ok(decoded) => decoded,
                Err(_) => return unauthorized(true),
            };
            let Some((username, password)) = decoded.split_once(':') else {
                return unauthorized(true);
            };

            let expected_user = state.config.auth.username.clone().unwrap_or_default();
            let hash = state.config.auth.password_hash.clone().unwrap_or_default();
            let username = username.to_string();
            let password = password.to_string();

            let valid = tokio::task::spawn_blocking(move || {
                verify_basic(&username, &password, &expected_user, &hash)
            })
            .await
            .unwrap_or(false);

            if !valid {
                return unauthorized(true);
            }

            cache_auth(&state, encoded);
            next.run(request).await
        }
        _ => unauthorized(false),
    }
}

fn verify_basic(username: &str, password: &str, expected_user: &str, hash: &str) -> bool {
    if username != expected_user {
        return false;
    }
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

fn is_cached(state: &AppState, encoded: &str) -> bool {
    let mut cache = state.auth_cache.lock().unwrap();
    match cache.get(encoded) {
        Some(timestamp) if timestamp.elapsed() < AUTH_CACHE_TTL => true,
        Some(_) => {
            cache.remove(encoded);
            false
        }
        None => false,
    }
}

fn cache_auth(state: &AppState, encoded: &str) {
    let mut cache = state.auth_cache.lock().unwrap();
    if cache.len() > 1024 {
        cache.retain(|_, timestamp| timestamp.elapsed() < AUTH_CACHE_TTL);
    }
    cache.insert(encoded.to_string(), Instant::now());
}

fn unauthorized(basic: bool) -> Response {
    let mut response = (
        axum::http::StatusCode::UNAUTHORIZED,
        "authentication required",
    )
        .into_response();
    if basic {
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            "Basic realm=\"photoSelf\"".parse().unwrap(),
        );
    }
    response
}
