#![feature(async_closure)]

use egg_mode::user::{self, TwitterUser};
use egg_mode::{self, Response, Token};
use futures::StreamExt;
use miette::{self, Diagnostic};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::time::Duration;
use thiserror::Error;
use tokio::time::sleep;

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
const SLEEP_DURATION: Duration = Duration::from_secs(3);
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

/// TODO: https://docs.rs/egg-mode/latest/egg_mode/cursor/struct.CursorIter.html#manual-paging
/// ````
/// let mut list = egg_mode::user::followers_of("rustlang", &token).with_page_size(20);
/// let resp = list.call().await.unwrap();

/// for user in resp.response.users {
///     println!("{} (@{})", user.name, user.screen_name);
/// }

/// list.next_cursor = resp.response.next_cursor;
/// let resp = list.call().await.unwrap();

/// for user in resp.response.users {
///     println!("{} (@{})", user.name, user.screen_name);
/// }
/// ```
///

/// Walk through paginated, enumerated results.
async fn walk_pages(
    mut friends: Vec<TwitterUser>,
    (n, response): (usize, egg_mode::error::Result<Response<TwitterUser>>),
) -> Vec<TwitterUser> {
    // retrieve user
    let user = match response {
        Ok(response) => response.response,
        Err(error) => panic!("failed to fetch all friends: {error}"),
    };

    // add user to list
    friends.push(user.clone());

    // sleep before making a network call
    if n % PAGE_SIZE == 0 {
        sleep(SLEEP_DURATION).await
    };

    // return accumulated users each step
    friends
}

/// Fetch who I'm following.
async fn fetch_following(token: &Token) -> miette::Result<Vec<TwitterUser>> {
    Ok(user::friends_of(ME, token)
        .with_page_size(PAGE_SIZE.try_into().unwrap())
        .enumerate()
        .fold(vec![], walk_pages)
        .await)
}

/// Fetch my followers.
async fn fetch_followers(token: &Token) -> miette::Result<Vec<TwitterUser>> {
    Ok(user::followers_of(ME, token)
        .with_page_size(PAGE_SIZE.try_into().unwrap())
        .enumerate()
        .fold(vec![], walk_pages)
        .await)
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    // load config, setup tracing
    let config = load_config()?;

    // construct bearer token for twitter API
    let token = Token::Bearer(config.fetch_followers_token);

    // retrieve followers + following
    // let following: Vec<TwitterUser> = fetch_following(&token).await?;
    // let followers: Vec<TwitterUser> = fetch_followers(&token).await?;

    // // output as JSON
    // let output = Output {
    //     following,
    //     followers,
    // };

    // let json = serde_json::to_string(&output).expect("failed to convert to JSON");
    // print!("{json}");

    Ok(())
}
