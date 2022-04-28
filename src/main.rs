#![feature(async_closure)]

use chrono::{NaiveDate, NaiveDateTime};

use egg_mode::cursor::{CursorIter, UserCursor};
use egg_mode::error::Error;
use egg_mode::user::{self, TwitterUser};
use egg_mode::{self, Token};
use futures::future;
use miette::{self, Diagnostic};
use serde::Deserialize;
use thiserror::Error;
use tracing::{info_span, warn_span};

#[derive(Debug)]
/// A single session executing the program to fetch followers + following.
pub struct Session {
    pub id: i32,
    pub start: NaiveDateTime,
    pub finish: NaiveDateTime,
    pub follower_count: i32,
    pub following_count: i32,
}

#[derive(Debug)]
/// A user account. There can be several snapshots for each user.
pub struct User {
    pub user_id: i32,
    pub twitter_user_id: i32,
    pub creation_date: NaiveDateTime,
}

#[derive(Debug)]
/// A snapshot of a user's metadata taken during a session.
pub struct UserSnapshot {
    pub id: i32,
    pub session_id: i32,
    pub snapshot_time: NaiveDateTime,
    pub screen_name: String,
    pub location: String,
    pub description: String,
    pub url: Option<String>,
    pub follower_count: i32,
    pub following_count: i32,
    pub status_count: i32,
    pub verified: bool,
}

const PAGE_SIZE: usize = 200;
const ME: &str = "djanatyn";

#[derive(Deserialize, Debug)]
struct Config {
    fetch_followers_token: String,
}

#[derive(Error, Debug, Diagnostic)]
enum AppError {
    #[error("failed to load environment variable: {0:?}")]
    MissingVariables(envy::Error),

    #[error("hit rate limit, must wait until: {0:?}")]
    RateLimit(i32),

    #[error("unknown error")]
    UnknownError,
}

#[derive(Debug)]
struct Output {
    followers: Vec<TwitterUser>,
    following: Vec<TwitterUser>,
}

/// Try to load Twitter API Bearer token from environment variables.
fn load_config() -> miette::Result<Config> {
    match envy::from_env::<Config>() {
        Ok(config) => Ok(config),
        Err(error) => Err(AppError::MissingVariables(error))?,
    }
}

/// Flip through paginated results of users.
/// Used with `user::followers_of` and `user::friends_of`.
async fn flip_pages(mut pages: CursorIter<UserCursor>) -> miette::Result<Vec<TwitterUser>> {
    // initialize user list
    let mut users: Vec<TwitterUser> = Vec::new();

    // check for rate limit on first call
    let mut cursor = pages.call().await;
    if let Err(Error::RateLimit(timestamp)) = cursor {
        Err(AppError::RateLimit(timestamp))?
    }

    // loop over successful, non-empty responses
    while let Ok(ref mut response) = cursor {
        // stop if there are no users in the response
        if users.is_empty() {
            break;
        }

        // add users from page to results
        users.append(&mut response.users);

        // get next page
        pages.next_cursor = response.next_cursor;
        cursor = pages.call().await;

        // check for errors before continuing
        match cursor {
            Err(Error::RateLimit(timestamp)) => Err(AppError::RateLimit(timestamp))?,
            Err(_) => Err(AppError::UnknownError)?,
            Ok(_) => continue,
        };
    }

    // return accumulated users
    Ok(users)
}

/// Fetch my followers.
async fn fetch_followers(token: &Token) -> miette::Result<Vec<TwitterUser>> {
    let span = warn_span!("fetch_followers");
    span.in_scope(async || {
        let followers = user::followers_of(ME, token).with_page_size(PAGE_SIZE as i32);
        flip_pages(followers).await
    })
    .await
}

/// Fetch users I am following.
async fn fetch_following(token: &Token) -> miette::Result<Vec<TwitterUser>> {
    let span = warn_span!("fetch_following");
    span.in_scope(async || {
        let following = user::friends_of(ME, token).with_page_size(PAGE_SIZE as i32);
        flip_pages(following).await
    })
    .await
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    // load config, setup tracing
    let config = load_config()?;
    tracing_subscriber::fmt::init();
    let span = info_span!("session");
    span.in_scope(async || {
        // construct bearer token for twitter API
        let token = Token::Bearer(config.fetch_followers_token);

        // retrieve followers + following
        let (following, followers) =
            future::try_join(fetch_following(&token), fetch_followers(&token)).await?;

        // output as JSON
        let output = Output {
            following,
            followers,
        };

        println!("done");

        Ok(())
    })
    .await
}
