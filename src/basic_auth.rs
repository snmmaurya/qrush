use actix_web::{HttpRequest, HttpResponse};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crate::config::get_basic_auth;


pub fn check_basic_auth(req: &HttpRequest) -> bool {
    if let Some(config) = get_basic_auth() {
        if let Some(auth_header) = req.headers().get("Authorization") {
            if let Ok(auth_str) = auth_header.to_str() {
                if let Some(encoded) = auth_str.strip_prefix("Basic ") {
                    if let Ok(decoded) = STANDARD.decode(encoded) {
                        if let Ok(credentials) = std::str::from_utf8(&decoded) {
                            let parts: Vec<&str> = credentials.splitn(2, ':').collect();
                            if parts.len() == 2 && parts[0] == config.username && parts[1] == config.password {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        return false;
    }
    true
}

pub fn unauthorized_response() -> HttpResponse {
    HttpResponse::Unauthorized()
        .append_header(("WWW-Authenticate", r#"Basic realm="SideQueue""#))
        .finish()
}
