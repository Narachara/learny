use domain_tagging::{CardTagInfo, Tag};
use crate::RepositoryError;

/// Outbound port for tag persistence and card-tag associations.
pub trait TagRepository {
    // --- Tag CRUD ---

    /// Create a new tag. Returns DuplicateName if the name already exists.
    fn create(&self, name: &str) -> Result<Tag, RepositoryError>;

    /// Load a tag by id. Returns None if not found.
    fn find_by_id(&self, id: i64) -> Result<Option<Tag>, RepositoryError>;

    /// Load a tag by exact name. Returns None if not found.
    fn find_by_name(&self, name: &str) -> Result<Option<Tag>, RepositoryError>;

    /// Load all tags ordered by name.
    fn find_all(&self) -> Result<Vec<Tag>, RepositoryError>;

    /// Load only tags that are assigned to at least one card.
    fn find_in_use(&self) -> Result<Vec<Tag>, RepositoryError>;

    /// Rename a tag globally. Returns DuplicateName if the new name is taken.
    fn rename(&self, id: i64, new_name: &str) -> Result<Tag, RepositoryError>;

    /// Delete a tag and remove it from all cards.
    fn delete(&self, id: i64) -> Result<(), RepositoryError>;

    // --- Knowledge map (hierarchy + canvas layout) ---

    /// Add a parent → child link. No-ops if it already exists.
    fn add_edge(&self, parent_id: i64, child_id: i64) -> Result<(), RepositoryError>;

    /// Remove a specific parent → child link. No-ops if absent.
    fn remove_edge(&self, parent_id: i64, child_id: i64) -> Result<(), RepositoryError>;

    /// All knowledge-map edges as (parent_id, child_id) pairs.
    fn all_edges(&self) -> Result<Vec<(i64, i64)>, RepositoryError>;

    /// Explicitly assign a tag to a deck (independent of cards). No-ops if set.
    fn assign_to_deck(&self, tag_id: i64, deck_id: i64) -> Result<(), RepositoryError>;

    /// Remove an explicit tag → deck assignment. No-ops if absent.
    fn unassign_from_deck(&self, tag_id: i64, deck_id: i64) -> Result<(), RepositoryError>;

    /// All (tag_id, deck_id) memberships — both derived from cards and explicit
    /// assignments — for building the map's deck filter.
    fn tag_deck_pairs(&self) -> Result<Vec<(i64, i64)>, RepositoryError>;

    /// Persist a tag's position on the knowledge-map canvas.
    fn set_position(&self, tag_id: i64, x: f64, y: f64) -> Result<(), RepositoryError>;

    /// Clear a tag's canvas position (returns it to the unplaced sidebar).
    fn clear_position(&self, tag_id: i64) -> Result<(), RepositoryError>;

    /// Reset the whole knowledge map: clear every tag's position and parent
    /// link. Tags themselves are untouched.
    fn reset_map(&self) -> Result<(), RepositoryError>;

    // --- Card-tag associations ---

    /// Assign a tag to a card. No-ops if already assigned.
    fn assign_to_card(&self, tag_id: i64, card_id: i64) -> Result<(), RepositoryError>;

    /// Remove a tag from a card. No-ops if not assigned.
    fn remove_from_card(&self, tag_id: i64, card_id: i64) -> Result<(), RepositoryError>;

    /// Remove a tag from every card it's on (keeps the tag itself).
    fn detach_all_cards(&self, tag_id: i64) -> Result<(), RepositoryError>;

    /// All tags assigned to a specific card.
    fn tags_for_card(&self, card_id: i64) -> Result<Vec<Tag>, RepositoryError>;

    /// All tags assigned to a specific card, each with its position within
    /// that tag's progression (see `set_card_tag_position`).
    fn tags_for_card_with_position(&self, card_id: i64) -> Result<Vec<CardTagInfo>, RepositoryError>;

    /// Set a card's position within one tag's progression. Cards with a
    /// lower position sort earlier wherever that tag's cards are listed.
    fn set_card_tag_position(&self, tag_id: i64, card_id: i64, position: i64) -> Result<(), RepositoryError>;

    /// Every (tag_id, card_id, position) triple for the given tags, so a list
    /// of cards can be labelled with its progression number under each tag in
    /// one round-trip.
    fn card_positions_for_tags(&self, tag_ids: &[i64]) -> Result<Vec<(i64, i64, i64)>, RepositoryError>;

    /// All card ids that have a specific tag (used for filtering).
    fn card_ids_for_tag(&self, tag_id: i64) -> Result<Vec<i64>, RepositoryError>;

    /// All distinct tags used by any card in the given deck.
    fn tags_for_deck(&self, deck_id: i64) -> Result<Vec<Tag>, RepositoryError>;

    /// All (card_id, tag_id) pairs for cards in the given deck.
    /// Used to build a client-side filter map in one round-trip.
    fn card_tag_pairs_for_deck(&self, deck_id: i64) -> Result<Vec<(i64, i64)>, RepositoryError>;
}
