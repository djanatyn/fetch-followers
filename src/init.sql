-- Add migration script here
create table if not exists "sessions" (
    id integer primary key not null,
    start_time integer,
    finish_time integer,
    follower_count integer,
    following_count integer
);

create table if not exists "user" (
    id integer primary key not null,
    user_id integer not null,
    creation_date integer not null
);

create table if not exists "snapshots" (
    id integer primary key not null,
    session_id integer not null,
    snapshot_time integer not null,
    screen_name NVARCHAR(80) not null
    -- pub location: String,
    -- pub description: String,
    -- pub url: Option<String>,
    -- pub follower_count: i32,
    -- pub following_count: i32,
    -- pub status_count: i32,
    -- pub verified: bool,

);
