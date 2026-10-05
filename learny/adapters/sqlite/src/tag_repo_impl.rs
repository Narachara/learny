use rusqlite::{params, ErrorCode};
use domain_tagging::{CardTagInfo, Tag};
use ports::tag_repo::TagRepository;
use ports::RepositoryError;
use crate::SharedConnection;

pub struct SqliteTagRepository {
    conn: SharedConnection,
}

impl SqliteTagRepository {
    pub fn new(conn: SharedConnection) -> Self {
        Self { conn }
    }
}

fn map_err(e: rusqlite::Error) -> RepositoryError {
    RepositoryError::Internal(e.to_string())
}

fn is_unique_violation(e: &rusqlite::Error) -> bool {
    matches!(e, rusqlite::Error::SqliteFailure(f, _) if f.code == ErrorCode::ConstraintViolation)
}

fn row_to_tag(row: &rusqlite::Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: row.get(0)?,
        name: row.get(1)?,
        pos_x: row.get(2)?,
        pos_y: row.get(3)?,
    })
}

impl TagRepository for SqliteTagRepository {
    fn create(&self, name: &str) -> Result<Tag, RepositoryError> {
        let name = name.trim();
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        match conn.execute("INSERT INTO tag (name) VALUES (?1)", [name]) {
            Ok(_) => Ok(Tag {
                id: conn.last_insert_rowid(),
                name: name.to_string(),
                pos_x: None,
                pos_y: None,
            }),
            Err(e) if is_unique_violation(&e) => Err(RepositoryError::DuplicateName),
            Err(e) => Err(map_err(e)),
        }
    }

    fn find_by_id(&self, id: i64) -> Result<Option<Tag>, RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        match conn.query_row("SELECT id, name, pos_x, pos_y FROM tag WHERE id = ?1", [id], row_to_tag) {
            Ok(t) => Ok(Some(t)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(map_err(e)),
        }
    }

    fn find_by_name(&self, name: &str) -> Result<Option<Tag>, RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        match conn.query_row("SELECT id, name, pos_x, pos_y FROM tag WHERE name = ?1", [name.trim()], row_to_tag) {
            Ok(t) => Ok(Some(t)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(map_err(e)),
        }
    }

    fn find_all(&self) -> Result<Vec<Tag>, RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        let mut stmt = conn.prepare("SELECT id, name, pos_x, pos_y FROM tag ORDER BY name ASC").map_err(map_err)?;
        let tags = stmt.query_map([], row_to_tag)
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;
        Ok(tags)
    }

    fn find_in_use(&self) -> Result<Vec<Tag>, RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT t.id, t.name, t.pos_x, t.pos_y FROM tag t
             INNER JOIN card_tag ct ON ct.tag_id = t.id
             ORDER BY t.name ASC",
        ).map_err(map_err)?;
        let tags = stmt.query_map([], row_to_tag)
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;
        Ok(tags)
    }

    fn rename(&self, id: i64, new_name: &str) -> Result<Tag, RepositoryError> {
        let new_name = new_name.trim();
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        match conn.execute("UPDATE tag SET name = ?1 WHERE id = ?2", params![new_name, id]) {
            Ok(0) => Err(RepositoryError::NotFound),
            Ok(_) => match conn.query_row(
                "SELECT id, name, pos_x, pos_y FROM tag WHERE id = ?1",
                [id],
                row_to_tag,
            ) {
                Ok(t) => Ok(t),
                Err(e) => Err(map_err(e)),
            },
            Err(e) if is_unique_violation(&e) => Err(RepositoryError::DuplicateName),
            Err(e) => Err(map_err(e)),
        }
    }

    fn delete(&self, id: i64) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        conn.execute("DELETE FROM tag WHERE id = ?1", [id]).map_err(map_err)?;
        Ok(())
    }

    fn add_edge(&self, parent_id: i64, child_id: i64) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        conn.execute(
            "INSERT OR IGNORE INTO tag_edge (parent_id, child_id) VALUES (?1, ?2)",
            params![parent_id, child_id],
        ).map_err(map_err)?;
        Ok(())
    }

    fn remove_edge(&self, parent_id: i64, child_id: i64) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        conn.execute(
            "DELETE FROM tag_edge WHERE parent_id = ?1 AND child_id = ?2",
            params![parent_id, child_id],
        ).map_err(map_err)?;
        Ok(())
    }

    fn all_edges(&self) -> Result<Vec<(i64, i64)>, RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        let mut stmt = conn.prepare(
            "SELECT parent_id, child_id FROM tag_edge",
        ).map_err(map_err)?;
        let edges = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;
        Ok(edges)
    }

    fn assign_to_deck(&self, tag_id: i64, deck_id: i64) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        conn.execute(
            "INSERT OR IGNORE INTO tag_deck (tag_id, deck_id) VALUES (?1, ?2)",
            params![tag_id, deck_id],
        ).map_err(map_err)?;
        Ok(())
    }

    fn unassign_from_deck(&self, tag_id: i64, deck_id: i64) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        conn.execute(
            "DELETE FROM tag_deck WHERE tag_id = ?1 AND deck_id = ?2",
            params![tag_id, deck_id],
        ).map_err(map_err)?;
        Ok(())
    }

    fn tag_deck_pairs(&self) -> Result<Vec<(i64, i64)>, RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT ct.tag_id, c.deck_id FROM card_tag ct JOIN card c ON ct.card_id = c.id
             UNION
             SELECT tag_id, deck_id FROM tag_deck",
        ).map_err(map_err)?;
        let pairs = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;
        Ok(pairs)
    }

    fn set_position(&self, tag_id: i64, x: f64, y: f64) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        conn.execute(
            "UPDATE tag SET pos_x = ?1, pos_y = ?2 WHERE id = ?3",
            params![x, y, tag_id],
        ).map_err(map_err)?;
        Ok(())
    }

    fn clear_position(&self, tag_id: i64) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        conn.execute(
            "UPDATE tag SET pos_x = NULL, pos_y = NULL WHERE id = ?1",
            [tag_id],
        ).map_err(map_err)?;
        Ok(())
    }

    fn reset_map(&self) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        conn.execute("DELETE FROM tag_edge", []).map_err(map_err)?;
        conn.execute("UPDATE tag SET pos_x = NULL, pos_y = NULL", []).map_err(map_err)?;
        Ok(())
    }

    fn assign_to_card(&self, tag_id: i64, card_id: i64) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        // New cards join at the end of the tag's progression rather than at
        // position 0, so assigning a tag never jumps a card to the front.
        conn.execute(
            "INSERT OR IGNORE INTO card_tag (card_id, tag_id, position)
             VALUES (?1, ?2, (SELECT COALESCE(MAX(position), -1) + 1 FROM card_tag WHERE tag_id = ?2))",
            params![card_id, tag_id],
        ).map_err(map_err)?;
        Ok(())
    }

    fn remove_from_card(&self, tag_id: i64, card_id: i64) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        conn.execute(
            "DELETE FROM card_tag WHERE card_id = ?1 AND tag_id = ?2",
            params![card_id, tag_id],
        ).map_err(map_err)?;
        Ok(())
    }

    fn detach_all_cards(&self, tag_id: i64) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        conn.execute("DELETE FROM card_tag WHERE tag_id = ?1", [tag_id]).map_err(map_err)?;
        Ok(())
    }

    fn tags_for_card(&self, card_id: i64) -> Result<Vec<Tag>, RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.pos_x, t.pos_y FROM tag t
             JOIN card_tag ct ON ct.tag_id = t.id
             WHERE ct.card_id = ?1 ORDER BY t.name ASC",
        ).map_err(map_err)?;
        let tags = stmt.query_map([card_id], row_to_tag)
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;
        Ok(tags)
    }

    fn tags_for_card_with_position(&self, card_id: i64) -> Result<Vec<CardTagInfo>, RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.pos_x, t.pos_y, ct.position FROM tag t
             JOIN card_tag ct ON ct.tag_id = t.id
             WHERE ct.card_id = ?1 ORDER BY t.name ASC",
        ).map_err(map_err)?;
        let infos = stmt.query_map([card_id], |row| {
                Ok(CardTagInfo { tag: row_to_tag(row)?, position: row.get(4)? })
            })
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;
        Ok(infos)
    }

    fn set_card_tag_position(&self, tag_id: i64, card_id: i64, position: i64) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        conn.execute(
            "UPDATE card_tag SET position = ?3 WHERE tag_id = ?1 AND card_id = ?2",
            params![tag_id, card_id, position],
        ).map_err(map_err)?;
        Ok(())
    }

    fn card_positions_for_tags(&self, tag_ids: &[i64]) -> Result<Vec<(i64, i64, i64)>, RepositoryError> {
        if tag_ids.is_empty() { return Ok(vec![]); }
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        let placeholders = tag_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT tag_id, card_id, position FROM card_tag WHERE tag_id IN ({})",
            placeholders,
        );
        let triples = conn.prepare(&sql).map_err(map_err)?
            .query_map(rusqlite::params_from_iter(tag_ids.iter().copied()), |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;
        Ok(triples)
    }

    fn card_ids_for_tag(&self, tag_id: i64) -> Result<Vec<i64>, RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        let mut stmt = conn.prepare(
            "SELECT card_id FROM card_tag WHERE tag_id = ?1 ORDER BY position ASC, card_id ASC",
        ).map_err(map_err)?;
        let ids = stmt.query_map([tag_id], |row| row.get(0))
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;
        Ok(ids)
    }

    fn card_tag_pairs_for_deck(&self, deck_id: i64) -> Result<Vec<(i64, i64)>, RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        let mut stmt = conn.prepare(
            "SELECT ct.card_id, ct.tag_id FROM card_tag ct
             JOIN card c ON ct.card_id = c.id
             WHERE c.deck_id = ?1",
        ).map_err(map_err)?;
        let pairs = stmt.query_map([deck_id], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;
        Ok(pairs)
    }

    fn tags_for_deck(&self, deck_id: i64) -> Result<Vec<Tag>, RepositoryError> {
        let conn = self.conn.lock().map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT t.id, t.name, t.pos_x, t.pos_y FROM tag t
             JOIN card_tag ct ON t.id = ct.tag_id
             JOIN card c ON ct.card_id = c.id
             WHERE c.deck_id = ?1
             ORDER BY t.name ASC",
        ).map_err(map_err)?;
        let tags = stmt.query_map([deck_id], row_to_tag)
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;
        Ok(tags)
    }
}
