//! Deck export/import: the portable .zip archive format shared by every host.
//!
//! Archive layout (export_version 3):
//!   export.json                    — DeckExport manifest (deflated)
//!   deck_cover.<ext>               — optional deck cover (stored)
//!   files/card_<i>/cover.<ext>     — optional card covers (stored)
//!   files/card_<i>/<side>_<j>.<ext> — media referenced by card blocks (stored)
//!
//! Hosts only provide transport: where the bytes come from / go to.

use std::collections::HashMap;
use std::io::{Read, Seek, Write};
use std::path::Path;

use domain_flashcard::{Card, Deck};
use ports::transaction::TransactionScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zip::{write::FileOptions, ZipArchive, ZipWriter};

use crate::flashcard_service::FlashcardService;
use crate::tagging_service::{KnowledgeMapExport, TaggingService};
use crate::AppError;

const EXPORT_VERSION: u32 = 3;
const SUPPORTED_VERSIONS: std::ops::RangeInclusive<u32> = 2..=3;

#[derive(Debug, Serialize, Deserialize)]
struct DeckExport {
    export_version: u32,
    deck: Deck,
    cards: Vec<Card>,
    /// Tag names per card, parallel to `cards`. Absent in old exports.
    #[serde(default)]
    card_tags: Vec<Vec<String>>,
    /// The deck's slice of the knowledge map. Absent in v2 and older exports.
    #[serde(default)]
    knowledge_map: Option<KnowledgeMapExport>,
}

fn io_err(e: impl std::fmt::Display) -> AppError {
    AppError::Io(e.to_string())
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// Export a deck as a zip archive into `writer`.
/// Media files are read from `data_dir` and copied through in chunks, so the
/// peak memory footprint stays flat no matter how much media the deck carries.
pub fn export_deck<W: Write + Seek>(
    flashcard: &FlashcardService,
    tagging: &TaggingService,
    deck_id: i64,
    data_dir: &Path,
    writer: W,
) -> Result<(), AppError> {
    let deck = flashcard.get_deck(deck_id)?;
    let cards = flashcard.get_cards_full(deck_id)?;

    // Two queries instead of one per card: all pairs for the deck, then
    // tag names resolved through one snapshot of the tag table.
    let tag_names: HashMap<i64, String> = tagging
        .get_all_tags()
        .unwrap_or_default()
        .into_iter()
        .map(|t| (t.id, t.name))
        .collect();
    let mut tags_by_card: HashMap<i64, Vec<String>> = HashMap::new();
    for (card_id, tag_id) in tagging.card_tag_pairs_for_deck(deck_id).unwrap_or_default() {
        if let Some(name) = tag_names.get(&tag_id) {
            tags_by_card.entry(card_id).or_default().push(name.clone());
        }
    }
    let card_tags: Vec<Vec<String>> = cards
        .iter()
        .map(|card| tags_by_card.remove(&card.id).unwrap_or_default())
        .collect();
    let knowledge_map = tagging.export_knowledge_map_for_deck(deck_id).ok();

    // Collect (virtual_path, zip_path) for every media file the deck references.
    let mut export_files: Vec<(String, String)> = Vec::new();
    if let Some(ref src) = deck.cover_image {
        export_files.push((src.clone(), format!("deck_cover.{}", file_ext(src))));
    }
    for (ci, card) in cards.iter().enumerate() {
        if let Some(ref src) = card.cover_image {
            export_files.push((src.clone(), format!("files/card_{}/cover.{}", ci, file_ext(src))));
        }
        for (bi, block) in card.front_blocks.iter().enumerate() {
            if let Some(src) = block.file_path() {
                export_files.push((src.to_string(), format!("files/card_{}/front_{}.{}", ci, bi, file_ext(src))));
            }
        }
        for (bi, block) in card.back_blocks.iter().enumerate() {
            if let Some(src) = block.file_path() {
                export_files.push((src.to_string(), format!("files/card_{}/back_{}.{}", ci, bi, file_ext(src))));
            }
        }
    }

    let path_map: HashMap<String, String> = export_files.iter().cloned().collect();

    // Rewrite covers + block paths so export.json references archive paths.
    let mut exported_deck = deck;
    rewrite_path(&mut exported_deck.cover_image, &path_map);
    let mut exported_cards = cards;
    for card in exported_cards.iter_mut() {
        rewrite_path(&mut card.cover_image, &path_map);
        for block in card.all_blocks_mut() {
            if let Some(p) = block.file_path_mut() {
                if let Some(new) = path_map.get(p) { *p = new.clone(); }
            }
        }
    }

    let export = DeckExport {
        export_version: EXPORT_VERSION,
        deck: exported_deck,
        cards: exported_cards,
        card_tags,
        knowledge_map,
    };
    let json = serde_json::to_string_pretty(&export).map_err(io_err)?;

    let mut zip = ZipWriter::new(writer);
    let deflated: FileOptions<()> = FileOptions::default();
    // Media (images/audio/video) is already compressed — deflating it again
    // burns CPU for ~no size gain, so store those entries as-is.
    let stored: FileOptions<()> = FileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);

    zip.start_file("export.json", deflated).map_err(io_err)?;
    zip.write_all(json.as_bytes()).map_err(io_err)?;

    for (virtual_path, zip_path) in &export_files {
        let src_path = data_dir.join(virtual_path);
        let mut reader = std::fs::File::open(&src_path)
            .map_err(|e| AppError::Io(format!("read {:?}: {}", src_path, e)))?;
        zip.start_file(zip_path, stored).map_err(io_err)?;
        std::io::copy(&mut reader, &mut zip).map_err(io_err)?;
    }

    zip.finish().map_err(io_err)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

/// Import a deck archive: extract media into `data_dir/files` under fresh
/// UUID names, then restore the deck, cards, tags, and knowledge-map slice
/// inside one transaction.
pub fn import_deck(
    flashcard: &FlashcardService,
    tagging: &TaggingService,
    tx: &dyn TransactionScope,
    data_dir: &Path,
    zip_bytes: &[u8],
) -> Result<i64, AppError> {
    let export = extract_archive(data_dir, zip_bytes)?;

    // One transaction for the whole import: hundreds of inserts share a
    // single commit, and a failure rolls the deck back atomically.
    tx.begin()?;
    match restore_export(flashcard, tagging, export) {
        Ok(deck_id) => {
            tx.commit()?;
            Ok(deck_id)
        }
        Err(e) => {
            let _ = tx.rollback();
            Err(e)
        }
    }
}

/// Read export.json, copy media entries into `data_dir/files` under fresh
/// UUID names, and rewrite all paths in the manifest accordingly.
fn extract_archive(data_dir: &Path, zip_bytes: &[u8]) -> Result<DeckExport, AppError> {
    let mut zip = ZipArchive::new(std::io::Cursor::new(zip_bytes)).map_err(io_err)?;

    let mut json = String::new();
    zip.by_name("export.json")
        .map_err(|_| AppError::ValidationError("export.json not found in archive".into()))?
        .read_to_string(&mut json)
        .map_err(io_err)?;

    let mut export: DeckExport = serde_json::from_str(&json)
        .map_err(|e| AppError::ValidationError(format!("invalid export.json: {}", e)))?;
    if !SUPPORTED_VERSIONS.contains(&export.export_version) {
        return Err(AppError::ValidationError(format!(
            "unsupported export version {}",
            export.export_version
        )));
    }

    let files_dir = data_dir.join("files");
    std::fs::create_dir_all(&files_dir).map_err(io_err)?;

    let mut path_map: HashMap<String, String> = HashMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(io_err)?;
        let zip_path = entry.name().to_string();
        if !zip_path.starts_with("files/") && !zip_path.starts_with("deck_cover") {
            continue;
        }

        let new_name = format!("{}.{}", Uuid::new_v4(), file_ext(&zip_path));
        let dest = files_dir.join(&new_name);
        let mut out = std::fs::File::create(&dest).map_err(io_err)?;
        std::io::copy(&mut entry, &mut out).map_err(io_err)?;
        path_map.insert(zip_path, format!("files/{}", new_name));
    }

    rewrite_path(&mut export.deck.cover_image, &path_map);
    for card in export.cards.iter_mut() {
        rewrite_path(&mut card.cover_image, &path_map);
        for block in card.all_blocks_mut() {
            if let Some(p) = block.file_path_mut() {
                if let Some(new) = path_map.get(p) { *p = new.clone(); }
            }
        }
    }

    Ok(export)
}

/// Restore an extracted export into the database (no transaction handling —
/// `import_deck` wraps this).
fn restore_export(
    flashcard: &FlashcardService,
    tagging: &TaggingService,
    export: DeckExport,
) -> Result<i64, AppError> {
    let deck = flashcard.create_deck(&export.deck.name)?;

    // Restore deck cover (create_deck doesn't set it).
    if export.deck.cover_image.is_some() {
        flashcard.set_deck_cover(deck.id, export.deck.cover_image)?;
    }

    // Merge the knowledge map before restoring cards, so tags created here
    // (with edges and positions) are reused by the card-tag assignments below.
    if let Some(ref map) = export.knowledge_map {
        tagging.import_knowledge_map(deck.id, map)?;
    }

    // Resolve tag names through one snapshot instead of per-card lookups.
    let mut tag_cache = tagging.tag_name_cache()?;

    for (i, card) in export.cards.into_iter().enumerate() {
        // Save cover before restore_card clears it.
        let cover = card.cover_image.clone();
        let restored = flashcard.restore_card(deck.id, card)?;
        if cover.is_some() {
            flashcard.set_card_cover(restored.id, cover)?;
        }
        if let Some(tag_names) = export.card_tags.get(i) {
            let _ = tagging.assign_tags_cached(restored.id, tag_names, &mut tag_cache);
        }
    }

    Ok(deck.id)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn file_ext(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or("bin")
}

fn rewrite_path(slot: &mut Option<String>, path_map: &HashMap<String, String>) {
    if let Some(p) = slot {
        if let Some(new) = path_map.get(p) {
            *p = new.clone();
        }
    }
}
