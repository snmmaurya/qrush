// qrush/src/services/basic_auth_service.rs
use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpRequest, HttpResponse,
};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use futures_util::future::{ready, Either, Ready}; // futures_util
use std::future::Ready as StdReady;              // StdReady = for new_transform
use crate::config::get_basic_auth;

pub struct BasicAuthMiddleware;

impl<S> Transform<S, ServiceRequest> for BasicAuthMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse, Error = Error> + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse;
    type Error = Error;
    type InitError = ();
    type Transform = BasicAuthMiddlewareService<S>;
    type Future = StdReady<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        std::future::ready(Ok(BasicAuthMiddlewareService { service }))
    }
}

pub struct BasicAuthMiddlewareService<S> {
    service: S,
}

impl<S> Service<ServiceRequest> for BasicAuthMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse, Error = Error> + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse;
    type Error = Error;

    // Correct Future type: Either<Ready<Result<...>>, S::Future>
    type Future = Either<Ready<Result<Self::Response, Self::Error>>, S::Future>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        if check_basic_auth(req.request()) {
            Either::Right(self.service.call(req))
        } else {
            let response = req.into_response(unauthorized_response());
            Either::Left(ready(Ok(response)))
        }
    }
}

pub fn check_basic_auth(req: &HttpRequest) -> bool {
    if let Some(config) = get_basic_auth() {
        if let Some(auth_header) = req.headers().get("Authorization") {
            if let Ok(auth_str) = auth_header.to_str() {
                if let Some(encoded) = auth_str.strip_prefix("Basic ") {
                    if let Ok(decoded) = STANDARD.decode(encoded) {
                        if let Ok(credentials) = std::str::from_utf8(&decoded) {
                            let mut parts = credentials.splitn(2, ':');
                            let user = parts.next().unwrap_or_default();
                            let pass = parts.next().unwrap_or_default();
                            if user == config.username && pass == config.password {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        return false;
    }
    // If config is not set, allow by default
    true
}

pub fn unauthorized_response() -> HttpResponse {
    HttpResponse::Unauthorized()
        .append_header(("WWW-Authenticate", r#"Basic realm="QRush""#))
        .finish()
}
