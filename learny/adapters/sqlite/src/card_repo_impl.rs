use std::path::PathBuf;
use rusqlite::params;
use domain_flashcard::{Block, Card};
use ports::card_repo::CardRepository;
use ports::RepositoryError;
use crate::SharedConnection;

pub struct SqliteCardRepository {
    conn: SharedConnection,
    /// Needed to resolve virtual paths (e.g. "files/uuid.png") for file deletion.
    app_data_dir: PathBuf,
}

impl SqliteCardRepository {
    pub fn new(conn: SharedConnection, app_data_dir: PathBuf) -> Self {
        Self { conn, app_data_dir }
    }

    fn load_blocks(&self, card_id: i64, conn: &rusqlite::Connection) -> Result<(Vec<Block>, Vec<Block>), rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT side, content FROM block WHERE card_id = ?1 ORDER BY position ASC"
        )?;

        let mut front = Vec::new();
        let mut back  = Vec::new();

        let rows = stmt.query_map([card_id], |row| {
            let side:    String = row.get(0)?;
            let content: String = row.get(1)?;
            Ok((side, content))
        })?;

        for row in rows {
            let (side, content) = row?;
            let block: Block = serde_json::from_str(&content)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                    0, rusqlite::types::Type::Text, Box::new(e),
                ))?;
            if side == "front" { front.push(block); } else { back.push(block); }
        }

        Ok((front, back))
    }

    fn insert_blocks(
        card_id: i64,
        front: &[Block],
        back:  &[Block],
        conn: &rusqlite::Connection,
    ) -> Result<(), rusqlite::Error> {
        conn.execute("DELETE FROM block WHERE card_id = ?1", [card_id])?;

        for (i, block) in front.iter().enumerate() {
            conn.execute(
                "INSERT INTO block (card_id, side, position, block_type, content)
                 VALUES (?1, 'front', ?2, ?3, ?4)",
                params![
                    card_id, i as i64,
                    block.block_type(),
                    serde_json::to_string(block).unwrap(),
                ],
            )?;
        }
        for (i, block) in back.iter().enumerate() {
            conn.execute(
                "INSERT INTO block (card_id, side, position, block_type, content)
                 VALUES (?1, 'back', ?2, ?3, ?4)",
                params![
                    card_id, i as i64,
                    block.block_type(),
                    serde_json::to_string(block).unwrap(),
                ],
            )?;
        }

        Ok(())
    }

    fn delete_block_files(&self, card_id: i64, conn: &rusqlite::Connection) {
        let mut stmt = match conn.prepare(
            "SELECT content FROM block WHERE card_id = ?1"
        ) {
            Ok(s)  => s,
            Err(_) => return,
        };

        let blocks: Vec<Block> = stmt
            .query_map([card_id], |row| {
                let content: String = row.get(0)?;
                let block: Block = serde_json::from_str(&content)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                        0, rusqlite::types::Type::Text, Box::new(e),
                    ))?;
                Ok(block)
            })
            .unwrap()
            .filter_map(Result::ok)
            .collect();

        for block in blocks {
            if let Some(path) = block.file_path() {
                let full_path = self.app_data_dir.join(path);
                if full_path.exists() {
                    let _ = std::fs::remove_file(full_path);
                }
            }
        }
    }
}

fn map_err(e: rusqlite::Error) -> RepositoryError {
    RepositoryError::Internal(e.to_string())
}

/// Escape SQL `LIKE` wildcards so the query is matched literally.
/// Pair with `ESCAPE '\'` in the statement.
fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

impl CardRepository for SqliteCardRepository {
    fn find_by_id(&self, id: i64) -> Result<Option<Card>, RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;

        let result = conn.query_row(
            "SELECT id, deck_id, name, created_at, times_seen, times_correct, cover_image,
                    what_was_easy, what_was_difficult
             FROM card WHERE id = ?1",
            [id],
            |row| Ok(Card {
                id:                 row.get(0)?,
                deck_id:            row.get(1)?,
                name:               row.get(2)?,
                created_at:         row.get(3)?,
                times_seen:         row.get(4)?,
                times_correct:      row.get(5)?,
                cover_image:        row.get(6)?,
                what_was_easy:      row.get(7)?,
                what_was_difficult: row.get(8)?,
                front_blocks:       vec![],
                back_blocks:        vec![],
            }),
        );

        let mut card = match result {
            Ok(c)                                       => c,
            Err(rusqlite::Error::QueryReturnedNoRows)   => return Ok(None),
            Err(e)                                      => return Err(map_err(e)),
        };

        let (front, back) = self.load_blocks(id, &conn).map_err(map_err)?;
        card.front_blocks = front;
        card.back_blocks  = back;

        Ok(Some(card))
    }

    fn find_by_deck(&self, deck_id: i64) -> Result<Vec<Card>, RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;

        let mut stmt = conn.prepare(
            "SELECT id, deck_id, name, created_at, times_seen, times_correct, cover_image,
                    what_was_easy, what_was_difficult
             FROM card WHERE deck_id = ?1 ORDER BY created_at DESC",
        ).map_err(map_err)?;

        let cards = stmt
            .query_map([deck_id], |row| Ok(Card {
                id:                 row.get(0)?,
                deck_id:            row.get(1)?,
                name:               row.get(2)?,
                created_at:         row.get(3)?,
                times_seen:         row.get(4)?,
                times_correct:      row.get(5)?,
                cover_image:        row.get(6)?,
                what_was_easy:      row.get(7)?,
                what_was_difficult: row.get(8)?,
                front_blocks:       vec![],
                back_blocks:        vec![],
            }))
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;

        // List view — blocks are intentionally not loaded here.
        Ok(cards)
    }

    fn find_by_deck_full(&self, deck_id: i64) -> Result<Vec<Card>, RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;

        let mut stmt = conn.prepare(
            "SELECT id, deck_id, name, created_at, times_seen, times_correct, cover_image,
                    what_was_easy, what_was_difficult
             FROM card WHERE deck_id = ?1 ORDER BY created_at DESC",
        ).map_err(map_err)?;
        let mut cards: Vec<Card> = stmt.query_map([deck_id], |row| Ok(Card {
            id:                 row.get(0)?,
            deck_id:            row.get(1)?,
            name:               row.get(2)?,
            created_at:         row.get(3)?,
            times_seen:         row.get(4)?,
            times_correct:      row.get(5)?,
            cover_image:        row.get(6)?,
            what_was_easy:      row.get(7)?,
            what_was_difficult: row.get(8)?,
            front_blocks:       vec![],
            back_blocks:        vec![],
        }))
        .map_err(map_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_err)?;
        drop(stmt);

        if cards.is_empty() { return Ok(cards); }

        // Load all blocks for this deck in one query, distribute to cards.
        let mut stmt = conn.prepare(
            "SELECT b.card_id, b.side, b.content
             FROM block b
             JOIN card c ON c.id = b.card_id
             WHERE c.deck_id = ?1
             ORDER BY b.card_id, b.position ASC",
        ).map_err(map_err)?;

        let rows = stmt.query_map([deck_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        }).map_err(map_err)?.collect::<Result<Vec<_>, _>>().map_err(map_err)?;

        let card_index: std::collections::HashMap<i64, usize> =
            cards.iter().enumerate().map(|(i, c)| (c.id, i)).collect();

        for (card_id, side, content) in rows {
            let block: Block = serde_json::from_str(&content)
                .map_err(|e| RepositoryError::Internal(e.to_string()))?;
            if let Some(&idx) = card_index.get(&card_id) {
                if side == "front" { cards[idx].front_blocks.push(block); }
                else               { cards[idx].back_blocks.push(block); }
            }
        }

        Ok(cards)
    }

    /// Insert or update a card and atomically replace its blocks.
    /// Uses a transaction so the card row and block rows are never out of sync.
    fn save(&self, card: &mut Card) -> Result<(), RepositoryError> {
        let mut conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        // Savepoint, not a transaction: bulk operations (deck import) wrap
        // many saves in one outer transaction, and BEGIN doesn't nest.
        let tx = conn.savepoint().map_err(map_err)?;

        if card.id == -1 {
            tx.execute(
                "INSERT INTO card (deck_id, name, created_at, times_seen, times_correct)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![card.deck_id, card.name, card.created_at, card.times_seen, card.times_correct],
            ).map_err(map_err)?;
            card.id = tx.last_insert_rowid();
        } else {
            tx.execute(
                "UPDATE card SET name = ?1, times_seen = ?2, times_correct = ?3 WHERE id = ?4",
                params![card.name, card.times_seen, card.times_correct, card.id],
            ).map_err(map_err)?;
        }

        Self::insert_blocks(card.id, &card.front_blocks, &card.back_blocks, &tx)
            .map_err(map_err)?;

        tx.commit().map_err(map_err)?;
        Ok(())
    }

    /// Delete a card, clean up any image/file assets, and let blocks cascade.
    fn delete(&self, id: i64) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;

        self.delete_block_files(id, &conn);

        // Delete the card's own cover image if one exists.
        let cover: Option<String> = conn
            .query_row("SELECT cover_image FROM card WHERE id = ?1", [id], |r| r.get(0))
            .unwrap_or(None);
        if let Some(path) = cover {
            let full = self.app_data_dir.join(&path);
            if full.exists() {
                let _ = std::fs::remove_file(full);
            }
        }

        let affected = conn
            .execute("DELETE FROM card WHERE id = ?1", [id])
            .map_err(map_err)?;

        if affected == 0 {
            return Err(RepositoryError::NotFound);
        }

        Ok(())
    }

    fn set_cover_image(&self, card_id: i64, path: Option<String>) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;

        // Delete old file if there was one.
        let old: Option<String> = conn
            .query_row("SELECT cover_image FROM card WHERE id = ?1", [card_id], |r| r.get(0))
            .unwrap_or(None);
        if let Some(old_path) = old {
            let full = self.app_data_dir.join(&old_path);
            if full.exists() {
                let _ = std::fs::remove_file(full);
            }
        }

        conn.execute(
            "UPDATE card SET cover_image = ?1 WHERE id = ?2",
            params![path, card_id],
        ).map_err(map_err)?;

        Ok(())
    }

    fn record_answer(&self, card_id: i64, correct: bool) -> Result<Card, RepositoryError> {
        {
            let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
            let increment = if correct { 1 } else { 0 };
            conn.execute(
                "UPDATE card SET times_seen = times_seen + 1, times_correct = times_correct + ?1
                 WHERE id = ?2",
                params![increment, card_id],
            ).map_err(map_err)?;
        }
        // Reload the updated card with its blocks
        self.find_by_id(card_id)?.ok_or(RepositoryError::NotFound)
    }

    fn save_notes(&self, card_id: i64, easy: Option<String>, difficult: Option<String>) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        conn.execute(
            "UPDATE card SET what_was_easy = ?1, what_was_difficult = ?2 WHERE id = ?3",
            params![easy, difficult, card_id],
        ).map_err(map_err)?;
        Ok(())
    }

    fn move_to_deck(&self, card_ids: &[i64], target_deck_id: i64) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        for &card_id in card_ids {
            conn.execute(
                "UPDATE card SET deck_id = ?1 WHERE id = ?2",
                params![target_deck_id, card_id],
            ).map_err(map_err)?;
        }
        Ok(())
    }

    fn reset_progress(&self, deck_id: i64) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        conn.execute(
            "UPDATE card SET times_seen = 0, times_correct = 0 WHERE deck_id = ?1",
            params![deck_id],
        ).map_err(map_err)?;
        Ok(())
    }

    fn find_by_tags(&self, tag_ids: &[i64]) -> Result<Vec<Card>, RepositoryError> {
        if tag_ids.is_empty() { return Ok(vec![]); }
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        let placeholders = tag_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        // Grouped by card so a card matching several of the given tags is
        // returned once; ordered by the best (lowest) position it holds
        // among those tags, so a tag's progression order is respected.
        let sql = format!(
            "SELECT c.id, c.deck_id, c.name, c.created_at, c.times_seen, c.times_correct, \
             c.cover_image, c.what_was_easy, c.what_was_difficult \
             FROM card c JOIN card_tag ct ON c.id = ct.card_id \
             WHERE ct.tag_id IN ({}) \
             GROUP BY c.id \
             ORDER BY MIN(ct.position) ASC, c.created_at ASC",
            placeholders,
        );
        let cards = conn.prepare(&sql).map_err(map_err)?
            .query_map(rusqlite::params_from_iter(tag_ids.iter().copied()), |row| {
                Ok(Card {
                    id:                 row.get(0)?,
                    deck_id:            row.get(1)?,
                    name:               row.get(2)?,
                    created_at:         row.get(3)?,
                    times_seen:         row.get(4)?,
                    times_correct:      row.get(5)?,
                    cover_image:        row.get(6)?,
                    what_was_easy:      row.get(7)?,
                    what_was_difficult: row.get(8)?,
                    front_blocks:       vec![],
                    back_blocks:        vec![],
                })
            })
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;
        Ok(cards)
    }

    /// Full-text search across card names and block content.
    ///
    /// Matching is exact and contiguous (the whole query, whitespace included,
    /// is matched as a single phrase) rather than word-by-word. Wrap the query
    /// in slashes — `/pattern/` — to match with a regular expression instead.
    fn search(&self, query: &str) -> Result<Vec<Card>, RepositoryError> {
        let trimmed = query.trim();
        if trimmed.is_empty() { return Ok(vec![]); }
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;

        // Two modes, both expressed as a single bound parameter (?1) that's
        // reused for the name and an EXISTS-over-blocks check:
        //   * `/pattern/`  -> REGEXP, the raw regex pattern.
        //   * anything else -> LIKE, an exact substring of the literal query
        //     (wildcards %, _ and \ are escaped so they match literally and the
        //     surrounding %…% keeps internal whitespace intact).
        let is_regex = trimmed.len() >= 2 && trimmed.starts_with('/') && trimmed.ends_with('/');
        let (op, escape, pattern) = if is_regex {
            ("REGEXP", "", trimmed[1..trimmed.len() - 1].to_string())
        } else {
            ("LIKE", " ESCAPE '\\'", format!("%{}%", escape_like(query)))
        };

        let where_clause = format!(
            "(c.name {op} ?1{escape} OR EXISTS \
             (SELECT 1 FROM block b WHERE b.card_id = c.id AND b.content {op} ?1{escape}))",
        );

        let sql = format!(
            "SELECT c.id, c.deck_id, c.name, c.created_at, c.times_seen, c.times_correct, \
             c.cover_image, c.what_was_easy, c.what_was_difficult \
             FROM card c WHERE {} \
             ORDER BY c.deck_id, c.created_at DESC",
            where_clause,
        );

        let mut stmt = conn.prepare(&sql).map_err(map_err)?;
        let mut cards: Vec<Card> = stmt.query_map(params![pattern], |row| {
            Ok(Card {
                id:                 row.get(0)?,
                deck_id:            row.get(1)?,
                name:               row.get(2)?,
                created_at:         row.get(3)?,
                times_seen:         row.get(4)?,
                times_correct:      row.get(5)?,
                cover_image:        row.get(6)?,
                what_was_easy:      row.get(7)?,
                what_was_difficult: row.get(8)?,
                front_blocks:       vec![],
                back_blocks:        vec![],
            })
        }).map_err(map_err)?.collect::<Result<Vec<_>, _>>().map_err(map_err)?;
        drop(stmt);

        if cards.is_empty() { return Ok(cards); }

        // Load blocks for matched cards in one query.
        let id_placeholders = cards.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let block_sql = format!(
            "SELECT card_id, side, content FROM block \
             WHERE card_id IN ({}) ORDER BY card_id, position ASC",
            id_placeholders,
        );
        let card_ids: Vec<i64> = cards.iter().map(|c| c.id).collect();
        let mut stmt = conn.prepare(&block_sql).map_err(map_err)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(card_ids.iter().copied()), |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
            })
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;

        let card_index: std::collections::HashMap<i64, usize> =
            cards.iter().enumerate().map(|(i, c)| (c.id, i)).collect();

        for (card_id, side, content) in rows {
            let block: Block = serde_json::from_str(&content)
                .map_err(|e| RepositoryError::Internal(e.to_string()))?;
            if let Some(&idx) = card_index.get(&card_id) {
                if side == "front" { cards[idx].front_blocks.push(block); }
                else               { cards[idx].back_blocks.push(block); }
            }
        }

        Ok(cards)
    }

    fn find_by_deck_with_tags(&self, deck_id: i64, tag_ids: &[i64]) -> Result<Vec<Card>, RepositoryError> {
        if tag_ids.is_empty() {
            return self.find_by_deck_full(deck_id);
        }
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        let placeholders = tag_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT DISTINCT c.id, c.deck_id, c.name, c.created_at, c.times_seen, c.times_correct, \
             c.cover_image, c.what_was_easy, c.what_was_difficult \
             FROM card c JOIN card_tag ct ON c.id = ct.card_id \
             WHERE c.deck_id = ? AND ct.tag_id IN ({}) \
             ORDER BY c.created_at DESC",
            placeholders,
        );
        let mut all_params: Vec<i64> = vec![deck_id];
        all_params.extend_from_slice(tag_ids);
        let mut stmt = conn.prepare(&sql).map_err(map_err)?;
        let mut cards: Vec<Card> = stmt
            .query_map(rusqlite::params_from_iter(all_params.iter().copied()), |row| {
                Ok(Card {
                    id:                 row.get(0)?,
                    deck_id:            row.get(1)?,
                    name:               row.get(2)?,
                    created_at:         row.get(3)?,
                    times_seen:         row.get(4)?,
                    times_correct:      row.get(5)?,
                    cover_image:        row.get(6)?,
                    what_was_easy:      row.get(7)?,
                    what_was_difficult: row.get(8)?,
                    front_blocks:       vec![],
                    back_blocks:        vec![],
                })
            })
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;
        drop(stmt);

        if cards.is_empty() { return Ok(cards); }

        // Load blocks for the matched cards in one query.
        let id_placeholders = cards.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let block_sql = format!(
            "SELECT card_id, side, content FROM block \
             WHERE card_id IN ({}) ORDER BY card_id, position ASC",
            id_placeholders,
        );
        let card_ids: Vec<i64> = cards.iter().map(|c| c.id).collect();
        let mut stmt = conn.prepare(&block_sql).map_err(map_err)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(card_ids.iter().copied()), |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
            })
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;

        let card_index: std::collections::HashMap<i64, usize> =
            cards.iter().enumerate().map(|(i, c)| (c.id, i)).collect();

        for (card_id, side, content) in rows {
            let block: Block = serde_json::from_str(&content)
                .map_err(|e| RepositoryError::Internal(e.to_string()))?;
            if let Some(&idx) = card_index.get(&card_id) {
                if side == "front" { cards[idx].front_blocks.push(block); }
                else               { cards[idx].back_blocks.push(block); }
            }
        }

        Ok(cards)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::run_migrations;
    use std::sync::{Arc, Mutex};

    fn setup() -> SqliteCardRepository {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::register_functions(&conn).unwrap();
        run_migrations(&conn).unwrap();
        // Insert a deck so FK constraints pass
        conn.execute("INSERT INTO deck (id, name, created_at, card_count) VALUES (1, 'Test', 0, 0)", []).unwrap();
        let shared = Arc::new(Mutex::new(conn));
        SqliteCardRepository::new(shared, PathBuf::from("/tmp"))
    }

    #[test]
    fn save_inserts_and_assigns_id() {
        let repo = setup();
        let mut card = Card::new(1, "Derivatives", 0).unwrap();
        assert_eq!(card.id, -1);
        repo.save(&mut card).unwrap();
        assert!(card.id > 0);
    }

    #[test]
    fn find_by_id_returns_card_with_blocks() {
        let repo = setup();
        let mut card = Card::new(1, "Test", 0).unwrap();
        card.front_blocks = vec![Block::Text { value: "Question".into() }];
        repo.save(&mut card).unwrap();

        let loaded = repo.find_by_id(card.id).unwrap().unwrap();
        assert_eq!(loaded.name, "Test");
        assert_eq!(loaded.front_blocks.len(), 1);
    }

    #[test]
    fn save_replaces_blocks_atomically() {
        let repo = setup();
        let mut card = Card::new(1, "Test", 0).unwrap();
        card.front_blocks = vec![Block::Text { value: "old".into() }];
        repo.save(&mut card).unwrap();

        card.front_blocks = vec![Block::Text { value: "new".into() }];
        repo.save(&mut card).unwrap();

        let loaded = repo.find_by_id(card.id).unwrap().unwrap();
        assert_eq!(loaded.front_blocks.len(), 1);
        if let Block::Text { value } = &loaded.front_blocks[0] {
            assert_eq!(value, "new");
        } else {
            panic!("wrong block type");
        }
    }

    #[test]
    fn delete_removes_card() {
        let repo = setup();
        let mut card = Card::new(1, "Test", 0).unwrap();
        repo.save(&mut card).unwrap();
        repo.delete(card.id).unwrap();
        assert!(repo.find_by_id(card.id).unwrap().is_none());
    }

    fn card_with_text(repo: &SqliteCardRepository, name: &str, text: &str) -> i64 {
        let mut card = Card::new(1, name, 0).unwrap();
        card.front_blocks = vec![Block::Text { value: text.into() }];
        repo.save(&mut card).unwrap();
        card.id
    }

    #[test]
    fn search_matches_exact_phrase_not_separate_words() {
        let repo = setup();
        let hit  = card_with_text(&repo, "A", "the identical trails diverge");
        let _miss = card_with_text(&repo, "B", "identical and the trails apart");

        let results = repo.search("identical trails").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, hit);
    }

    #[test]
    fn search_is_case_insensitive_substring() {
        let repo = setup();
        let id = card_with_text(&repo, "Title", "Hello World");
        let results = repo.search("hello wor").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id);
    }

    #[test]
    fn search_escapes_like_wildcards() {
        let repo = setup();
        let hit = card_with_text(&repo, "A", "discount is 50% today");
        let _miss = card_with_text(&repo, "B", "no discount here");

        // The '%' must be matched literally, not as a wildcard.
        let results = repo.search("50%").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, hit);
    }

    #[test]
    fn search_supports_regex_mode() {
        let repo = setup();
        let a = card_with_text(&repo, "A", "order 123 shipped");
        let _b = card_with_text(&repo, "B", "order shipped soon");

        let results = repo.search(r"/order \d+/").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, a);
    }

    #[test]
    fn record_answer_increments_counts() {
        let repo = setup();
        let mut card = Card::new(1, "Test", 0).unwrap();
        repo.save(&mut card).unwrap();

        repo.record_answer(card.id, true).unwrap();
        let updated = repo.find_by_id(card.id).unwrap().unwrap();
        assert_eq!(updated.times_seen, 1);
        assert_eq!(updated.times_correct, 1);

        repo.record_answer(card.id, false).unwrap();
        let updated = repo.find_by_id(card.id).unwrap().unwrap();
        assert_eq!(updated.times_seen, 2);
        assert_eq!(updated.times_correct, 1);
    }
}
