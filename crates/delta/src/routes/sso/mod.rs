mod callback;
mod discovery;
mod end_session;
mod login;

pub use discovery::get_oidc_endpoints;

use revolt_rocket_okapi::revolt_okapi::openapi3::OpenApi;
use rocket::response::Redirect;
use rocket::Route;

pub(crate) fn sso_error_redirect(app_url: &str, error: &str) -> Redirect {
    Redirect::to(format!(
        "{}/login?error={}",
        app_url,
        url_escape::encode_component(error)
    ))
}

pub fn routes() -> (Vec<Route>, OpenApi) {
    openapi_get_routes_spec![
        login::sso_login,
        callback::sso_callback,
        end_session::sso_end_session,
    ]
}
