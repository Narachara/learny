use std::path::PathBuf;
use rusqlite::params;
use domain_flashcard::Deck;
use ports::deck_repo::DeckRepository;
use ports::RepositoryError;
use crate::SharedConnection;

pub struct SqliteDeckRepository {
    conn: SharedConnection,
    /// Needed to resolve virtual paths (e.g. "files/uuid.png") for cover deletion.
    app_data_dir: PathBuf,
}

impl SqliteDeckRepository {
    pub fn new(conn: SharedConnection, app_data_dir: PathBuf) -> Self {
        Self { conn, app_data_dir }
    }
}

fn map_err(e: rusqlite::Error) -> RepositoryError {
    RepositoryError::Internal(e.to_string())
}

fn row_to_deck(row: &rusqlite::Row<'_>) -> rusqlite::Result<Deck> {
    Ok(Deck {
        id:                row.get(0)?,
        name:              row.get(1)?,
        created_at:        row.get(2)?,
        card_count:        row.get(3)?,
        cover_image:       row.get(4)?,
        user_id:           row.get(5)?,
    })
}

impl DeckRepository for SqliteDeckRepository {
    fn find_by_id(&self, id: i64) -> Result<Option<Deck>, RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;

        let result = conn.query_row(
            "SELECT id, name, created_at, card_count, cover_image, user_id FROM deck WHERE id = ?1",
            [id],
            row_to_deck,
        );

        match result {
            Ok(deck)                                    => Ok(Some(deck)),
            Err(rusqlite::Error::QueryReturnedNoRows)   => Ok(None),
            Err(e)                                      => Err(map_err(e)),
        }
    }

    fn find_all_for_user(&self, user_id: &str) -> Result<Vec<Deck>, RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;

        let mut stmt = conn
            .prepare("SELECT id, name, created_at, card_count, cover_image, user_id FROM deck WHERE user_id = ?1 ORDER BY created_at DESC")
            .map_err(map_err)?;

        let decks = stmt
            .query_map([user_id], row_to_deck)
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;

        Ok(decks)
    }

    fn save(&self, deck: &mut Deck) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;

        if deck.id == -1 {
            conn.execute(
                "INSERT INTO deck (name, created_at, card_count, user_id) VALUES (?1, ?2, 0, ?3)",
                params![deck.name, deck.created_at, deck.user_id],
            ).map_err(map_err)?;
            deck.id = conn.last_insert_rowid();
        } else {
            conn.execute(
                "UPDATE deck SET name = ?1 WHERE id = ?2",
                params![deck.name, deck.id],
            ).map_err(map_err)?;
        }

        Ok(())
    }

    fn delete(&self, id: i64) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;

        let cover: Option<String> = conn
            .query_row("SELECT cover_image FROM deck WHERE id = ?1", [id], |r| r.get(0))
            .unwrap_or(None);
        if let Some(path) = cover {
            let full = self.app_data_dir.join(&path);
            if full.exists() {
                let _ = std::fs::remove_file(full);
            }
        }

        let affected = conn
            .execute("DELETE FROM deck WHERE id = ?1", [id])
            .map_err(map_err)?;

        if affected == 0 {
            return Err(RepositoryError::NotFound);
        }

        Ok(())
    }

    fn set_cover_image(&self, deck_id: i64, path: Option<String>) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;

        let old: Option<String> = conn
            .query_row("SELECT cover_image FROM deck WHERE id = ?1", [deck_id], |r| r.get(0))
            .unwrap_or(None);
        if let Some(old_path) = old {
            let full = self.app_data_dir.join(&old_path);
            if full.exists() {
                let _ = std::fs::remove_file(full);
            }
        }

        conn.execute(
            "UPDATE deck SET cover_image = ?1 WHERE id = ?2",
            params![path, deck_id],
        ).map_err(map_err)?;

        Ok(())
    }
}
