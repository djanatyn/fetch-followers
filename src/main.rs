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
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::time::Duration;
use thiserror::Error;
use tokio::time::{self, sleep};
use tonic::{
    metadata::{MetadataKey, MetadataMap},
    transport::ClientTlsConfig,
};

const PAGE_SIZE: usize = 200;

// https://serde.rs/remote-derive.html
// https://docs.rs/egg-mode/0.16.0/src/egg_mode/user/mod.rs.html#165
#[derive(Serialize, Debug, Clone)]
#[serde(remote = "TwitterUser")]
pub struct TwitterUserRef {
    pub contributors_enabled: bool,
    // pub created_at: chrono::DateTime<chrono::Utc>,
    pub default_profile: bool,
    pub default_profile_image: bool,
    pub description: Option<String>,
    // pub entities: UserEntities,
    pub favourites_count: i32,
    pub follow_request_sent: Option<bool>,
    pub followers_count: i32,
    pub friends_count: i32,
    pub geo_enabled: bool,
    pub id: u64,
    pub is_translator: bool,
    pub lang: Option<String>,
    pub listed_count: i32,
    pub location: Option<String>,
    pub name: String,
    pub profile_background_color: String,
    pub profile_background_image_url: Option<String>,
    pub profile_background_image_url_https: Option<String>,
    pub profile_background_tile: Option<bool>,
    pub profile_banner_url: Option<String>,
    pub profile_image_url: String,
    pub profile_image_url_https: String,
    pub profile_link_color: String,
    pub profile_sidebar_border_color: String,
    pub profile_sidebar_fill_color: String,
    pub profile_text_color: String,
    pub profile_use_background_image: bool,
    pub protected: bool,
    pub screen_name: String,
    pub show_all_inline_media: Option<bool>,
    // pub status: Option<Box<tweet::Tweet>>,
    pub statuses_count: i32,
    pub time_zone: Option<String>,
    pub url: Option<String>,
    pub utc_offset: Option<i32>,
    pub verified: bool,
    pub withheld_in_countries: Option<Vec<String>>,
    pub withheld_scope: Option<String>,
}

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
        Ok(config) => Ok(config),
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
    Ok(egg_mode::user::friends_of("djanatyn", token)
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

            let json = serde_json::to_string(&users_you_follow);
            println!("{json:#?}");

            Ok(()): miette::Result<()>
        })
        .await?;

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
