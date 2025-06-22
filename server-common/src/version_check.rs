use actix_web::{
    body::EitherBody,
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpResponse, Result,
};
use futures_util::future::LocalBoxFuture;
use semver::Version;
use std::{
    future::{ready, Ready},
    str::FromStr,
};

#[derive(Clone)]
pub struct VersionCheck {
    min_version: String,
}

impl VersionCheck {
    pub fn new(min_version: &str) -> Self {
        Self {
            min_version: min_version.to_string(),
        }
    }

    fn is_version_supported(&self, version: &str) -> bool {
        match (
            Version::from_str(version),
            Version::from_str(&self.min_version),
        ) {
            (Ok(current), Ok(minimum)) => current >= minimum,
            _ => {
                // If version parsing fails, we assume the version is not supported
                false
            }
        }
    }
}

impl<S, B> Transform<S, ServiceRequest> for VersionCheck
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = VersionCheckMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(VersionCheckMiddleware {
            service,
            config: self.clone(),
        }))
    }
}

pub struct VersionCheckMiddleware<S> {
    service: S,
    config: VersionCheck,
}

impl<S, B> Service<ServiceRequest> for VersionCheckMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        // Extract version from Client-Version header
        let version = req
            .headers()
            .get("client-version")
            .and_then(|header| header.to_str().ok())
            .map(|s| s.to_string());

        if let Some(version) = version {
            if self.config.is_version_supported(&version) {
                // Version is acceptable
                let fut = self.service.call(req);
                Box::pin(async move {
                    let res = fut.await?;
                    Ok(res.map_into_left_body())
                })
            } else {
                // Version is too old
                let min_version = self.config.min_version.clone();
                Box::pin(async move {
                    let response = HttpResponse::UpgradeRequired()
                        .json(serde_json::json!({
                            "error": "CLIENT_VERSION_TOO_OLD",
                            "message": format!(
                                "Client version {} is no longer supported. Please upgrade to {} or later.",
                                version, min_version
                            ),
                            "minimum_version": min_version
                        }));
                    Ok(req.into_response(response).map_into_right_body())
                })
            }
        } else {
            // No Client-Version header, skip version check
            let fut = self.service.call(req);
            Box::pin(async move {
                let res = fut.await?;
                Ok(res.map_into_left_body())
            })
        }
    }
}
