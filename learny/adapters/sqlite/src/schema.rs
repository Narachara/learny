use rusqlite::Connection;

/// Run all pending migrations against `conn`.
/// Uses PRAGMA user_version as the schema version counter.
pub fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    // Read once; update the local variable after each migration so that a
    // fresh install (version = 0) runs all pending migrations in sequence
    // rather than relying on the snapshot staying stale.
    let mut version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;

    if version < 1 {
        migrate_v1(conn)?;
        conn.execute_batch("PRAGMA user_version = 1")?;
        version = 1;
    }

    if version < 2 {
        migrate_v2(conn)?;
        conn.execute_batch("PRAGMA user_version = 2")?;
        version = 2;
    }

    if version < 3 {
        migrate_v3(conn)?;
        conn.execute_batch("PRAGMA user_version = 3")?;
        version = 3;
    }

    if version < 4 {
        migrate_v4(conn)?;
        conn.execute_batch("PRAGMA user_version = 4")?;
        version = 4;
    }

    if version < 5 {
        migrate_v5(conn)?;
        conn.execute_batch("PRAGMA user_version = 5")?;
        version = 5;
    }

    if version < 6 {
        migrate_v6(conn)?;
        conn.execute_batch("PRAGMA user_version = 6")?;
        version = 6;
    }

    if version < 7 {
        migrate_v7(conn)?;
        conn.execute_batch("PRAGMA user_version = 7")?;
        version = 7;
    }

    if version < 8 {
        migrate_v8(conn)?;
        conn.execute_batch("PRAGMA user_version = 8")?;
        version = 8;
    }

    if version < 9 {
        migrate_v9(conn)?;
        conn.execute_batch("PRAGMA user_version = 9")?;
        version = 9;
    }

    if version < 10 {
        migrate_v10(conn)?;
        conn.execute_batch("PRAGMA user_version = 10")?;
        version = 10;
    }

    if version < 11 {
        migrate_v11(conn)?;
        conn.execute_batch("PRAGMA user_version = 11")?;
        version = 11;
    }

    if version < 12 {
        migrate_v12(conn)?;
        conn.execute_batch("PRAGMA user_version = 12")?;
        version = 12;
    }

    if version < 13 {
        migrate_v13(conn)?;
        conn.execute_batch("PRAGMA user_version = 13")?;
        version = 13;
    }

    if version < 14 {
        migrate_v14(conn)?;
        conn.execute_batch("PRAGMA user_version = 14")?;
        version = 14;
    }

    if version < 15 {
        migrate_v15(conn)?;
        conn.execute_batch("PRAGMA user_version = 15")?;
        version = 15;
    }

    if version < 16 {
        migrate_v16(conn)?;
        conn.execute_batch("PRAGMA user_version = 16")?;
        version = 16;
    }

    if version < 17 {
        migrate_v17(conn)?;
        conn.execute_batch("PRAGMA user_version = 17")?;
        version = 17;
    }

    if version < 18 {
        migrate_v18(conn)?;
        conn.execute_batch("PRAGMA user_version = 18")?;
        let _ = version;
    }

    Ok(())
}

/// v1 — full initial schema.
/// All CREATE statements are IF NOT EXISTS so this is safe to run against
/// a database that already has the tables (e.g. existing users).
fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS deck (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            name       TEXT    NOT NULL,
            created_at INTEGER NOT NULL,
            card_count INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS card (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            deck_id       INTEGER NOT NULL REFERENCES deck(id) ON DELETE CASCADE,
            name          TEXT    NOT NULL,
            created_at    INTEGER NOT NULL,
            times_seen    INTEGER NOT NULL DEFAULT 0,
            times_correct INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS block (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            card_id    INTEGER NOT NULL,
            side       TEXT    NOT NULL,
            position   INTEGER NOT NULL,
            block_type TEXT    NOT NULL,
            content    TEXT    NOT NULL,
            FOREIGN KEY (card_id) REFERENCES card(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS tag (
            id   INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE
        );

        CREATE TABLE IF NOT EXISTS card_tag (
            card_id INTEGER NOT NULL REFERENCES card(id) ON DELETE CASCADE,
            tag_id  INTEGER NOT NULL REFERENCES tag(id)  ON DELETE CASCADE,
            PRIMARY KEY (card_id, tag_id)
        );

        -- Keep deck.card_count in sync automatically.
        -- Fixes the two known bugs: count was never incremented on insert
        -- or decremented on delete.
        CREATE TRIGGER IF NOT EXISTS trg_card_insert_count
        AFTER INSERT ON card
        BEGIN
            UPDATE deck SET card_count = card_count + 1 WHERE id = NEW.deck_id;
        END;

        CREATE TRIGGER IF NOT EXISTS trg_card_delete_count
        AFTER DELETE ON card
        BEGIN
            UPDATE deck SET card_count = card_count - 1 WHERE id = OLD.deck_id;
        END;
    ")?;

    // Drop the legacy comma-separated tags column from existing databases.
    // Silently ignored on a fresh install where the column never existed.
    let _ = conn.execute_batch("ALTER TABLE card DROP COLUMN tags;");

    Ok(())
}

/// v2 — cover image support for decks and cards.
fn migrate_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
    let _ = conn.execute_batch("ALTER TABLE deck ADD COLUMN cover_image TEXT;");
    let _ = conn.execute_batch("ALTER TABLE card ADD COLUMN cover_image TEXT;");
    Ok(())
}

/// v3 — user accounts for web authentication.
fn migrate_v3(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS user (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            username      TEXT NOT NULL UNIQUE COLLATE NOCASE,
            password_hash TEXT NOT NULL
        );
    ")
}

/// v9 — knowledge-map support for tags: optional parent (manual hierarchy)
/// and persisted canvas position (pos_x, pos_y). All nullable so existing
/// tags are unaffected.
fn migrate_v9(conn: &Connection) -> Result<(), rusqlite::Error> {
    let _ = conn.execute_batch("ALTER TABLE tag ADD COLUMN parent_tag_id INTEGER REFERENCES tag(id) ON DELETE SET NULL;");
    let _ = conn.execute_batch("ALTER TABLE tag ADD COLUMN pos_x REAL;");
    let _ = conn.execute_batch("ALTER TABLE tag ADD COLUMN pos_y REAL;");
    Ok(())
}

/// v10 — knowledge map becomes a DAG: a tag can have multiple parents.
/// Replaces the single `tag.parent_tag_id` with a many-to-many `tag_edge`
/// table (parent_id → child_id). Existing single-parent links are migrated in;
/// the old column is left in place but unused.
fn migrate_v10(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS tag_edge (
            parent_id INTEGER NOT NULL REFERENCES tag(id) ON DELETE CASCADE,
            child_id  INTEGER NOT NULL REFERENCES tag(id) ON DELETE CASCADE,
            PRIMARY KEY (parent_id, child_id)
        );
    ")?;
    // Migrate any existing single-parent links into the edge table.
    let _ = conn.execute_batch("
        INSERT OR IGNORE INTO tag_edge (parent_id, child_id)
        SELECT parent_tag_id, id FROM tag WHERE parent_tag_id IS NOT NULL;
    ");
    Ok(())
}

/// v11 — explicit tag → deck assignments, independent of cards. Lets a tag
/// belong to a deck (for the knowledge map) before any card uses it.
fn migrate_v11(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS tag_deck (
            tag_id  INTEGER NOT NULL REFERENCES tag(id)  ON DELETE CASCADE,
            deck_id INTEGER NOT NULL REFERENCES deck(id) ON DELETE CASCADE,
            PRIMARY KEY (tag_id, deck_id)
        );
    ")
}

/// v12 — per-(card, tag) ordering, so cards sharing a tag can be arranged
/// into a progression (lesson 1, 2, 3…) instead of only alphabetical order.
fn migrate_v12(conn: &Connection) -> Result<(), rusqlite::Error> {
    let _ = conn.execute_batch("ALTER TABLE card_tag ADD COLUMN position INTEGER NOT NULL DEFAULT 0;");
    Ok(())
}

/// v13–v15 created and reshaped the planner tables. The planner feature was
/// removed, so these are now no-ops: fresh databases never create the tables,
/// and databases that already have them are cleaned up by v16. They stay as
/// numbered steps so the version counter keeps its meaning.
fn migrate_v13(_conn: &Connection) -> Result<(), rusqlite::Error> {
    Ok(())
}

fn migrate_v14(_conn: &Connection) -> Result<(), rusqlite::Error> {
    Ok(())
}

fn migrate_v15(_conn: &Connection) -> Result<(), rusqlite::Error> {
    Ok(())
}

/// v16 — drop the planner tables left behind on databases created before the
/// feature was removed. The edge table goes first: it has a foreign key onto
/// `planner_task`.
fn migrate_v16(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch("
        DROP TABLE IF EXISTS planner_task_edge;
        DROP TABLE IF EXISTS planner_task;
    ")
}

/// v17 — drop the deck columns left over from removed features: the per-deck
/// session cap (study sessions) and the cached AI analysis text. Tolerated
/// rather than required — a column is absent on databases created after the
/// earlier migrations stopped adding it, and DROP COLUMN needs SQLite 3.35+.
/// Nothing reads either column in any case.
fn migrate_v17(conn: &Connection) -> Result<(), rusqlite::Error> {
    let _ = conn.execute_batch("ALTER TABLE deck DROP COLUMN session_max_cards;");
    let _ = conn.execute_batch("ALTER TABLE deck DROP COLUMN ai_analysis;");
    Ok(())
}

/// v18 — drops the settings table. Its only key was the Anthropic API key,
/// which the app no longer needs: it exposes its decks over MCP rather than
/// calling any model itself. Dropping it also clears keys already stored.
fn migrate_v18(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch("DROP TABLE IF EXISTS setting;")
}

/// v6 — easy/difficult notes on cards, settings table.
fn migrate_v8(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch("
        CREATE TRIGGER IF NOT EXISTS trg_card_move_count
        AFTER UPDATE OF deck_id ON card
        WHEN OLD.deck_id != NEW.deck_id
        BEGIN
            UPDATE deck SET card_count = card_count - 1 WHERE id = OLD.deck_id;
            UPDATE deck SET card_count = card_count + 1 WHERE id = NEW.deck_id;
        END;
    ")
}

// Added ai_analysis, which the removed in-app AI features used. Now a no-op so
// fresh databases never gain the column; v17 drops it where it already exists.
// Kept as a numbered step so the version counter keeps its meaning.
fn migrate_v7(_conn: &Connection) -> Result<(), rusqlite::Error> {
    Ok(())
}

fn migrate_v6(conn: &Connection) -> Result<(), rusqlite::Error> {
    let _ = conn.execute_batch("ALTER TABLE card ADD COLUMN what_was_easy TEXT;");
    let _ = conn.execute_batch("ALTER TABLE card ADD COLUMN what_was_difficult TEXT;");
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS setting (
            user_id TEXT NOT NULL,
            key     TEXT NOT NULL,
            value   TEXT NOT NULL,
            PRIMARY KEY (user_id, key)
        );
    ")
}

/// v5 — backfill card_count for decks created before triggers existed.
fn migrate_v5(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "UPDATE deck SET card_count = (SELECT COUNT(*) FROM card WHERE card.deck_id = deck.id);",
    )
}

/// v4 — migrate user ids to UUIDs (TEXT) and add per-user deck ownership.
fn migrate_v4(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch("
        CREATE TABLE user_new (
            id            TEXT PRIMARY KEY,
            username      TEXT NOT NULL UNIQUE COLLATE NOCASE,
            password_hash TEXT NOT NULL
        );

        INSERT INTO user_new (id, username, password_hash)
        SELECT
            lower(hex(randomblob(4))) || '-' ||
            lower(hex(randomblob(2))) || '-4' ||
            substr(lower(hex(randomblob(2))), 2) || '-' ||
            substr('89ab', abs(random()) % 4 + 1, 1) ||
            substr(lower(hex(randomblob(2))), 2) || '-' ||
            lower(hex(randomblob(6))),
            username,
            password_hash
        FROM user;

        DROP TABLE user;
        ALTER TABLE user_new RENAME TO user;

        ALTER TABLE deck ADD COLUMN user_id TEXT NOT NULL DEFAULT 'local';

        UPDATE deck SET user_id = 'local';
    ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn migrations_are_idempotent() {
        let conn = in_memory();
        // Running again must not fail
        run_migrations(&conn).unwrap();
    }

    #[test]
    fn card_count_increments_on_insert() {
        let conn = in_memory();
        conn.execute("INSERT INTO deck (name, created_at, card_count) VALUES ('D', 0, 0)", []).unwrap();
        let deck_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO card (deck_id, name, created_at) VALUES (?1, 'C', 0)",
            [deck_id],
        ).unwrap();
        let count: i64 = conn.query_row(
            "SELECT card_count FROM deck WHERE id = ?1", [deck_id], |r| r.get(0)
        ).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn card_count_decrements_on_delete() {
        let conn = in_memory();
        conn.execute("INSERT INTO deck (name, created_at, card_count) VALUES ('D', 0, 0)", []).unwrap();
        let deck_id = conn.last_insert_rowid();
        conn.execute("INSERT INTO card (deck_id, name, created_at) VALUES (?1, 'C', 0)", [deck_id]).unwrap();
        let card_id = conn.last_insert_rowid();
        conn.execute("DELETE FROM card WHERE id = ?1", [card_id]).unwrap();
        let count: i64 = conn.query_row(
            "SELECT card_count FROM deck WHERE id = ?1", [deck_id], |r| r.get(0)
        ).unwrap();
        assert_eq!(count, 0);
    }
}
