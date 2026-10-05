//! The deck import wraps many card saves in one outer transaction. Card
//! saves use savepoints internally, so they must compose with an open
//! BEGIN — and a rollback must undo all of them atomically.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use adapter_sqlite::{register_functions, SharedConnection, SqliteCardRepository, SqliteTransactionScope};
use adapter_sqlite::schema::run_migrations;
use domain_flashcard::Card;
use ports::card_repo::CardRepository;
use ports::transaction::TransactionScope;

fn setup() -> (SqliteCardRepository, SqliteTransactionScope, SharedConnection) {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    register_functions(&conn).unwrap();
    run_migrations(&conn).unwrap();
    conn.execute("INSERT INTO deck (id, name, created_at, card_count) VALUES (1, 'Test', 0, 0)", [])
        .unwrap();
    let shared: SharedConnection = Arc::new(Mutex::new(conn));
    (
        SqliteCardRepository::new(shared.clone(), PathBuf::from("/tmp")),
        SqliteTransactionScope::new(shared.clone()),
        shared,
    )
}

fn card_count(conn: &SharedConnection) -> i64 {
    conn.lock().unwrap()
        .query_row("SELECT COUNT(*) FROM card", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn card_saves_commit_inside_an_outer_transaction() {
    let (repo, tx, conn) = setup();

    tx.begin().unwrap();
    for i in 0..3 {
        let mut card = Card::new(1, &format!("card {i}"), 0).unwrap();
        repo.save(&mut card).unwrap();
    }
    tx.commit().unwrap();

    assert_eq!(card_count(&conn), 3);
}

#[test]
fn rollback_undoes_all_card_saves() {
    let (repo, tx, conn) = setup();

    tx.begin().unwrap();
    for i in 0..3 {
        let mut card = Card::new(1, &format!("card {i}"), 0).unwrap();
        repo.save(&mut card).unwrap();
    }
    tx.rollback().unwrap();

    assert_eq!(card_count(&conn), 0);
}
