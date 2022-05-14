fetch-followers
===============

# motivation

twitter accounts are subject to moderation, and may be deleted.

periodically saving a list of followers + folllowing for your account can help you recover if your account is removed.

# implementation

- your followers / users you are following persisted to sqlite using [rusqlite](https://docs.rs/rusqlite/latest/rusqlite/index.html)
- user metadata stored as "snapshots", so you can keep track of users over time
``` sql
CREATE TABLE IF NOT EXISTS snapshots (
    id INTEGER PRIMARY KEY NOT NULL,
    user_id INTEGER NOT NULL,
    session_id INTEGER NOT NULL,
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
    UNIQUE(user_id, session_id) ON CONFLICT REPLACE,
    FOREIGN KEY (session_id) REFERENCES sessions (id)
);
```
- followers and following are fetched asynchronously while writing to db, using `tokio::sync::mpsc`
``` rust
#[derive(Debug)]
/// Commands to send to DB worker.
enum DatabaseCommand {
    /// Store a user snapshot.
    StoreSnapshot(UserSnapshot),
    /// Store a user ID as a follower.
    StoreFollower(u64),
    /// Store a user ID as someone we are following.
    StoreFollowing(u64),
    /// Mark a session as failed.
    FailedSession,
}
```
```rust
// create channel for DatabaseCommand
let (tx1, mut rx) = mpsc::channel::<DatabaseCommand>(32);
let tx2 = tx1.clone();

// retrieve followers + following
let (following, followers, _) = future::try_join3(
    fetch_following(tx1, &token),
    fetch_followers(tx2, &token),
    db_manager(session as i32, &db, &mut rx),
)
.await?;
```

# usage

```sh
; git clone https://github.com/djanatyn/fetch-followers.git
; cd fetch-followers
; FETCH_FOLLOWERS_TOKEN="..." cargo run
```

```
2022-05-03T01:46:42.342706Z  WARN init_db: fetch_followers: opened db
2022-05-03T01:46:42.342904Z  WARN init_db: fetch_followers: ran init script updated=0
2022-05-03T01:46:42.349168Z  WARN fetch_followers: created session in db updated=1
2022-05-03T01:46:43.602428Z  WARN fetch_followers: fetched page length=200
2022-05-03T01:46:43.623201Z  WARN fetch_followers: updated=1 snap.screen_name="PublicSourcePA" snap.user_id=310983849
2022-05-03T01:46:43.628380Z  WARN fetch_followers: wrote following user_id=310983849
...
2022-05-03T01:47:52.857455Z  WARN fetch_followers: updated=0 snap.screen_name="la_mifra" snap.user_id=18660056
2022-05-03T01:47:52.862490Z  WARN fetch_followers: wrote following user_id=18660056
2022-05-03T01:47:52.980854Z  WARN fetch_followers: finished session, finalizing follower_count=302 following_count=1241
2022-05-03T01:47:52.993062Z  WARN fetch_followers: finalized session
2022-05-03T01:47:52.993114Z  WARN fetch_followers: complete :)
```

``` sh
sqlite> select * from sessions;
id  start_time  finish_time  session_state
--  ----------  -----------  -------------
1   1651542402  1651542472   FINISHED     

sqlite> select count(*) from following where session_id = 1;
count(*)
--------
1241    

sqlite> select count(*) from followers where session_id = 1;
count(*)
--------
302     
```

finding verified followers:

```sh
; sqlite3 followers.sqlite <<EOF
SELECT COUNT(*) FROM snapshots
INNER JOIN followers ON
  followers.session_id = snapshots.session_id AND
  followers.user_id = snapshots.user_id
WHERE verified = TRUE
EOF
1
```

finding users whose screen names have changed between snapshots:

```sql
SELECT DISTINCT a.screen_name, b.screen_name
FROM snapshots a 
INNER JOIN snapshots b 
  ON a.session_id != b.session_id
    AND a.session_id > b.session_id 
    AND a.user_id = b.user_id 
    AND a.screen_name != b.screen_name;
```

finding mutuals:

``` sql
SELECT COUNT(*) FROM followers
INNER JOIN following ON
  followers.session_id = following.session_id AND
    followers.user_id = following.user_id
```

# tools used

- [egg_mode](https://lib.rs/crates/egg-mode)
- [tokio](https://lib.rs/crates/tokio)
- [tracing](https://lib.rs/crates/tracing)
- [thiserror](https://lib.rs/crates/thiserror)
- [miette](https://lib.rs/crates/miette)
- [chrono](https://lib.rs/crates/chrono)
- [rusqlite](https://lib.rs/crates/rusqlite)
