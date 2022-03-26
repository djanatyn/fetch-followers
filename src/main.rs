#![feature(async_closure)]

use egg_mode::user::TwitterUser;
use egg_mode::{self, Token};
use futures::StreamExt;
use futures::TryStreamExt;
use miette::{self, Diagnostic};
use opentelemetry::{
    sdk::trace,
    trace::{TraceContextExt, Tracer},
    KeyValue,
};
use opentelemetry_otlp::{ExportConfig, Protocol, WithExportConfig};
use serde::Deserialize;
use std::str::FromStr;
use std::time::Duration;
use thiserror::Error;
use tonic::{
    metadata::{MetadataKey, MetadataMap},
    transport::ClientTlsConfig,
};

const PAGE_SIZE: i32 = 20;
const MAX_USERS: usize = 20; // TODO: increase

#[derive(Deserialize, Debug)]
struct Config {
    fetch_followers_token: String,
    honeycomb_team: String,
    honeycomb_dataset: String,
}

#[derive(Error, Debug, Diagnostic)]
enum AppError {
    #[error("failed to load environment variable: {0:?}")]
    MissingVariables(envy::Error),
}

const HONEYCOMB_ENDPOINT: &str = "https://api.honeycomb.io:443";
const HONEYCOMB_DOMAIN: &str = "api.honeycomb.io";

/// Try to load Twitter API Bearer token from environment variables.
fn load_config() -> Result<Config, AppError> {
    match envy::from_env::<Config>() {
        Ok(config) => {
            println!("loaded config!");
            Ok(config)
        }
        Err(error) => Err(AppError::MissingVariables(error)),
    }
}

/// Set up OpenTelemetry OTLP tracer, exporting to Honeycomb.
fn init_tracer(config: &Config) -> trace::Tracer {
    let export_config = ExportConfig {
        endpoint: HONEYCOMB_ENDPOINT.to_string(),
        timeout: Duration::from_secs(10),
        protocol: Protocol::Grpc,
    };

    let mut metadata = MetadataMap::new();
    metadata.insert(
        MetadataKey::from_str("x-honeycomb-team").unwrap(),
        config.honeycomb_team.parse().unwrap(),
    );
    metadata.insert(
        MetadataKey::from_str("x-honeycomb-dataset").unwrap(),
        config.honeycomb_dataset.parse().unwrap(),
    );

    opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_export_config(export_config)
                .with_metadata(dbg!(metadata))
                .with_tls_config(ClientTlsConfig::new().domain_name(HONEYCOMB_DOMAIN)),
        )
        .install_batch(opentelemetry::runtime::Tokio)
        .expect("failed to create tracer")
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    // load config, setup tracing
    let config = load_config()?;
    let tracer = init_tracer(&config);

    tracer
        .in_span("start app", async move |cx| {
            let token = Token::Bearer(config.fetch_followers_token);
            let friends: egg_mode::error::Result<Vec<TwitterUser>> =
                egg_mode::user::friends_of("djanatyn", &token)
                    .with_page_size(PAGE_SIZE)
                    .take(MAX_USERS)
                    .map_ok(|r| r.response)
                    .try_collect::<Vec<_>>()
                    .await;

            if let Ok(friends) = friends {
                for friend in friends {
                    cx.span().add_event(
                        "friend found",
                        vec![KeyValue::new("name", dbg!(friend.name))],
                    );
                    // event!(Level::INFO, ?friend.name, ?friend.screen_name, "friend found");
                }
            } else {
                println!("{friends:#?}");
            }

            println!("done!");

            Ok(())
        })
        .await
}

#[cfg(test)]
mod tests {
    #[test]
    fn load_config() {
        todo!();
    }
}
