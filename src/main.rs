#![feature(async_closure)]

use chrono::{DateTime, Utc};

use egg_mode::cursor::{CursorIter, UserCursor};
use egg_mode::error::Error;
use egg_mode::user::{self, TwitterUser};
use egg_mode::{self, Token};
use futures::future;
use miette::{self, Diagnostic};
use rusqlite::{named_params, Connection};
use serde::Deserialize;
use std::path::Path;
use thiserror::Error;
use tracing::{event, info_span, warn_span, Level};
#[derive(Debug)]
/// A single session executing the program to fetch followers + following.
pub struct Session {
    pub id: i32,
    pub start: DateTime<Utc>,
    pub finish: DateTime<Utc>,
    pub follower_count: i32,
    pub following_count: i32,
}

#[derive(Debug)]
/// A snapshot of a user's metadata taken during a session.
pub struct UserSnapshot {
    pub user_id: i32,
    /// FOREIGN KEY (session_id) REFERENCES sessions (id)
    pub session_id: i32,
    pub snapshot_time: DateTime<Utc>,
    pub created_date: DateTime<Utc>,
    pub screen_name: String,
    pub location: String,
    pub description: Option<String>,
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

    #[error("failed to open database: {0:#?}")]
    FailedOpenDatabase(rusqlite::Error),

    #[error("failed to run init.sql: {0:#?}")]
    FailedInitialization(rusqlite::Error),

    #[error("unknown error")]
    UnknownError,
}

#[derive(Debug)]
struct Output {
    followers: Vec<TwitterUser>,
    following: Vec<TwitterUser>,
}

/// Run init.sql, a non-destructive script to create tables.
fn init_db<P: AsRef<Path>>(path: P) -> miette::Result<Connection> {
    warn_span!("init_db").in_scope(|| {
        let db: Connection = match Connection::open(path) {
            Err(e) => Err(AppError::FailedOpenDatabase(e))?,
            Ok(db) => {
                event!(Level::WARN, "opened db");
                db
            }
        };

        match db.execute(include_str!("init.sql"), []) {
            Err(e) => Err(AppError::FailedInitialization(e))?,
            Ok(updated) => {
                event!(Level::WARN, updated, "ran init script");
                Ok(db)
            }
        }
    })
}

/// Given a connection, write a UserSnapshot to the database.
///
/// snap.session_id should respect the foreign key constraint:
/// FOREIGN KEY (session_id) REFERENCES sessions (id)
fn write_snapshot(db: &Connection, snap: &UserSnapshot) -> miette::Result<usize> {
    let result = db.execute(
        "INSERT INTO snapshots (
            user_id,
            session_id,
            snapshot_time,
            created_date,
            screen_name,
            location,
            description,
            url,
            follower_count,
            following_count
        ) VALUES (
            :user_id,
            :session_id,
            :snapshot_time,
            :created_date,
            :screen_name,
            :location,
            :description,
            :url,
            :follower_count,
            :following_count
        )",
        named_params! {
            ":user_id": snap.user_id,
            ":session_id": snap.session_id,
            ":snapshot_time": snap.snapshot_time.timestamp(),
            ":created_date": snap.created_date.timestamp(),
            ":screen_name": snap.screen_name,
            ":location": snap.location,
            ":description": snap.description,
            ":url": snap.url,
            ":follower_count": snap.follower_count,
            ":following_count": snap.following_count
        },
    );

    match result {
        Ok(updated) => {
            event!(Level::WARN, updated, ?snap.screen_name, ?snap.user_id);
            Ok(updated)
        }
        Err(e) => panic!("{e}"),
    }
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

        let db = init_db("followers.sqlite")?;

        // retrieve followers + following
        // let (following, followers) =
        //     future::try_join(fetch_following(&token), fetch_followers(&token)).await?;

        // // output as JSON
        // let output = Output {
        //     following,
        //     followers,
        // };

        println!("done");

        Ok(())
    })
    .await
}
