pub mod schema;
pub mod card_repo_impl;
pub mod deck_repo_impl;
pub mod tag_repo_impl;
pub mod user_repo_impl;
pub mod transaction_impl;

pub use card_repo_impl::SqliteCardRepository;
pub use deck_repo_impl::SqliteDeckRepository;
pub use tag_repo_impl::SqliteTagRepository;
pub use user_repo_impl::SqliteUserRepository;
pub use transaction_impl::SqliteTransactionScope;

use std::path::Path;
use std::sync::{Arc, Mutex};
use rusqlite::Connection;
use rusqlite::functions::FunctionFlags;

/// A connection shared between all adapters in the same process.
pub type SharedConnection = Arc<Mutex<Connection>>;

/// Open (or create) the SQLite database, enable foreign keys, and run migrations.
pub fn open_db(db_path: &Path) -> Result<SharedConnection, rusqlite::Error> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    // WAL + NORMAL makes commits far cheaper (one WAL append instead of a
    // journal rewrite) and stays durable across app crashes; on a power loss
    // the last transactions may be lost but the database stays consistent.
    let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
    conn.execute_batch("PRAGMA synchronous = NORMAL;")?;
    register_functions(&conn)?;
    schema::run_migrations(&conn)?;
    Ok(Arc::new(Mutex::new(conn)))
}

/// Register custom SQL functions on a connection.
///
/// Adds a `REGEXP` operator backed by the `regex` crate so the card search can
/// support `text REGEXP pattern`. The compiled regex is cached on the function's
/// aux data, so a pattern is only compiled once per query rather than per row.
pub fn register_functions(conn: &Connection) -> rusqlite::Result<()> {
    conn.create_scalar_function(
        "regexp",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;
            let re: Arc<regex::Regex> = ctx.get_or_create_aux(0, |vr| -> Result<_, BoxError> {
                Ok(regex::Regex::new(vr.as_str()?)?)
            })?;
            // Column 1 may be NULL (e.g. an empty text field) — treat as no match.
            let text = match ctx.get_raw(1).as_str_or_null()
                .map_err(|e| rusqlite::Error::UserFunctionError(e.into()))?
            {
                Some(t) => t,
                None => return Ok(false),
            };
            Ok(re.is_match(text))
        },
    )
}
