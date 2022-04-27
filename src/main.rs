#![feature(async_closure)]

use egg_mode::error::Error;
use egg_mode::user::{self, TwitterUser};
use egg_mode::{self, Token};
use miette::{self, Diagnostic};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// TODO: paginated queries, checking rate limits
// TODO: serialize to diesel database
//
// poll table:
// - primary key id
// - date start
// - date finish
// - count follower
// - count following
//
// user table:
// - primary key account id
// - date account creation
//
// user snapshots:
// - foreign key account id
// - string screen_name
// - string location
// - string account description (you must replace URLs)
// - string display url
// - int follow count
// - int following count
// - int favorites count
// - int status count
// - bool verified

const PAGE_SIZE: usize = 200;
const ME: &str = "djanatyn";

#[derive(Serialize)]
pub struct TwitterUserRef<'a>(#[serde(with = "TwitterUser")] &'a TwitterUser);

#[derive(Deserialize, Debug)]
struct Config {
    fetch_followers_token: String,
}

#[derive(Error, Debug, Diagnostic)]
enum AppError {
    #[error("failed to load environment variable: {0:?}")]
    MissingVariables(envy::Error),
}

#[derive(Serialize, Debug)]
struct Output {
    followers: Vec<TwitterUser>,
    following: Vec<TwitterUser>,
}

/// Try to load Twitter API Bearer token from environment variables.
fn load_config() -> Result<Config, AppError> {
    match envy::from_env::<Config>() {
        Ok(config) => Ok(config),
        Err(error) => Err(AppError::MissingVariables(error)),
    }
}

/// Fetch my followers.
async fn fetch_followers(token: &Token) -> miette::Result<Vec<TwitterUser>> {
    // accumulate users
    let mut users: Vec<TwitterUser> = Vec::new();

    // fetch first set of followers
    let mut followers = user::followers_of(ME, token).with_page_size(PAGE_SIZE.try_into().unwrap());
    let mut resp = followers.call().await;

    // retry if we hit rate limit
    if let Err(Error::RateLimit(timestamp)) = resp {
        todo!("add miette handler for rate limit in first call: {timestamp}")
    }

    // loop over successful, non-empty responses
    while let Ok(ref mut response) = resp {
        // break if there are no users in the response
        if users.is_empty() {
            break;
        }

        users.append(&mut response.users);
        println!("{users:#?}");

        // get next page
        followers.next_cursor = response.next_cursor;
        resp = followers.call().await;

        // retry for rate limit
        if let Err(Error::RateLimit(timestamp)) = resp {
            todo!("need to wait for rate limit: {timestamp}")
        }
    }

    // return accumulated users
    Ok(users)
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    // load config, setup tracing
    let config = load_config()?;

    // construct bearer token for twitter API
    let token = Token::Bearer(config.fetch_followers_token);

    // retrieve followers + following
    // let following: Vec<TwitterUser> = fetch_following(&token).await?;
    let followers: Vec<TwitterUser> = fetch_followers(&token).await?;

    // // output as JSON
    // let output = Output {
    //     following,
    //     followers,
    // };

    // let json = serde_json::to_string(&output).expect("failed to convert to JSON");
    // print!("{json}");

    println!("done");
    Ok(())
}
