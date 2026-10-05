use domain_flashcard::Card;
use domain_tagging::{CardTagInfo, Tag};
use ports::card_repo::CardRepository;
use ports::tag_repo::TagRepository;
use crate::AppError;

/// A tag as it appears in a deck export: identified by name (ids are local
/// to each database) with its knowledge-map canvas position, if placed.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TagMapEntry {
    pub name: String,
    #[serde(default)]
    pub pos_x: Option<f64>,
    #[serde(default)]
    pub pos_y: Option<f64>,
}

/// The slice of the knowledge map relevant to one deck, in a portable form:
/// the deck's tags plus their ancestors, the DAG edges among them, and which
/// of them are assigned to the deck itself.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KnowledgeMapExport {
    pub tags: Vec<TagMapEntry>,
    /// (parent_name, child_name) pairs.
    pub edges: Vec<(String, String)>,
    /// Names of tags assigned to the deck via tag_deck.
    pub deck_tags: Vec<String>,
}

pub struct TaggingService {
    tag_repo:  Box<dyn TagRepository + Send + Sync>,
    card_repo: Box<dyn CardRepository + Send + Sync>,
}

impl TaggingService {
    pub fn new(
        tag_repo:  Box<dyn TagRepository + Send + Sync>,
        card_repo: Box<dyn CardRepository + Send + Sync>,
    ) -> Self {
        Self { tag_repo, card_repo }
    }

    // --- Tag CRUD ---

    pub fn create_tag(&self, name: &str) -> Result<Tag, AppError> {
        Ok(self.tag_repo.create(name)?)
    }

    pub fn get_all_tags(&self) -> Result<Vec<Tag>, AppError> {
        Ok(self.tag_repo.find_all()?)
    }

    pub fn get_tags_in_use(&self) -> Result<Vec<Tag>, AppError> {
        Ok(self.tag_repo.find_in_use()?)
    }

    pub fn rename_tag(&self, tag_id: i64, new_name: &str) -> Result<Tag, AppError> {
        Ok(self.tag_repo.rename(tag_id, new_name)?)
    }

    pub fn delete_tag(&self, tag_id: i64) -> Result<(), AppError> {
        Ok(self.tag_repo.delete(tag_id)?)
    }

    // --- Knowledge map (hierarchy + canvas layout) ---

    /// Link `parent_id` → `child_id` in the knowledge map. A tag may have many
    /// parents (the structure is a DAG). Rejects self-links and any edge that
    /// would create a cycle.
    pub fn add_tag_edge(&self, parent_id: i64, child_id: i64) -> Result<(), AppError> {
        if parent_id == child_id {
            return Err(AppError::ValidationError("a tag cannot be its own parent".into()));
        }
        // A cycle would form if `parent_id` is already reachable as a descendant
        // of `child_id`. Walk down from the child following existing edges.
        let edges = self.tag_repo.all_edges()?;
        let mut stack = vec![child_id];
        let mut seen = std::collections::HashSet::new();
        while let Some(node) = stack.pop() {
            if node == parent_id {
                return Err(AppError::ValidationError("that link would create a cycle".into()));
            }
            for (p, c) in &edges {
                if *p == node && seen.insert(*c) {
                    stack.push(*c);
                }
            }
        }
        Ok(self.tag_repo.add_edge(parent_id, child_id)?)
    }

    pub fn remove_tag_edge(&self, parent_id: i64, child_id: i64) -> Result<(), AppError> {
        Ok(self.tag_repo.remove_edge(parent_id, child_id)?)
    }

    pub fn get_tag_edges(&self) -> Result<Vec<(i64, i64)>, AppError> {
        Ok(self.tag_repo.all_edges()?)
    }

    pub fn assign_tag_to_deck(&self, tag_id: i64, deck_id: i64) -> Result<(), AppError> {
        Ok(self.tag_repo.assign_to_deck(tag_id, deck_id)?)
    }

    pub fn unassign_tag_from_deck(&self, tag_id: i64, deck_id: i64) -> Result<(), AppError> {
        Ok(self.tag_repo.unassign_from_deck(tag_id, deck_id)?)
    }

    pub fn get_tag_deck_pairs(&self) -> Result<Vec<(i64, i64)>, AppError> {
        Ok(self.tag_repo.tag_deck_pairs()?)
    }

    pub fn detach_tag_from_cards(&self, tag_id: i64) -> Result<(), AppError> {
        Ok(self.tag_repo.detach_all_cards(tag_id)?)
    }

    pub fn set_tag_position(&self, tag_id: i64, x: f64, y: f64) -> Result<(), AppError> {
        Ok(self.tag_repo.set_position(tag_id, x, y)?)
    }

    pub fn clear_tag_position(&self, tag_id: i64) -> Result<(), AppError> {
        Ok(self.tag_repo.clear_position(tag_id)?)
    }

    pub fn reset_tag_map(&self) -> Result<(), AppError> {
        Ok(self.tag_repo.reset_map()?)
    }

    // --- Knowledge map export / import ---

    /// Builds the portable knowledge-map slice for a deck: tags used by its
    /// cards or assigned to it, plus all their ancestors in the DAG, and the
    /// edges among that set.
    pub fn export_knowledge_map_for_deck(&self, deck_id: i64) -> Result<KnowledgeMapExport, AppError> {
        let all_tags = self.tag_repo.find_all()?;
        let edges = self.tag_repo.all_edges()?;

        // Seed: tags linked to the deck, both via its cards and via explicit
        // tag_deck assignments (tag_deck_pairs is the union of the two).
        let deck_tag_ids: Vec<i64> = self.tag_repo.tag_deck_pairs()?
            .into_iter()
            .filter(|(_tag_id, d)| *d == deck_id)
            .map(|(tag_id, _d)| tag_id)
            .collect();
        let mut included: std::collections::HashSet<i64> =
            deck_tag_ids.iter().copied().collect();

        // Ancestor closure, so exported tags keep their place in the hierarchy.
        let mut stack: Vec<i64> = included.iter().copied().collect();
        while let Some(node) = stack.pop() {
            for (parent, child) in &edges {
                if *child == node && included.insert(*parent) {
                    stack.push(*parent);
                }
            }
        }

        let name_of: std::collections::HashMap<i64, &str> =
            all_tags.iter().map(|t| (t.id, t.name.as_str())).collect();

        let tags = all_tags.iter()
            .filter(|t| included.contains(&t.id))
            .map(|t| TagMapEntry { name: t.name.clone(), pos_x: t.pos_x, pos_y: t.pos_y })
            .collect();

        let edges = edges.iter()
            .filter(|(p, c)| included.contains(p) && included.contains(c))
            .filter_map(|(p, c)| Some((name_of.get(p)?.to_string(), name_of.get(c)?.to_string())))
            .collect();

        let deck_tags = deck_tag_ids.iter()
            .filter_map(|id| name_of.get(id).map(|n| n.to_string()))
            .collect();

        Ok(KnowledgeMapExport { tags, edges, deck_tags })
    }

    /// Merges an exported knowledge-map slice into this database for a newly
    /// imported deck. Tags are matched by name (case-insensitive): existing
    /// tags keep their canvas position, new tags get the exported one. Edges
    /// go through the cycle check and are skipped if they'd conflict.
    pub fn import_knowledge_map(&self, deck_id: i64, map: &KnowledgeMapExport) -> Result<(), AppError> {
        let existing = self.tag_repo.find_all()?;
        let mut id_by_name: std::collections::HashMap<String, i64> = existing.iter()
            .map(|t| (t.name.to_lowercase(), t.id))
            .collect();
        let unplaced: std::collections::HashSet<i64> = existing.iter()
            .filter(|t| t.pos_x.is_none() || t.pos_y.is_none())
            .map(|t| t.id)
            .collect();

        for entry in &map.tags {
            let name = entry.name.trim();
            if name.is_empty() { continue; }
            let key = name.to_lowercase();
            let (id, is_new) = match id_by_name.get(&key) {
                Some(&id) => (id, false),
                None => {
                    let tag = self.tag_repo.create(name)?;
                    id_by_name.insert(key, tag.id);
                    (tag.id, true)
                }
            };
            // Apply the exported position to new tags, and to existing tags
            // that were never placed — never move tags the user has laid out.
            if is_new || unplaced.contains(&id) {
                if let (Some(x), Some(y)) = (entry.pos_x, entry.pos_y) {
                    self.tag_repo.set_position(id, x, y)?;
                }
            }
        }

        for (parent_name, child_name) in &map.edges {
            let parent = id_by_name.get(&parent_name.trim().to_lowercase());
            let child = id_by_name.get(&child_name.trim().to_lowercase());
            if let (Some(&p), Some(&c)) = (parent, child) {
                // Best effort: skip edges that would create a cycle with the
                // importer's existing graph.
                let _ = self.add_tag_edge(p, c);
            }
        }

        for name in &map.deck_tags {
            if let Some(&id) = id_by_name.get(&name.trim().to_lowercase()) {
                self.tag_repo.assign_to_deck(id, deck_id)?;
            }
        }

        Ok(())
    }

    // --- Card-tag associations ---

    pub fn assign_tag(&self, tag_id: i64, card_id: i64) -> Result<(), AppError> {
        Ok(self.tag_repo.assign_to_card(tag_id, card_id)?)
    }

    pub fn remove_tag(&self, tag_id: i64, card_id: i64) -> Result<(), AppError> {
        Ok(self.tag_repo.remove_from_card(tag_id, card_id)?)
    }

    pub fn tags_for_card(&self, card_id: i64) -> Result<Vec<Tag>, AppError> {
        Ok(self.tag_repo.tags_for_card(card_id)?)
    }

    pub fn tags_for_card_with_position(&self, card_id: i64) -> Result<Vec<CardTagInfo>, AppError> {
        Ok(self.tag_repo.tags_for_card_with_position(card_id)?)
    }

    pub fn set_card_tag_position(&self, tag_id: i64, card_id: i64, position: i64) -> Result<(), AppError> {
        Ok(self.tag_repo.set_card_tag_position(tag_id, card_id, position)?)
    }

    /// (tag_id, card_id, position) for the given tags — lets a card list show
    /// each card's progression number under every selected tag.
    pub fn card_tag_positions(&self, tag_ids: &[i64]) -> Result<Vec<(i64, i64, i64)>, AppError> {
        Ok(self.tag_repo.card_positions_for_tags(tag_ids)?)
    }

    pub fn tags_for_deck(&self, deck_id: i64) -> Result<Vec<Tag>, AppError> {
        Ok(self.tag_repo.tags_for_deck(deck_id)?)
    }

    pub fn card_tag_pairs_for_deck(&self, deck_id: i64) -> Result<Vec<(i64, i64)>, AppError> {
        Ok(self.tag_repo.card_tag_pairs_for_deck(deck_id)?)
    }

    pub fn cards_by_tags(&self, tag_ids: &[i64]) -> Result<Vec<Card>, AppError> {
        Ok(self.card_repo.find_by_tags(tag_ids)?)
    }

    /// Snapshot of all tags as a lowercase-name → id map, for bulk imports
    /// where re-reading the tag table per card would be O(cards × tags).
    pub fn tag_name_cache(&self) -> Result<std::collections::HashMap<String, i64>, AppError> {
        Ok(self.tag_repo.find_all()?
            .into_iter()
            .map(|t| (t.name.to_lowercase(), t.id))
            .collect())
    }

    /// Assigns tags to a freshly created card during import, resolving names
    /// through (and updating) a caller-held cache from `tag_name_cache`.
    /// Unlike `sync_tags_for_card` this never removes tags, so it's only
    /// correct for cards that don't have any yet.
    pub fn assign_tags_cached(
        &self,
        card_id: i64,
        tag_names: &[String],
        cache: &mut std::collections::HashMap<String, i64>,
    ) -> Result<(), AppError> {
        for raw in tag_names {
            let name = raw.trim();
            if name.is_empty() { continue; }
            let key = name.to_lowercase();
            let tag_id = match cache.get(&key) {
                Some(&id) => id,
                None => {
                    let tag = self.tag_repo.create(name)?;
                    cache.insert(key, tag.id);
                    tag.id
                }
            };
            self.tag_repo.assign_to_card(tag_id, card_id)?;
        }
        Ok(())
    }

    /// Sync a card's tags to exactly the given list of names.
    /// Creates tags that don't exist yet (unique by name, case-insensitive).
    /// Assigns new ones and removes dropped ones. Returns the final tag list.
    pub fn sync_tags_for_card(&self, card_id: i64, tag_names: Vec<String>) -> Result<Vec<Tag>, AppError> {
        // Normalize + dedup (preserve order, case-insensitive dedup)
        let mut seen = std::collections::HashSet::new();
        let desired: Vec<String> = tag_names.iter()
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty() && seen.insert(n.to_lowercase()))
            .collect();

        let all_tags = self.tag_repo.find_all()?;
        let current = self.tag_repo.tags_for_card(card_id)?;

        // Find or create a tag for each desired name
        let mut final_tags: Vec<Tag> = Vec::new();
        for name in &desired {
            let tag = match all_tags.iter().find(|t| t.name.eq_ignore_ascii_case(name)) {
                Some(existing) => existing.clone(),
                None => self.tag_repo.create(name)?,
            };
            final_tags.push(tag);
        }

        // Assign new tags
        for tag in &final_tags {
            if !current.iter().any(|t| t.id == tag.id) {
                self.tag_repo.assign_to_card(tag.id, card_id)?;
            }
        }

        // Remove tags no longer in the list
        for tag in &current {
            if !final_tags.iter().any(|t| t.id == tag.id) {
                self.tag_repo.remove_from_card(tag.id, card_id)?;
            }
        }

        Ok(final_tags)
    }

    /// Returns all cards that have the given tag assigned,
    /// with their blocks populated for full display.
    pub fn filter_cards_by_tag(&self, tag_id: i64) -> Result<Vec<Card>, AppError> {
        let card_ids = self.tag_repo.card_ids_for_tag(tag_id)?;

        let mut cards = Vec::with_capacity(card_ids.len());
        for id in card_ids {
            if let Some(card) = self.card_repo.find_by_id(id)? {
                cards.push(card);
            }
        }
        Ok(cards)
    }
}
