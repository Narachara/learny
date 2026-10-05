//! Round-trip test for the deck knowledge-map export/import (export v3):
//! exporting from one database and merging into another must preserve the
//! deck's tag subgraph without duplicating or disturbing existing tags.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use adapter_sqlite::{register_functions, SharedConnection, SqliteCardRepository, SqliteTagRepository};
use adapter_sqlite::schema::run_migrations;
use application::tagging_service::TaggingService;

fn setup() -> (TaggingService, SharedConnection) {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    register_functions(&conn).unwrap();
    run_migrations(&conn).unwrap();
    let shared: SharedConnection = Arc::new(Mutex::new(conn));
    let service = TaggingService::new(
        Box::new(SqliteTagRepository::new(shared.clone())),
        Box::new(SqliteCardRepository::new(shared.clone(), PathBuf::from("/tmp"))),
    );
    (service, shared)
}

fn insert_deck(conn: &SharedConnection, id: i64, name: &str) {
    conn.lock().unwrap()
        .execute(
            "INSERT INTO deck (id, name, created_at, card_count) VALUES (?1, ?2, 0, 0)",
            rusqlite::params![id, name],
        )
        .unwrap();
}

fn insert_card(conn: &SharedConnection, id: i64, deck_id: i64) {
    conn.lock().unwrap()
        .execute(
            "INSERT INTO card (id, deck_id, name, created_at) VALUES (?1, ?2, 'card', 0)",
            rusqlite::params![id, deck_id],
        )
        .unwrap();
}

#[test]
fn export_includes_deck_tags_ancestors_and_positions() {
    let (svc, conn) = setup();
    insert_deck(&conn, 1, "Deck A");
    insert_card(&conn, 1, 1);

    let math = svc.create_tag("Math").unwrap();
    let calculus = svc.create_tag("Calculus").unwrap();
    let algebra = svc.create_tag("Algebra").unwrap();
    let physics = svc.create_tag("Physics").unwrap(); // unrelated, must not export

    svc.add_tag_edge(math.id, calculus.id).unwrap();
    svc.add_tag_edge(math.id, algebra.id).unwrap();
    svc.set_tag_position(math.id, 10.0, 20.0).unwrap();
    svc.set_tag_position(calculus.id, 30.0, 40.0).unwrap();
    svc.set_tag_position(physics.id, 99.0, 99.0).unwrap();

    // Deck 1 touches Calculus via a card and Algebra via a direct assignment.
    svc.assign_tag(calculus.id, 1).unwrap();
    svc.assign_tag_to_deck(algebra.id, 1).unwrap();

    let map = svc.export_knowledge_map_for_deck(1).unwrap();

    let mut names: Vec<&str> = map.tags.iter().map(|t| t.name.as_str()).collect();
    names.sort();
    // Math comes in as an ancestor; Physics stays out.
    assert_eq!(names, vec!["Algebra", "Calculus", "Math"]);

    let math_entry = map.tags.iter().find(|t| t.name == "Math").unwrap();
    assert_eq!((math_entry.pos_x, math_entry.pos_y), (Some(10.0), Some(20.0)));

    let mut edges = map.edges.clone();
    edges.sort();
    assert_eq!(edges, vec![
        ("Math".to_string(), "Algebra".to_string()),
        ("Math".to_string(), "Calculus".to_string()),
    ]);

    // Both the explicit assignment (Algebra) and the card-derived link
    // (Calculus) count as deck tags; the ancestor Math does not.
    let mut deck_tags = map.deck_tags.clone();
    deck_tags.sort();
    assert_eq!(deck_tags, vec!["Algebra".to_string(), "Calculus".to_string()]);
}

#[test]
fn import_merges_by_name_and_keeps_existing_layout() {
    // Source database.
    let (src, src_conn) = setup();
    insert_deck(&src_conn, 1, "Deck A");
    insert_card(&src_conn, 1, 1);
    let math = src.create_tag("Math").unwrap();
    let calculus = src.create_tag("Calculus").unwrap();
    src.add_tag_edge(math.id, calculus.id).unwrap();
    src.set_tag_position(math.id, 10.0, 20.0).unwrap();
    src.set_tag_position(calculus.id, 30.0, 40.0).unwrap();
    src.assign_tag(calculus.id, 1).unwrap();
    src.assign_tag_to_deck(calculus.id, 1).unwrap();

    let map = src.export_knowledge_map_for_deck(1).unwrap();

    // Destination database: "math" already exists (different case) and is
    // already placed on the canvas.
    let (dst, dst_conn) = setup();
    insert_deck(&dst_conn, 5, "Imported deck");
    let existing_math = dst.create_tag("math").unwrap();
    dst.set_tag_position(existing_math.id, 500.0, 600.0).unwrap();

    dst.import_knowledge_map(5, &map).unwrap();

    let tags = dst.get_all_tags().unwrap();
    // Matched by name: no duplicate Math tag.
    assert_eq!(tags.len(), 2);

    let math_after = tags.iter().find(|t| t.name == "math").unwrap();
    // Existing tag keeps the user's layout, not the imported position.
    assert_eq!((math_after.pos_x, math_after.pos_y), (Some(500.0), Some(600.0)));

    let calc_after = tags.iter().find(|t| t.name == "Calculus").unwrap();
    // New tag arrives with its exported position.
    assert_eq!((calc_after.pos_x, calc_after.pos_y), (Some(30.0), Some(40.0)));

    let edges = dst.get_tag_edges().unwrap();
    assert_eq!(edges, vec![(math_after.id, calc_after.id)]);

    let deck_pairs = dst.get_tag_deck_pairs().unwrap();
    assert!(deck_pairs.contains(&(calc_after.id, 5)));
}

#[test]
fn import_skips_edges_that_would_create_a_cycle() {
    let (src, src_conn) = setup();
    insert_deck(&src_conn, 1, "Deck A");
    insert_card(&src_conn, 1, 1);
    let a = src.create_tag("A").unwrap();
    let b = src.create_tag("B").unwrap();
    src.add_tag_edge(a.id, b.id).unwrap();
    src.assign_tag(b.id, 1).unwrap();
    let map = src.export_knowledge_map_for_deck(1).unwrap();

    // Destination already links B → A, so importing A → B would be a cycle.
    let (dst, dst_conn) = setup();
    insert_deck(&dst_conn, 5, "Imported deck");
    let a2 = dst.create_tag("A").unwrap();
    let b2 = dst.create_tag("B").unwrap();
    dst.add_tag_edge(b2.id, a2.id).unwrap();

    // Must succeed, silently skipping the conflicting edge.
    dst.import_knowledge_map(5, &map).unwrap();

    let edges = dst.get_tag_edges().unwrap();
    assert_eq!(edges, vec![(b2.id, a2.id)]);
}
