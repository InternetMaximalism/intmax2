use actix_web::{
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    HttpMessage,
};
use opentelemetry::trace::TracerProvider as _;
use std::time::Instant;
use thiserror::Error;
use tracing::Span;
use tracing_actix_web::RootSpanBuilder;
use tracing_subscriber::{
    fmt,
    layer::SubscriberExt as _,
    util::{SubscriberInitExt as _, TryInitError},
    EnvFilter,
};

use crate::{env::Env, health_check::load_name_and_version, tracer};
use common::env::EnvType;

#[derive(Error, Debug)]
pub enum InitLoggerError {
    #[error("Failed to initialize logger: {0}")]
    SetGlobalSubscriberError(#[from] tracing::subscriber::SetGlobalDefaultError),

    #[error("Failed to initialize logger: {0}")]
    TryInitError(#[from] TryInitError),
}

pub fn init_logger() -> Result<(), InitLoggerError> {
    // Get package info for log file naming
    let (name, version) = load_name_and_version();

    dotenv::dotenv().ok();
    let env = envy::from_env::<Env>().expect("Failed to load environment variables");
    let env_filter = EnvFilter::new(env.app_log);

    // Initialize the global subscriber
    if env.env == EnvType::Local {
        // log to stdout with human-readable log format
        let subscriber = fmt::Layer::new().with_line_number(true);
        tracing_subscriber::registry()
            .with(subscriber)
            .with(env_filter)
            .try_init()?;
    } else {
        let provider = tracer::init_tracer(
            name.as_str(),
            version.as_str(),
            env.otlp_collector_endpoint.as_str(),
        );

        // log to stdout with json structured log format
        let subscriber = fmt::Layer::new()
            .with_target(false)
            .with_span_events(fmt::format::FmtSpan::NEW | fmt::format::FmtSpan::CLOSE)
            .json();

        if let Some(p) = provider {
            // NOTE: setup with otlp tracer when set OTLP_COLLECTOR_ENDPOINT env value
            let otlp_tracer = p.tracer(name);
            tracing_subscriber::registry()
                .with(subscriber)
                .with(env_filter)
                .with(tracing_opentelemetry::layer().with_tracer(otlp_tracer))
                .try_init()?;
        } else {
            tracing_subscriber::registry()
                .with(subscriber)
                .with(env_filter)
                .try_init()?;
        };
    }
    Ok(())
}

pub struct CustomRootSpanBuilder;

impl RootSpanBuilder for CustomRootSpanBuilder {
    fn on_request_start(request: &ServiceRequest) -> tracing::Span {
        request.extensions_mut().insert(Instant::now());
        let span = tracing::info_span!(
            "http-request",
            "http.client_ip" = %request.connection_info().peer_addr().unwrap_or(""),
            "http.flavor" = ?request.version(),
            "http.host" = %request.connection_info().host(),
            "http.method" = %request.method(),
            "http.route" = %request.path(),
            "http.scheme" = %request.connection_info().scheme(),
            "http.user_agent" = %request.headers().get("user-agent").and_then(|h| h.to_str().ok()).unwrap_or(""),
            "otel.kind" = "server",
            "otel.name" = %format!("{} {}", request.method(), request.path()),
            "request_id" = %uuid::Uuid::new_v4(),
            status_code = tracing::field::Empty,
            latency = tracing::field::Empty,
        );
        span
    }

    fn on_request_end<B: MessageBody>(
        span: Span,
        response: &Result<ServiceResponse<B>, actix_web::Error>,
    ) {
        match response {
            Ok(resp) => {
                span.record("status_code", resp.status().as_u16());
                if let Some(start_time) = resp.request().extensions().get::<Instant>() {
                    span.record("latency", format!("{:?}", start_time.elapsed()));
                }
                tracing::info!("request-end");
            }
            Err(error) => {
                span.record("error", error.to_string());
                tracing::error!(error = %error, "request failed");
            }
        }
    }
}
