use revolt_config::config;
use revolt_result::Result;

use rocket::response::Redirect;

use super::get_oidc_endpoints;
use super::sso_error_redirect;

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;

use rand::RngCore;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

pub(crate) static CODE_VERIFIERS: Lazy<Mutex<HashMap<String, (String, Instant)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn generate_code_verifier() -> String {
    let mut random_bytes = [0u8; 64];
    rand::thread_rng().fill_bytes(&mut random_bytes);
    URL_SAFE_NO_PAD.encode(random_bytes)
}

fn generate_code_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();
    URL_SAFE_NO_PAD.encode(hash)
}

#[openapi(tag = "SSO")]
#[get("/login")]
pub async fn sso_login() -> Result<Redirect> {
    let settings = config().await;

    let sso = match &settings.sso {
        Some(s) if s.enabled => s,
        _ => return Ok(sso_error_redirect(&settings.hosts.app, "sso_disabled")),
    };

    let endpoints = match get_oidc_endpoints(&sso.issuer_url).await {
        Ok(ep) => ep,
        Err(_) => return Ok(sso_error_redirect(&settings.hosts.app, "sso_error")),
    };

    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);
    let state = nanoid::nanoid!(32);

    {
        let mut verifiers = CODE_VERIFIERS
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        verifiers.retain(|_, (_, created)| created.elapsed() < Duration::from_secs(600));
        verifiers.insert(state.clone(), (code_verifier, Instant::now()));
    }

    let authorize_url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope=openid+email+profile&state={}&code_challenge={}&code_challenge_method=S256",
        endpoints.authorization_endpoint,
        url_escape::encode_component(&sso.client_id),
        url_escape::encode_component(&sso.redirect_uri),
        url_escape::encode_component(&state),
        url_escape::encode_component(&code_challenge),
    );

    Ok(Redirect::to(authorize_url))
}
