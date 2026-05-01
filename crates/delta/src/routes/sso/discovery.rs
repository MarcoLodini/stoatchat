use once_cell::sync::OnceCell;
use revolt_result::{create_error, Result};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct OidcEndpoints {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub end_session_endpoint: Option<String>,
}

static OIDC_ENDPOINTS: OnceCell<OidcEndpoints> = OnceCell::new();

pub async fn get_oidc_endpoints(issuer_url: &str) -> Result<&OidcEndpoints> {
    if let Some(endpoints) = OIDC_ENDPOINTS.get() {
        return Ok(endpoints);
    }

    let discovery_url = format!(
        "{}/.well-known/openid-configuration",
        issuer_url.trim_end_matches('/')
    );

    let resp = reqwest::get(&discovery_url)
        .await
        .map_err(|_| create_error!(InternalError))?;

    let endpoints: OidcEndpoints = resp
        .json()
        .await
        .map_err(|_| create_error!(InternalError))?;

    OIDC_ENDPOINTS
        .set(endpoints)
        .map_err(|_| create_error!(InternalError))?;

    Ok(OIDC_ENDPOINTS.get().unwrap())
}
