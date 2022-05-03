CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY NOT NULL,
    start_time INTEGER,
    finish_time INTEGER,
    session_state TEXT
        CHECK (session_state IN ('STARTED', 'FINISHED', 'FAILED'))
        NOT NULL DEFAULT 'STARTED'
);

CREATE TABLE IF NOT EXISTS snapshots (
    id INTEGER PRIMARY KEY NOT NULL,
    user_id INTEGER NOT NULL,
    session_id INTEGER UNIQUE NOT NULL,
    snapshot_time INTEGER NOT NULL,
    created_date INTEGER NOT NULL,
    screen_name TEXT NOT NULL,
    location TEXT NOT NULL,
    description TEXT,
    url TEXT,
    follower_count INTEGER NOT NULL,
    following_count INTEGER NOT NULL,
    status_count INTEGER NOT NULL,
    verified INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions (id)
);


CREATE TABLE IF NOT EXISTS following (
    id INTEGER PRIMARY KEY NOT NULL,
    user_id INTEGER NOT NULL,
    session_id INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions (id)
);

CREATE TABLE IF NOT EXISTS followers (
    id INTEGER PRIMARY KEY NOT NULL,
    user_id INTEGER NOT NULL,
    session_id INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions (id)
);
