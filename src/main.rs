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
use tokio::sync::mpsc::{self, Receiver, Sender};
use tracing::{event, info_span, warn_span, Level};

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

    #[error("could not initialize session: {0:#?}")]
    FailiedInitSession(rusqlite::Error),

    #[error("unexpected error inserting into DB: {0:#?}")]
    FailedInsert(rusqlite::Error),

    #[error("unknown error")]
    UnknownError,
}

#[derive(Debug)]
struct Output {
    followers: Vec<TwitterUser>,
    following: Vec<TwitterUser>,
}

/// TODO: create tokio thread to update database
/// TODO: pass messages to db thread to update

#[derive(Debug)]
enum SessionState {
    Started,
    Finished,
    Failed,
}

#[derive(Debug)]
/// A snapshot of a user's metadata taken during a session.
struct UserSnapshot {
    /// User ID (from Twitter, not the database)
    user_id: i64,
    /// Time of snapshot.
    snapshot_time: DateTime<Utc>,
    /// Time account was created (returned from Twitter API).
    created_date: DateTime<Utc>,
    /// Screen name for account.
    screen_name: String,
    /// Location of account.
    location: Option<String>,
    /// Description of account.
    description: Option<String>,
    /// URL listed in account profile.
    url: Option<String>,
    /// Number of followers at time of snapshot.
    follower_count: i64,
    /// Number of users this account is following at time of snapshot.
    following_count: i64,
    /// Number of statuses posted by this account.
    status_count: i64,
    /// Whether this account is verified.
    verified: bool,
}

#[derive(Debug)]
struct FollowerCount {
    followers: i32,
    following: i32,
}

/// Commands to send to DB worker.
enum DatabaseCommand {
    /// Store a user snapshot.
    StoreSnapshot(Box<TwitterUser>),
    /// Store a user ID as a follower.
    StoreFollower(i64),
    /// Store a user ID as someone we are following.
    StoreFollowing(i64),
    /// Store a user ID as someone we're following.
    SuccessfulSession(FollowerCount),
    FailedSession(FollowerCount),
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

/// Initialize a session, recording the current start time.
///
/// Returns ID of Session within the database.
fn init_session(db: &Connection) -> miette::Result<i64> {
    let now = Utc::now();
    let rows = db.execute(
        "INSERT INTO sessions (start_time) VALUES (:start)",
        named_params! {
            ":start": now.timestamp()
        },
    );

    let updated = match rows {
        Err(e) => Err(AppError::FailiedInitSession(e))?,
        Ok(updated) => {
            event!(Level::WARN, updated, "created session in db");
            updated
        }
    };

    Ok(db.last_insert_rowid())
}

/// Given a connection, write a UserSnapshot to the database.
///
/// snap.session_id should respect the foreign key constraint:
/// FOREIGN KEY (session_id) REFERENCES sessions (id)
fn write_snapshot(session_id: i32, db: &Connection, snap: &UserSnapshot) -> miette::Result<usize> {
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
            following_count,
            status_count,
            verified
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
            :following_count,
            :status_count,
            :verified
        )",
        named_params! {
            ":user_id": snap.user_id,
            ":session_id": session_id,
            ":snapshot_time": snap.snapshot_time.timestamp(),
            ":created_date": snap.created_date.timestamp(),
            ":screen_name": snap.screen_name,
            ":location": snap.location,
            ":description": snap.description,
            ":url": snap.url,
            ":follower_count": snap.follower_count,
            ":following_count": snap.following_count,
            ":status_count": snap.status_count,
            ":verified": snap.verified
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
async fn flip_pages(
    tx: Sender<DatabaseCommand>,
    mut pages: CursorIter<UserCursor>,
) -> miette::Result<Vec<TwitterUser>> {
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
        if response.users.is_empty() {
            break;
        }

        let length = response.users.len();
        event!(Level::WARN, length, "fetched page");

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
async fn fetch_followers(
    tx: Sender<DatabaseCommand>,
    token: &Token,
) -> miette::Result<Vec<TwitterUser>> {
    let span = warn_span!("fetch_followers");
    span.in_scope(async || {
        let followers = user::followers_of(ME, token).with_page_size(PAGE_SIZE as i32);
        flip_pages(tx, followers).await
    })
    .await
}

/// Fetch users I am following.
async fn fetch_following(
    tx: Sender<DatabaseCommand>,
    token: &Token,
) -> miette::Result<Vec<TwitterUser>> {
    let span = warn_span!("fetch_following");
    span.in_scope(async || {
        let following = user::friends_of(ME, token).with_page_size(PAGE_SIZE as i32);
        flip_pages(tx, following).await
    })
    .await
}

fn store_follower(session_id: i32, db: &Connection, user_id: i64) -> miette::Result<usize> {
    let rows = db.execute(
        "INSERT INTO followers (user_id, session_id) VALUES (:user_id, :session_id)",
        named_params! {
            ":user_id": user_id,
            ":session_id": session_id,
        },
    );

    let updated = match rows {
        Err(e) => Err(AppError::FailedInsert(e))?,
        Ok(updated) => {
            event!(Level::WARN, user_id, "wrote follower");
            updated
        }
    };

    Ok(updated)
}

fn store_following(session_id: i32, db: &Connection, user_id: i64) -> miette::Result<usize> {
    let rows = db.execute(
        "INSERT INTO following (user_id, session_id) VALUES (:user_id, :session_id)",
        named_params! {
            ":user_id": user_id,
            ":session_id": session_id,
        },
    );

    let updated = match rows {
        Err(e) => Err(AppError::FailedInsert(e))?,
        Ok(updated) => {
            event!(Level::WARN, user_id, "wrote following");
            updated
        }
    };

    Ok(updated)
}

fn finalize_session(
    session_id: i32,
    db: &Connection,
    count: &FollowerCount,
) -> miette::Result<usize> {
    todo!();
}

fn fail_session(sesssion_id: i32, db: &Connection, count: &FollowerCount) -> miette::Result<usize> {
    todo!();
}

fn user_snapshot(user: &TwitterUser) -> UserSnapshot {
    todo!("convert user to snapshot")
}

/// Interpreter task for DatabaseCommand channel. Drops Connection when complete.
async fn db_manager(
    session_id: i32,
    db: Connection,
    rx: &mut Receiver<DatabaseCommand>,
) -> miette::Result<()> {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            DatabaseCommand::StoreSnapshot(user) => {
                let snapshot = user_snapshot(&user);
                write_snapshot(session_id, &db, &snapshot)?;
            }
            DatabaseCommand::StoreFollower(user_id) => {
                store_follower(session_id, &db, user_id)?;
            }
            DatabaseCommand::StoreFollowing(user_id) => {
                store_following(session_id, &db, user_id)?;
            }
            DatabaseCommand::SuccessfulSession(count) => {
                finalize_session(session_id, &db, &count)?;
            }
            DatabaseCommand::FailedSession(count) => {
                fail_session(session_id, &db, &count)?;
            }
        }
    }

    Ok(())
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
        let session = init_session(&db)?;

        // create channel for DatabaseCommand
        let (tx1, mut rx) = mpsc::channel::<DatabaseCommand>(32);
        let tx2 = tx1.clone();

        // retrieve followers + following
        let (following, followers, _) = future::try_join3(
            fetch_following(tx1, &token),
            fetch_followers(tx2, &token),
            db_manager(session as i32, db, &mut rx),
        )
        .await?;

        // // output as JSON
        let output = Output {
            following,
            followers,
        };

        Ok(())
    })
    .await
}
