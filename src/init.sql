create table if not exists "sessions" (
    id INTEGER PRIMARY KEY NOT NULL,
    start_time INTEGER,
    finish_time INTEGER,
    follower_count INTEGER,
    following_count INTEGER
);

CREATE TABLE IF NOT EXISTS "user" (
    id INTEGER PRIMARY KEY NOT NULL,
    user_id INTEGER NOT NULL,
    creation_date INTEGER NOT NULL
);

create table if not exists "snapshots" (
    id INTEGER PRIMARY KEY NOT NULL,
    session_id INTEGER NOT NULL,
    snapshot_time INTEGER NOT NULL,
    screen_name TEXT NOT NULL,
    location TEXT not null,
    description TEXT,
    url TEXT,
    follower_count INTEGER NOT NULL,
    following_count INTEGER NOT NULL,
    status_count INTEGER NOT NULL,
    verified INTEGER NOT NULL
);
