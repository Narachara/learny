//! Locating and opening the Learny database.
//!
//! The server reads the very same file the desktop app writes, so whatever is
//! on screen is what the tools see. `open_db` runs migrations and enables WAL,
//! which also lets the app and this server read concurrently.

use std::path::PathBuf;

use adapter_sqlite::{
    open_db, SqliteCardRepository, SqliteDeckRepository, SqliteTagRepository,
};
use anyhow::{anyhow, Context, Result};
use application::flashcard_service::FlashcardService;
use application::tagging_service::TaggingService;

/// Tauri derives the app data directory from this (see tauri.conf.json).
const APP_IDENTIFIER: &str = "com.narachara.learny";

/// The desktop app is single-user and stores everything under this id.
const DEFAULT_USER_ID: &str = "local";

/// The services the tools are built on.
pub struct Db {
    pub flashcard: FlashcardService,
    pub tagging: TaggingService,
}

impl Db {
    pub fn open() -> Result<Self> {
        let db_path = database_path()?;
        if !db_path.exists() {
            return Err(anyhow!(
                "no Learny database at {}. Run the desktop app once to create it, \
                 or set LEARNY_DB_PATH to an existing database.",
                db_path.display()
            ));
        }
        // Media paths are stored relative to the data directory, so the repos
        // need it alongside the connection.
        let data_dir = db_path
            .parent()
            .ok_or_else(|| anyhow!("database path has no parent directory"))?
            .to_path_buf();

        let conn = open_db(&db_path)
            .with_context(|| format!("opening database at {}", db_path.display()))?;

        let user_id = std::env::var("LEARNY_USER_ID")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_USER_ID.to_string());

        let cards = || Box::new(SqliteCardRepository::new(conn.clone(), data_dir.clone()));
        let decks = || Box::new(SqliteDeckRepository::new(conn.clone(), data_dir.clone()));
        let tags = || Box::new(SqliteTagRepository::new(conn.clone()));

        Ok(Self {
            flashcard: FlashcardService::new(cards(), decks(), user_id),
            tagging: TaggingService::new(tags(), cards()),
        })
    }
}

/// `LEARNY_DB_PATH` wins; otherwise the platform location Tauri would use.
fn database_path() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("LEARNY_DB_PATH").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(p));
    }
    Ok(app_data_dir()?.join("cards.db"))
}

#[cfg(target_os = "macos")]
fn app_data_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME is not set"))?;
    Ok(PathBuf::from(home)
        .join("Library/Application Support")
        .join(APP_IDENTIFIER))
}

#[cfg(target_os = "linux")]
fn app_data_dir() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .ok_or_else(|| anyhow!("neither XDG_DATA_HOME nor HOME is set"))?;
    Ok(base.join(APP_IDENTIFIER))
}

#[cfg(target_os = "windows")]
fn app_data_dir() -> Result<PathBuf> {
    let base = std::env::var_os("APPDATA").ok_or_else(|| anyhow!("APPDATA is not set"))?;
    Ok(PathBuf::from(base).join(APP_IDENTIFIER))
}
