use domain_flashcard::Card;
use crate::RepositoryError;

/// Outbound port for card persistence.
/// The SQLite adapter implements this. Tests use a fake in-memory impl.
pub trait CardRepository {
    /// Load a card with its blocks. Returns None if not found.
    fn find_by_id(&self, id: i64) -> Result<Option<Card>, RepositoryError>;

    /// Load all cards in a deck. Blocks are NOT populated (list view only).
    fn find_by_deck(&self, deck_id: i64) -> Result<Vec<Card>, RepositoryError>;

    /// Load all cards in a deck with blocks populated (export / full-load).
    fn find_by_deck_full(&self, deck_id: i64) -> Result<Vec<Card>, RepositoryError>;

    /// Persist a card and its blocks.
    /// - If card.id == -1: inserts and returns the new id.
    /// - Otherwise: updates name and replaces all blocks.
    fn save(&self, card: &mut Card) -> Result<(), RepositoryError>;

    /// Delete a card, its blocks, and any referenced files.
    fn delete(&self, id: i64) -> Result<(), RepositoryError>;

    /// Record the outcome of a study answer and return the updated card.
    fn record_answer(&self, card_id: i64, correct: bool) -> Result<Card, RepositoryError>;

    /// Set or clear the cover image virtual path for a card.
    /// Deletes the previous file if one existed.
    fn set_cover_image(&self, card_id: i64, path: Option<String>) -> Result<(), RepositoryError>;

    /// Persist what_was_easy and what_was_difficult notes for a card.
    fn save_notes(&self, card_id: i64, easy: Option<String>, difficult: Option<String>) -> Result<(), RepositoryError>;

    /// Move a batch of cards to a different deck.
    fn move_to_deck(&self, card_ids: &[i64], target_deck_id: i64) -> Result<(), RepositoryError>;

    /// Reset times_seen and times_correct to 0 for all cards in a deck.
    fn reset_progress(&self, deck_id: i64) -> Result<(), RepositoryError>;

    /// Load cards in a deck that have at least one of the given tags assigned.
    /// Blocks are NOT populated (list view / session start).
    fn find_by_deck_with_tags(&self, deck_id: i64, tag_ids: &[i64]) -> Result<Vec<Card>, RepositoryError>;

    /// Load cards across ALL decks that have at least one of the given tags.
    /// Blocks are NOT populated (list view only).
    fn find_by_tags(&self, tag_ids: &[i64]) -> Result<Vec<Card>, RepositoryError>;

    /// Full-text search across card names and block text content.
    /// Returns matching cards with blocks populated.
    fn search(&self, query: &str) -> Result<Vec<Card>, RepositoryError>;
}
