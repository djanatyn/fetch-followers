#![feature(async_closure)]
#![feature(type_ascription)]

use egg_mode::user::TwitterUser;
use egg_mode::{self, Token};
use futures::StreamExt;
use miette::{self, Diagnostic};
use opentelemetry::global::shutdown_tracer_provider;
use opentelemetry::{
    global,
    sdk::trace,
    trace::{get_active_span, Tracer},
    KeyValue,
};
use opentelemetry_otlp::{ExportConfig, Protocol, WithExportConfig};
use serde::Deserialize;
use std::str::FromStr;
use std::time::Duration;
use thiserror::Error;
use tokio::time::{self, sleep};
use tonic::{
    metadata::{MetadataKey, MetadataMap},
    transport::ClientTlsConfig,
};

const PAGE_SIZE: usize = 200;

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

async fn fetch_users(token: &Token) -> miette::Result<Vec<TwitterUser>> {
    Ok(egg_mode::user::friends_of("djanatyn", &token)
        .with_page_size(PAGE_SIZE.try_into().unwrap())
        .enumerate()
        .fold(vec![], |mut friends, (n, response)| async move {
            // retrieve user
            let user = match response {
                Ok(response) => dbg!(response).response,
                Err(error) => panic!("failed to fetch all friends: {error}"),
            };

            // add user to list
            friends.push(user.clone());

            // record user found as event
            get_active_span(|span| {
                span.add_event(
                    "friend found",
                    vec![
                        KeyValue::new("name", dbg!(user.name)),
                        KeyValue::new("screen_name", dbg!(user.screen_name)),
                    ],
                );
            });

            // sleep before making a network call
            if n % PAGE_SIZE == 0 {
                sleep(time::Duration::from_secs(3)).await
            };

            // return accumulated users each step
            friends
        })
        .await)
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    // load config, setup tracing
    let config = load_config()?;
    let _ = init_tracer(&config);

    let tracer = global::tracer("fetch-followers");
    tracer
        .in_span("start app", async move |_cx| {
            let token = Token::Bearer(config.fetch_followers_token);
            let users_you_follow: Vec<TwitterUser> = fetch_users(&token).await?;

            println!("done!");

            Ok(()): miette::Result<()>
        })
        .await;

    shutdown_tracer_provider();

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn load_config() {
        todo!();
    }
}
