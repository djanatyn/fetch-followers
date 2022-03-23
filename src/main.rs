use egg_mode::user::TwitterUser;
use egg_mode::{self, Token};
use futures::StreamExt;
use futures::TryStreamExt;
use miette::{self, Diagnostic};
use serde::Deserialize;
use thiserror::Error;
use tracing::{event, span, Level};

const PAGE_SIZE: i32 = 20;
const MAX_USERS: usize = 100; // TODO: increase

#[derive(Deserialize, Debug)]
struct Config {
    fetch_followers_token: String,
}

#[derive(Error, Debug, Diagnostic)]
enum AppError {
    #[error("failed to load environment variable: {0:?}")]
    MissingVariables(envy::Error),
}

/// Try to load Twitter API Bearer token from environment variables.
fn load_token() -> Result<Config, AppError> {
    let span = span!(Level::INFO, "loading bearer token");
    let _ = span.enter();
    match envy::from_env::<Config>() {
        Ok(config) => {
            event!(Level::TRACE, "successfully loaded token");
            Ok(config)
        }
        Err(error) => {
            event!(Level::TRACE, ?error, "failed to load token");
            Err(AppError::MissingVariables(error))
        }
    }
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    // start session
    tracing_subscriber::fmt::init();
    let span = span!(Level::INFO, "session");
    let _ = span.enter();

    // load token
    let config = load_token()?;

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
            let name = friend.screen_name;
            println!("{name}");
        }
    } else {
        println!("{friends:#?}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn load_config() {
        todo!();
    }
}
