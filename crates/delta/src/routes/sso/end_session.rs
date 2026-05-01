use revolt_config::config;
use revolt_result::Result;

use rocket::response::Redirect;
use rocket::State;

use authifier::Authifier;

use super::get_oidc_endpoints;
use super::sso_error_redirect;

#[openapi(tag = "SSO")]
#[get("/end-session?<session_token>")]
pub async fn sso_end_session(
    session_token: Option<String>,
    authifier: &State<Authifier>,
) -> Result<Redirect> {
    let settings = config().await;

    let sso = match &settings.sso {
        Some(s) if s.enabled => s,
        _ => return Ok(sso_error_redirect(&settings.hosts.app, "sso_disabled")),
    };

    let endpoints = match get_oidc_endpoints(&sso.issuer_url).await {
        Ok(ep) => ep,
        Err(_) => return Ok(sso_error_redirect(&settings.hosts.app, "sso_error")),
    };

    if let Some(token) = &session_token {
        if let Ok(Some(session)) = authifier
            .database
            .find_session_by_token(token)
            .await
        {
            let _ = authifier.database.delete_session(&session.id).await;
        }
    }

    if let Some(end_session_ep) = &endpoints.end_session_endpoint {
        Ok(Redirect::to(format!(
            "{}?post_logout_redirect_uri={}",
            end_session_ep,
            url_escape::encode_component(&settings.hosts.app),
        )))
    } else {
        let app_url = settings.hosts.app.clone();
        Ok(Redirect::to(app_url))
    }
}
