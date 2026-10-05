use domain_flashcard::Deck;
use crate::RepositoryError;

/// Outbound port for deck persistence.
pub trait DeckRepository {
    /// Load a single deck by id. Returns None if not found.
    fn find_by_id(&self, id: i64) -> Result<Option<Deck>, RepositoryError>;

    /// Load all decks for a user, ordered by creation date descending.
    fn find_all_for_user(&self, user_id: &str) -> Result<Vec<Deck>, RepositoryError>;

    /// Persist a deck.
    /// - If deck.id == -1: inserts and sets deck.id to the new id.
    /// - Otherwise: updates the name.
    fn save(&self, deck: &mut Deck) -> Result<(), RepositoryError>;

    /// Delete a deck and all its cards (cascades).
    fn delete(&self, id: i64) -> Result<(), RepositoryError>;

    /// Set or clear the cover image virtual path for a deck.
    /// Deletes the previous file if one existed.
    fn set_cover_image(&self, deck_id: i64, path: Option<String>) -> Result<(), RepositoryError>;

}
