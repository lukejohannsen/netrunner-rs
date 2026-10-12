//! The daemon's store: what a server keeps about the keys it has met,
//! behind one API over SQLite and Postgres (Phase 4 §10).
//!
//! **The results log is the truth; this is its index.** `results.jsonl`
//! and the signed receipts beside each record are what anyone can
//! recompute a rating from (`docs/identity-and-rating.md` §4). The store
//! holds what a question needs answered quickly — a player's name and
//! what they wear, the rating per format and role, the results, the
//! titles and badges earned, which chair two keys last took against
//! each other, the tournaments — and every table but the players' and
//! the awards' can be rebuilt from the log. It replaced three JSON files
//! each rewritten whole on every change (`players.json`, `ratings.json`,
//! `tournaments.json`), which a ladder page could not be built on.
//!
//! **Engine-free and rating-free.** Keys, formats and roles cross this
//! boundary as the strings the rest of the workspace already prints
//! (`key:<base32>`, a format's lowercase name, `corp` / `runner`), so
//! the crate depends on sqlx and nothing of the game; the server
//! converts at the edge.
//!
//! **One API, two databases, by `sqlx::Any`.** Rejected: two backends
//! behind a trait, which is every query written twice; and a single
//! document read whole into memory, which is what the JSON files were.
//! The price of `Any` is a dialect both databases accept, which these
//! queries keep to: `CREATE TABLE IF NOT EXISTS`, `ON CONFLICT … DO
//! UPDATE`, the types `TEXT`, `BIGINT` and `DOUBLE PRECISION`, and
//! **`$1`-style placeholders, each used exactly once and in order** —
//! SQLite takes `$1` as a *named* parameter and gives a name one index
//! however often it appears, while sqlx binds by position, so a
//! placeholder written twice would bind the wrong value on SQLite and
//! the right one on Postgres. Times are Unix seconds in a `BIGINT`,
//! because `Any` binds no unsigned integer.
//!
//! **Which drivers are compiled in is a feature** (`sqlite`, `postgres`),
//! so a consumer that opens no database — the desktop client, which
//! hosts a game in-process through the server crate — builds neither.
//! `Store::open` on a URL whose driver is not compiled in is an error,
//! never a silent file.
//!
//! **No migrations.** Nothing is released, so the schema is created on
//! open and `SCHEMA_VERSION` is written for the day one is needed; a
//! store written by an older schema is reported, not upgraded.

use std::fmt;

use sqlx::any::AnyPoolOptions;
use sqlx::{AnyPool, Row};

/// Written to the `schema` table on open; a store at another version is
/// refused by `Store::open`.
pub const SCHEMA_VERSION: i64 = 1;

/// The file a daemon keeps under its data directory when it is given no
/// `--database`.
pub const SQLITE_FILE: &str = "store.sqlite";

/// A store that could not be opened, read or written.
#[derive(Debug)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<sqlx::Error> for Error {
    fn from(error: sqlx::Error) -> Self {
        Error(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// A key the daemon has met, as the players table holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Player {
    pub key: String,
    /// The name the key last attached with. A label: two keys may share
    /// one, and a key may change it without changing who it is.
    pub name: String,
    pub first_seen: i64,
    pub last_seen: i64,
    /// The title and badge the player wears, chosen among those earned
    /// (`awards`); `None` wears nothing.
    pub title: Option<String>,
    pub badge: Option<String>,
}

/// One key's rating in one format and one chair.
#[derive(Debug, Clone, PartialEq)]
pub struct RatingRow {
    pub key: String,
    pub format: String,
    /// `corp` or `runner`.
    pub role: String,
    pub rating: f64,
    pub deviation: f64,
    pub volatility: f64,
    pub wins: i64,
    pub draws: i64,
    pub losses: i64,
}

/// One rated game, as the results log holds it: the index row and the
/// signed receipt's own JSON line, so a player's receipt can be handed
/// back without reading the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultRow {
    pub match_id: String,
    pub ended_at: i64,
    pub format: String,
    pub corp: String,
    pub runner: String,
    /// `corp`, `runner`, or `None` for a game nobody won.
    pub winner: Option<String>,
    pub receipt: String,
}

/// A title or a badge a key has earned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Award {
    pub key: String,
    /// `title` or `badge`.
    pub kind: String,
    pub name: String,
    pub earned_at: i64,
    /// What granted it: an event's id, or an operator's key.
    pub source: String,
}

/// Which chair two keys last took against each other, so the next
/// pairing can swap them (Phase 4 §9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pairing {
    pub corp: String,
    pub runner: String,
    pub played_at: i64,
}

/// A tournament as the server keeps it: a document nothing queries
/// inside, so one row holds its JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TournamentRow {
    pub id: String,
    pub document: String,
    pub updated_at: i64,
}

/// The open store. Cheap to clone: a handle on one pool.
#[derive(Clone)]
pub struct Store {
    pool: AnyPool,
}

impl fmt::Debug for Store {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Store")
    }
}

/// The URL that opens (and makes) the SQLite file at `path`.
///
/// Every byte of the path but an unreserved one is percent-encoded.
/// `sqlx::Any` parses the URL as a URL before the SQLite driver reads
/// the file name back out of it and percent-decodes it, so a path
/// written in raw was mangled wherever it held something a URL treats
/// as syntax: a Windows path (`C:\Users\…`, the drive's colon and every
/// backslash) never opened, and a `?` or `#` anywhere cut it short.
pub fn sqlite_url(path: &std::path::Path) -> String {
    let mut url = String::from("sqlite://");
    for &byte in path.as_os_str().as_encoded_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            url.push(char::from(byte));
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
    url.push_str("?mode=rwc");
    url
}

impl Store {
    /// Opens the database at `url` — `sqlite://<path>?mode=rwc` or
    /// `postgres://…` — and creates the schema if it is not there.
    pub async fn open(url: &str) -> Result<Store> {
        sqlx::any::install_default_drivers();
        let pool = AnyPoolOptions::new().max_connections(4).connect(url).await?;
        let store = Store { pool };
        if url.starts_with("sqlite:") {
            // One writer at a time and readers beside it; set once, kept
            // by the file.
            sqlx::query("PRAGMA journal_mode = WAL").execute(&store.pool).await?;
        }
        store.create_schema().await?;
        Ok(store)
    }

    /// Closes the pool, waiting for its connections; a store dropped
    /// without this is closed as its connections are.
    pub async fn close(&self) {
        self.pool.close().await;
    }

    async fn create_schema(&self) -> Result<()> {
        for &statement in SCHEMA {
            sqlx::query(statement).execute(&self.pool).await?;
        }
        let versions: Vec<i64> = sqlx::query("SELECT version FROM schema").fetch_all(&self.pool).await?.iter().map(|row| row.get(0)).collect();
        match versions.first() {
            None => {
                sqlx::query("INSERT INTO schema (version) VALUES ($1)").bind(SCHEMA_VERSION).execute(&self.pool).await?;
            }
            Some(&version) if version == SCHEMA_VERSION => {}
            Some(version) => return Err(Error(format!("the store is at schema version {version}, and this build is at {SCHEMA_VERSION}"))),
        }
        Ok(())
    }

    // ----- players --------------------------------------------------

    /// Notes that `key` attached as `name` at `now`: a first visit makes
    /// the row, a later one updates the name and the time and keeps what
    /// the player wears.
    pub async fn saw_player(&self, key: &str, name: &str, now: i64) -> Result<()> {
        sqlx::query(
            "INSERT INTO players (player, name, first_seen, last_seen, title, badge) VALUES ($1, $2, $3, $4, NULL, NULL) \
             ON CONFLICT (player) DO UPDATE SET name = excluded.name, last_seen = excluded.last_seen",
        )
        .bind(key)
        .bind(name)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn player(&self, key: &str) -> Result<Option<Player>> {
        let row = sqlx::query("SELECT player, name, first_seen, last_seen, title, badge FROM players WHERE player = $1").bind(key).fetch_optional(&self.pool).await?;
        Ok(row.map(|row| Player {
            key: row.get(0),
            name: row.get(1),
            first_seen: row.get(2),
            last_seen: row.get(3),
            title: row.get(4),
            badge: row.get(5),
        }))
    }

    /// Sets what `key` wears. The caller has checked each against the
    /// awards; this writes what it is given.
    pub async fn set_worn(&self, key: &str, title: Option<&str>, badge: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE players SET title = $1, badge = $2 WHERE player = $3").bind(title).bind(badge).bind(key).execute(&self.pool).await?;
        Ok(())
    }

    // ----- ratings --------------------------------------------------

    /// Writes one rating row, replacing the one for its key, format and
    /// role.
    pub async fn put_rating(&self, row: &RatingRow) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        put_rating_in(&mut conn, row).await
    }

    /// One rated game's rows in one transaction — both keys' standings
    /// and the result — so a reader never sees half a game: a ladder read
    /// between two of the rows would show one player's standing moved and
    /// the other's not (a test did, on CI). `put_rating` alone is for a
    /// row with no game behind it, as a rebuild writes them.
    pub async fn record_game(&self, ratings: &[RatingRow], result: &ResultRow) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for row in ratings {
            put_rating_in(&mut tx, row).await?;
        }
        put_result_in(&mut tx, result).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Every rating row, in key, format, role order.
    pub async fn ratings(&self) -> Result<Vec<RatingRow>> {
        let rows = sqlx::query("SELECT player, format, role, rating, deviation, volatility, wins, draws, losses FROM ratings ORDER BY player, format, role")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(rating_row).collect())
    }

    /// Every rating row in `format`, in key then role order.
    pub async fn ratings_in(&self, format: &str) -> Result<Vec<RatingRow>> {
        let rows = sqlx::query("SELECT player, format, role, rating, deviation, volatility, wins, draws, losses FROM ratings WHERE format = $1 ORDER BY player, role")
            .bind(format)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(rating_row).collect())
    }

    /// `key`'s rating rows, every format and role.
    pub async fn ratings_of(&self, key: &str) -> Result<Vec<RatingRow>> {
        let rows = sqlx::query("SELECT player, format, role, rating, deviation, volatility, wins, draws, losses FROM ratings WHERE player = $1 ORDER BY format, role")
            .bind(key)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(rating_row).collect())
    }

    // ----- results --------------------------------------------------

    pub async fn put_result(&self, row: &ResultRow) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        put_result_in(&mut conn, row).await
    }

    /// The games `key` sat in, newest first.
    pub async fn results_of(&self, key: &str) -> Result<Vec<ResultRow>> {
        let rows = sqlx::query("SELECT match_id, ended_at, format, corp, runner, winner, receipt FROM results WHERE corp = $1 OR runner = $2 ORDER BY ended_at DESC, match_id")
            .bind(key)
            .bind(key)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .iter()
            .map(|row| ResultRow {
                match_id: row.get(0),
                ended_at: row.get(1),
                format: row.get(2),
                corp: row.get(3),
                runner: row.get(4),
                winner: row.get(5),
                receipt: row.get(6),
            })
            .collect())
    }

    pub async fn result_count(&self) -> Result<i64> {
        Ok(sqlx::query("SELECT COUNT(*) FROM results").fetch_one(&self.pool).await?.get(0))
    }

    /// Replaces the two tables the results log rebuilds — the ratings and
    /// the results — with the log's fold, in one transaction, so a daemon
    /// reading beside the rebuild sees the old index or the new and never
    /// an emptied one. The players, the awards, the pairings and the
    /// tournaments are not the log's and are kept.
    pub async fn replace_rebuilt(&self, ratings: &[RatingRow], results: &[ResultRow]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM ratings").execute(&mut *tx).await?;
        sqlx::query("DELETE FROM results").execute(&mut *tx).await?;
        for row in ratings {
            put_rating_in(&mut tx, row).await?;
        }
        for row in results {
            put_result_in(&mut tx, row).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    // ----- awards ---------------------------------------------------

    /// Grants `name` of `kind` to `key`; one already held is left as it
    /// was granted.
    pub async fn grant(&self, award: &Award) -> Result<()> {
        sqlx::query("INSERT INTO awards (player, kind, name, earned_at, source) VALUES ($1, $2, $3, $4, $5) ON CONFLICT (player, kind, name) DO NOTHING")
            .bind(&award.key)
            .bind(&award.kind)
            .bind(&award.name)
            .bind(award.earned_at)
            .bind(&award.source)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn revoke(&self, key: &str, kind: &str, name: &str) -> Result<()> {
        sqlx::query("DELETE FROM awards WHERE player = $1 AND kind = $2 AND name = $3").bind(key).bind(kind).bind(name).execute(&self.pool).await?;
        Ok(())
    }

    /// `key`'s awards, oldest first.
    pub async fn awards_of(&self, key: &str) -> Result<Vec<Award>> {
        let rows = sqlx::query("SELECT player, kind, name, earned_at, source FROM awards WHERE player = $1 ORDER BY earned_at, kind, name").bind(key).fetch_all(&self.pool).await?;
        Ok(rows.iter().map(|row| Award { key: row.get(0), kind: row.get(1), name: row.get(2), earned_at: row.get(3), source: row.get(4) }).collect())
    }

    // ----- pairings -------------------------------------------------

    /// Remembers that `corp` and `runner` played each other in those
    /// chairs at `played_at`, replacing what the pair last did.
    pub async fn put_pairing(&self, corp: &str, runner: &str, played_at: i64) -> Result<()> {
        let (one, other) = ordered(corp, runner);
        sqlx::query(
            "INSERT INTO last_pairing (one, other, corp, played_at) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (one, other) DO UPDATE SET corp = excluded.corp, played_at = excluded.played_at",
        )
        .bind(one)
        .bind(other)
        .bind(corp)
        .bind(played_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// What every pair of keys last did against each other: what a daemon
    /// reads at bind, since a pairing is made under a lock no query can
    /// wait inside.
    pub async fn pairings(&self) -> Result<Vec<Pairing>> {
        let rows = sqlx::query("SELECT one, other, corp, played_at FROM last_pairing ORDER BY one, other").fetch_all(&self.pool).await?;
        Ok(rows
            .iter()
            .map(|row| {
                let (one, other, corp): (String, String, String) = (row.get(0), row.get(1), row.get(2));
                let runner = if corp == one { other } else { one };
                Pairing { corp, runner, played_at: row.get(3) }
            })
            .collect())
    }

    /// What two keys last did against each other, whichever order they
    /// are named in.
    pub async fn last_pairing(&self, a: &str, b: &str) -> Result<Option<Pairing>> {
        let (one, other) = ordered(a, b);
        let row = sqlx::query("SELECT one, other, corp, played_at FROM last_pairing WHERE one = $1 AND other = $2").bind(one).bind(other).fetch_optional(&self.pool).await?;
        Ok(row.map(|row| {
            let (one, other, corp): (String, String, String) = (row.get(0), row.get(1), row.get(2));
            let runner = if corp == one { other } else { one };
            Pairing { corp, runner, played_at: row.get(3) }
        }))
    }

    // ----- tournaments ----------------------------------------------

    pub async fn put_tournament(&self, row: &TournamentRow) -> Result<()> {
        sqlx::query("INSERT INTO tournaments (id, document, updated_at) VALUES ($1, $2, $3) ON CONFLICT (id) DO UPDATE SET document = excluded.document, updated_at = excluded.updated_at")
            .bind(&row.id)
            .bind(&row.document)
            .bind(row.updated_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn remove_tournament(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM tournaments WHERE id = $1").bind(id).execute(&self.pool).await?;
        Ok(())
    }

    /// Every tournament, by id.
    pub async fn tournaments(&self) -> Result<Vec<TournamentRow>> {
        let rows = sqlx::query("SELECT id, document, updated_at FROM tournaments ORDER BY id").fetch_all(&self.pool).await?;
        Ok(rows.iter().map(|row| TournamentRow { id: row.get(0), document: row.get(1), updated_at: row.get(2) }).collect())
    }
}

async fn put_rating_in(conn: &mut sqlx::AnyConnection, row: &RatingRow) -> Result<()> {
    sqlx::query(
        "INSERT INTO ratings (player, format, role, rating, deviation, volatility, wins, draws, losses) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         ON CONFLICT (player, format, role) DO UPDATE SET rating = excluded.rating, deviation = excluded.deviation, \
         volatility = excluded.volatility, wins = excluded.wins, draws = excluded.draws, losses = excluded.losses",
    )
    .bind(&row.key)
    .bind(&row.format)
    .bind(&row.role)
    .bind(row.rating)
    .bind(row.deviation)
    .bind(row.volatility)
    .bind(row.wins)
    .bind(row.draws)
    .bind(row.losses)
    .execute(conn)
    .await?;
    Ok(())
}

async fn put_result_in(conn: &mut sqlx::AnyConnection, row: &ResultRow) -> Result<()> {
    sqlx::query(
        "INSERT INTO results (match_id, ended_at, format, corp, runner, winner, receipt) VALUES ($1, $2, $3, $4, $5, $6, $7) \
         ON CONFLICT (match_id) DO UPDATE SET ended_at = excluded.ended_at, format = excluded.format, corp = excluded.corp, \
         runner = excluded.runner, winner = excluded.winner, receipt = excluded.receipt",
    )
    .bind(&row.match_id)
    .bind(row.ended_at)
    .bind(&row.format)
    .bind(&row.corp)
    .bind(&row.runner)
    .bind(&row.winner)
    .bind(&row.receipt)
    .execute(conn)
    .await?;
    Ok(())
}

fn rating_row(row: &sqlx::any::AnyRow) -> RatingRow {
    RatingRow {
        key: row.get(0),
        format: row.get(1),
        role: row.get(2),
        rating: row.get(3),
        deviation: row.get(4),
        volatility: row.get(5),
        wins: row.get(6),
        draws: row.get(7),
        losses: row.get(8),
    }
}

/// A pair of keys in one order, so the two name one row.
fn ordered<'a>(a: &'a str, b: &'a str) -> (&'a str, &'a str) {
    if a <= b { (a, b) } else { (b, a) }
}

/// The schema, in the dialect both databases read (module doc). `player`
/// rather than `key`, and `one` / `other` rather than `first` / `second`,
/// to stay clear of every reserved word in either.
const SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS schema (version BIGINT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS players (\
        player TEXT PRIMARY KEY, name TEXT NOT NULL, first_seen BIGINT NOT NULL, last_seen BIGINT NOT NULL, \
        title TEXT, badge TEXT)",
    "CREATE TABLE IF NOT EXISTS ratings (\
        player TEXT NOT NULL, format TEXT NOT NULL, role TEXT NOT NULL, \
        rating DOUBLE PRECISION NOT NULL, deviation DOUBLE PRECISION NOT NULL, volatility DOUBLE PRECISION NOT NULL, \
        wins BIGINT NOT NULL, draws BIGINT NOT NULL, losses BIGINT NOT NULL, \
        PRIMARY KEY (player, format, role))",
    "CREATE INDEX IF NOT EXISTS ratings_by_format ON ratings (format, rating)",
    "CREATE TABLE IF NOT EXISTS results (\
        match_id TEXT PRIMARY KEY, ended_at BIGINT NOT NULL, format TEXT NOT NULL, \
        corp TEXT NOT NULL, runner TEXT NOT NULL, winner TEXT, receipt TEXT NOT NULL)",
    "CREATE INDEX IF NOT EXISTS results_by_corp ON results (corp, ended_at)",
    "CREATE INDEX IF NOT EXISTS results_by_runner ON results (runner, ended_at)",
    "CREATE TABLE IF NOT EXISTS awards (\
        player TEXT NOT NULL, kind TEXT NOT NULL, name TEXT NOT NULL, earned_at BIGINT NOT NULL, source TEXT NOT NULL, \
        PRIMARY KEY (player, kind, name))",
    "CREATE TABLE IF NOT EXISTS last_pairing (\
        one TEXT NOT NULL, other TEXT NOT NULL, corp TEXT NOT NULL, played_at BIGINT NOT NULL, \
        PRIMARY KEY (one, other))",
    "CREATE TABLE IF NOT EXISTS tournaments (id TEXT PRIMARY KEY, document TEXT NOT NULL, updated_at BIGINT NOT NULL)",
];

/// A namespace of a test's own on a shared Postgres server (Phase 4 §10
/// stage 3), for this crate's tests and the server's.
///
/// The Postgres driver is tested against a live server only where one
/// is given: `NETRUNNER_TEST_POSTGRES` names it by URL, and the weekly
/// workflow's `postgres` job sets it to a service container. Nobody has
/// to install Postgres to run `cargo test`, and a test that finds the
/// variable unset says it skipped and runs on SQLite alone. Each test
/// gets a schema of its own, reached by setting the connection's
/// `search_path` in the URL, so tests run in parallel against one server
/// and drop what they made. Rejected: a database per test, which needs
/// the right to create databases and a connection to each.
pub mod scratch {
    use sqlx::AssertSqlSafe;
    use sqlx::any::AnyPoolOptions;

    /// The variable naming the Postgres server tests may use.
    pub const POSTGRES_ENV: &str = "NETRUNNER_TEST_POSTGRES";

    /// A schema made for one test, and the URL that opens a store in it.
    #[derive(Debug)]
    pub struct PostgresSchema {
        base: String,
        schema: String,
        /// The server's URL with the schema as its search path.
        pub url: String,
    }

    /// A fresh, empty schema for the test `name` on the server
    /// `NETRUNNER_TEST_POSTGRES` names, or `None` when it names none.
    ///
    /// # Panics
    ///
    /// When the variable is set and this build has no Postgres driver —
    /// a run that asked for Postgres must not pass on SQLite alone — or
    /// when the server cannot be reached or refuses the schema.
    pub async fn postgres(name: &str) -> Option<PostgresSchema> {
        let base = match std::env::var(POSTGRES_ENV) {
            Ok(url) if !url.is_empty() => url,
            _ => {
                eprintln!("{name}: {POSTGRES_ENV} is unset, so this runs on SQLite only");
                return None;
            }
        };
        if !cfg!(feature = "postgres") {
            panic!("{POSTGRES_ENV} is set but this build has no Postgres driver: test with the `postgres` feature");
        }
        let tidy: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect();
        let schema = format!("test_{tidy}_{}", std::process::id());
        run(&base, &[format!("DROP SCHEMA IF EXISTS {schema} CASCADE"), format!("CREATE SCHEMA {schema}")]).await;
        let separator = if base.contains('?') { '&' } else { '?' };
        let url = format!("{base}{separator}options[search_path]={schema}");
        Some(PostgresSchema { base, schema, url })
    }

    impl PostgresSchema {
        /// Drops the schema and everything in it.
        pub async fn remove(self) {
            run(&self.base, &[format!("DROP SCHEMA IF EXISTS {} CASCADE", self.schema)]).await;
        }
    }

    /// Runs `statements` on the server at `base`, outside any schema.
    /// The names in them are made above from `[a-z0-9_]`, which is the
    /// audit `AssertSqlSafe` asks for.
    async fn run(base: &str, statements: &[String]) {
        sqlx::any::install_default_drivers();
        let pool = AnyPoolOptions::new().max_connections(1).connect(base).await.unwrap_or_else(|error| panic!("{POSTGRES_ENV}: {error}"));
        for statement in statements {
            sqlx::query(AssertSqlSafe(statement.clone())).execute(&pool).await.unwrap_or_else(|error| panic!("{statement}: {error}"));
        }
        pool.close().await;
    }
}

/// The tests need a driver, so they are the `sqlite` feature's — which a
/// workspace build turns on through the daemon's default features, and
/// `cargo test -p netrunner_store --features sqlite` runs alone. Each runs
/// once per database it can reach: always SQLite, and Postgres where
/// `NETRUNNER_TEST_POSTGRES` names a server (`scratch`).
#[cfg(all(test, feature = "sqlite"))]
mod tests {
    use super::*;

    /// Where a test's store lives, so it can be opened again and removed.
    enum Place {
        File(std::path::PathBuf),
        Postgres(scratch::PostgresSchema),
    }

    impl Place {
        fn url(&self) -> String {
            match self {
                Place::File(dir) => sqlite_url(&dir.join(SQLITE_FILE)),
                Place::Postgres(schema) => schema.url.clone(),
            }
        }

        async fn remove(self) {
            match self {
                Place::File(dir) => {
                    let _ = std::fs::remove_dir_all(dir);
                }
                Place::Postgres(schema) => schema.remove().await,
            }
        }
    }

    /// A fresh store for the test `name` on every database there is: a
    /// scratch SQLite file of its own, and a scratch schema on Postgres
    /// when one is given.
    async fn every_store(name: &str) -> Vec<(Store, Place)> {
        let dir = std::env::temp_dir().join(format!("netrunner_store_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut places = vec![Place::File(dir)];
        if let Some(schema) = scratch::postgres(name).await {
            places.push(Place::Postgres(schema));
        }
        let mut stores = Vec::new();
        for place in places {
            let store = Store::open(&place.url()).await.unwrap_or_else(|error| panic!("a scratch store opens at {}: {error}", place.url()));
            stores.push((store, place));
        }
        stores
    }

    #[tokio::test]
    async fn a_player_is_seen_twice_under_one_row_and_keeps_what_they_wear() {
        for (store, place) in every_store("players").await {
            store.saw_player("key:ann", "ann", 10).await.unwrap();
            store.set_worn("key:ann", Some("Gauntlet Runner"), None).await.unwrap();
            store.saw_player("key:ann", "annie", 20).await.unwrap();
            let ann = store.player("key:ann").await.unwrap().unwrap();
            assert_eq!(ann, Player { key: "key:ann".into(), name: "annie".into(), first_seen: 10, last_seen: 20, title: Some("Gauntlet Runner".into()), badge: None });
            assert_eq!(store.player("key:bo").await.unwrap(), None);
            store.close().await;
            place.remove().await;
        }
    }

    #[tokio::test]
    async fn a_rating_row_is_replaced_by_its_key_format_and_role() {
        for (store, place) in every_store("ratings").await {
            let mut row = RatingRow { key: "key:ann".into(), format: "standard".into(), role: "corp".into(), rating: 1500.0, deviation: 350.0, volatility: 0.06, wins: 0, draws: 0, losses: 0 };
            store.put_rating(&row).await.unwrap();
            row.rating = 1612.5;
            row.wins = 1;
            store.put_rating(&row).await.unwrap();
            store.put_rating(&RatingRow { format: "startup".into(), ..row.clone() }).await.unwrap();
            store.put_rating(&RatingRow { role: "runner".into(), ..row.clone() }).await.unwrap();
            assert_eq!(store.ratings().await.unwrap().len(), 3);
            assert_eq!(store.ratings_in("standard").await.unwrap().len(), 2);
            let standard_corp = &store.ratings_of("key:ann").await.unwrap()[0];
            assert_eq!((standard_corp.format.as_str(), standard_corp.role.as_str(), standard_corp.rating, standard_corp.wins), ("standard", "corp", 1612.5, 1));
            store.replace_rebuilt(&[], &[]).await.unwrap();
            assert!(store.ratings().await.unwrap().is_empty());
            let result = ResultRow { match_id: "m1".into(), ended_at: 1, format: "standard".into(), corp: "key:ann".into(), runner: "key:bo".into(), winner: Some("corp".into()), receipt: "{}".into() };
            store.record_game(&[row.clone(), RatingRow { key: "key:bo".into(), ..row.clone() }], &result).await.unwrap();
            assert_eq!(store.ratings().await.unwrap().len(), 2);
            assert_eq!(store.results_of("key:bo").await.unwrap(), vec![result]);
            store.close().await;
            place.remove().await;
        }
    }

    #[tokio::test]
    async fn results_awards_pairings_and_tournaments_round_trip() {
        for (store, place) in every_store("rest").await {
            let result = ResultRow { match_id: "m1".into(), ended_at: 100, format: "standard".into(), corp: "key:ann".into(), runner: "key:bo".into(), winner: Some("corp".into()), receipt: "{}".into() };
            store.put_result(&result).await.unwrap();
            store.put_result(&ResultRow { match_id: "m2".into(), ended_at: 200, winner: None, ..result.clone() }).await.unwrap();
            let of_bo = store.results_of("key:bo").await.unwrap();
            assert_eq!(of_bo.iter().map(|row| row.match_id.as_str()).collect::<Vec<_>>(), ["m2", "m1"], "newest first");
            assert_eq!(of_bo[0].winner, None);
            assert_eq!(store.result_count().await.unwrap(), 2);

            let badge = Award { key: "key:ann".into(), kind: "badge".into(), name: "first-gauntlet".into(), earned_at: 150, source: "event:abc".into() };
            store.grant(&badge).await.unwrap();
            store.grant(&Award { earned_at: 999, ..badge.clone() }).await.unwrap();
            assert_eq!(store.awards_of("key:ann").await.unwrap(), vec![badge.clone()], "granted once, kept as first granted");
            store.revoke("key:ann", "badge", "first-gauntlet").await.unwrap();
            assert!(store.awards_of("key:ann").await.unwrap().is_empty());

            store.put_pairing("key:ann", "key:bo", 100).await.unwrap();
            store.put_pairing("key:bo", "key:ann", 200).await.unwrap();
            let last = store.last_pairing("key:ann", "key:bo").await.unwrap().unwrap();
            assert_eq!(last, Pairing { corp: "key:bo".into(), runner: "key:ann".into(), played_at: 200 }, "one row for the pair, the later game's chairs");
            assert_eq!(store.last_pairing("key:bo", "key:ann").await.unwrap(), Some(last.clone()));
            assert_eq!(store.last_pairing("key:ann", "key:cy").await.unwrap(), None);
            store.put_pairing("key:cy", "key:ann", 300).await.unwrap();
            assert_eq!(
                store.pairings().await.unwrap(),
                vec![last.clone(), Pairing { corp: "key:cy".into(), runner: "key:ann".into(), played_at: 300 }],
                "every pair, read whole at bind"
            );

            store.put_tournament(&TournamentRow { id: "T1".into(), document: "{\"v\":1}".into(), updated_at: 1 }).await.unwrap();
            store.put_tournament(&TournamentRow { id: "T1".into(), document: "{\"v\":2}".into(), updated_at: 2 }).await.unwrap();
            assert_eq!(store.tournaments().await.unwrap(), vec![TournamentRow { id: "T1".into(), document: "{\"v\":2}".into(), updated_at: 2 }]);
            store.remove_tournament("T1").await.unwrap();
            assert!(store.tournaments().await.unwrap().is_empty());
            store.close().await;
            place.remove().await;
        }
    }

    /// What a URL treats as syntax, in a directory's name. Linux allows
    /// every character a Windows path is made of, so there the name holds
    /// a drive's colon and backslashes too; on Windows the temp directory
    /// itself brings them (`C:\\Users\\…`), and the name keeps to what
    /// Windows allows.
    #[tokio::test]
    async fn a_path_with_what_a_url_treats_as_syntax_opens_where_it_says() {
        let name = if cfg!(windows) { "netrunner_store a#b%c d" } else { "netrunner_store C:\\Users\\a?b#c%d e" };
        let dir = std::env::temp_dir().join(format!("{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(SQLITE_FILE);
        let store = Store::open(&sqlite_url(&path)).await.unwrap_or_else(|error| panic!("{}: {error}", sqlite_url(&path)));
        store.saw_player("key:ann", "ann", 1).await.unwrap();
        store.close().await;
        assert!(path.is_file(), "the file is where the path says, not at a prefix of it");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn the_store_is_made_on_open_and_opens_again_at_the_same_version() {
        for (store, place) in every_store("reopen").await {
            store.saw_player("key:ann", "ann", 1).await.unwrap();
            store.close().await;
            let again = Store::open(&place.url()).await.unwrap();
            assert_eq!(again.player("key:ann").await.unwrap().unwrap().name, "ann");
            again.close().await;
            place.remove().await;
        }
    }
}
