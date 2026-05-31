use authifier::models::Account;
use revolt_config::config;
use revolt_database::{Database, PartialUser};
use revolt_models::v0::UserFlags;
use revolt_result::Result;

use rocket::response::Redirect;
use rocket::serde::Deserialize;
use rocket::State;

use authifier::Authifier;

use std::time::Duration;

use super::get_oidc_endpoints;
use super::login::CODE_VERIFIERS;
use super::sso_error_redirect;

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct UserInfo {
    email: String,
    #[serde(default)]
    preferred_username: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[openapi(tag = "SSO")]
#[get("/callback?<code>&<state>")]
pub async fn sso_callback(
    code: String,
    state: String,
    db: &State<Database>,
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

    let code_verifier = {
        let mut verifiers = CODE_VERIFIERS
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let Some((verifier, created)) = verifiers.remove(&state) else {
            return Ok(sso_error_redirect(&settings.hosts.app, "sso_error"));
        };
        if created.elapsed() >= Duration::from_secs(600) {
            return Ok(sso_error_redirect(&settings.hosts.app, "sso_error"));
        }
        verifier
    };

    let token_body = format!(
        "grant_type=authorization_code&code={}&client_id={}&client_secret={}&redirect_uri={}&code_verifier={}",
        url_escape::encode_component(&code),
        url_escape::encode_component(&sso.client_id),
        url_escape::encode_component(&sso.client_secret),
        url_escape::encode_component(&sso.redirect_uri),
        url_escape::encode_component(&code_verifier),
    );

    let client = reqwest::Client::new();
    let token_response = client
        .post(&endpoints.token_endpoint)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(token_body)
        .send()
        .await;
    let token_resp: TokenResponse = match token_response {
        Ok(resp) => match resp.json().await {
            Ok(json) => json,
            Err(_) => return Ok(sso_error_redirect(&settings.hosts.app, "sso_error")),
        },
        Err(_) => return Ok(sso_error_redirect(&settings.hosts.app, "sso_error")),
    };

    let userinfo_response = client
        .get(&endpoints.userinfo_endpoint)
        .header(
            "Authorization",
            format!("Bearer {}", token_resp.access_token),
        )
        .send()
        .await;
    let user_info: UserInfo = match userinfo_response {
        Ok(resp) => match resp.json().await {
            Ok(json) => json,
            Err(_) => return Ok(sso_error_redirect(&settings.hosts.app, "sso_error")),
        },
        Err(_) => return Ok(sso_error_redirect(&settings.hosts.app, "sso_error")),
    };

    let email_normalised = user_info.email.to_lowercase();

    let existing_account = match authifier
        .database
        .find_account_by_normalised_email(&email_normalised)
        .await
    {
        Ok(account) => account,
        Err(_) => return Ok(sso_error_redirect(&settings.hosts.app, "sso_error")),
    };

    let account = if let Some(account) = existing_account {
        if account.disabled {
            return Ok(sso_error_redirect(&settings.hosts.app, "sso_error"));
        }
        account
    } else {
        let dummy_password = nanoid::nanoid!(64);
        match Account::new(authifier, user_info.email, dummy_password, false).await {
            Ok(account) => account,
            Err(_) => return Ok(sso_error_redirect(&settings.hosts.app, "sso_error")),
        }
    };

    let session = match account
        .create_session(authifier, "SSO".to_string())
        .await
    {
        Ok(s) => s,
        Err(_) => return Ok(sso_error_redirect(&settings.hosts.app, "sso_error")),
    };

    let existing_user = db.fetch_user(&account.id).await.ok();

    if let Some(mut existing_user) = existing_user {
        let current_flags = existing_user.flags.unwrap_or(0);
        if current_flags & (UserFlags::Sso as i32) == 0 {
            let new_flags = current_flags | (UserFlags::Sso as i32);
            existing_user.flags = Some(new_flags);
            if existing_user
                .update(
                    db,
                    PartialUser {
                        flags: Some(new_flags),
                        ..Default::default()
                    },
                    vec![],
                )
                .await
                .is_err()
            {
                return Ok(sso_error_redirect(&settings.hosts.app, "sso_error"));
            }
        }
    } else {
        let username = user_info
            .preferred_username
            .or(user_info.name)
            .unwrap_or_else(|| format!("user_{}", &account.id[..8]));

        let partial = PartialUser {
            flags: Some(UserFlags::Sso as i32),
            ..Default::default()
        };

        match revolt_database::User::create(db, username, account.id.clone(), Some(partial)).await
        {
            Ok(_) => {}
            Err(_) => return Ok(sso_error_redirect(&settings.hosts.app, "sso_error")),
        }
    }

    let app_url = settings.hosts.app.clone();
    Ok(Redirect::to(format!(
        "{}/login/sso?token={}",
        app_url, session.token
    )))
}
