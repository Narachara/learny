use serde::{Deserialize, Serialize};

/// A content block on one face of a card.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type")]
pub enum Block {
    Text  { value: String },
    /// `scale` is the display width as a percentage of the block's container
    /// (25/50/75/…); `None` means full width. Absent in cards saved before
    /// image scaling existed, hence the serde default.
    Image {
        src: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scale: Option<u8>,
    },
    Audio { src: String },
    Video { src: String },
    File  { path: String },
}

impl Block {
    pub fn block_type(&self) -> &'static str {
        match self {
            Block::Text  { .. } => "text",
            Block::Image { .. } => "image",
            Block::Audio { .. } => "audio",
            Block::Video { .. } => "video",
            Block::File  { .. } => "file",
        }
    }

    /// Returns the stored file path for blocks that own a file, None otherwise.
    pub fn file_path(&self) -> Option<&str> {
        match self {
            Block::Image { src, .. }
            | Block::Audio { src }
            | Block::Video { src } => Some(src),
            Block::File  { path } => Some(path),
            _ => None,
        }
    }

    pub fn file_path_mut(&mut self) -> Option<&mut String> {
        match self {
            Block::Image { src, .. }
            | Block::Audio { src }
            | Block::Video { src } => Some(src),
            Block::File  { path } => Some(path),
            _ => None,
        }
    }
}

/// A flashcard belonging to a deck.
/// Tags are a separate domain (domain_tagging) and are not stored here.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Card {
    pub id: i64,
    pub deck_id: i64,
    pub name: String,
    pub front_blocks: Vec<Block>,
    pub back_blocks: Vec<Block>,
    pub created_at: i64,
    pub times_seen: u32,
    pub times_correct: u32,
    /// Virtual path to a cover image (e.g. "files/uuid.png"), if one has been set.
    pub cover_image: Option<String>,
    /// User's free-text note about what they found easy on this card.
    pub what_was_easy: Option<String>,
    /// User's free-text note about what they found difficult on this card.
    pub what_was_difficult: Option<String>,
}

impl Card {
    /// Creates a new card. Returns an error if the name is blank.
    pub fn new(deck_id: i64, name: impl Into<String>, created_at: i64) -> Result<Self, CardError> {
        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err(CardError::EmptyName);
        }
        Ok(Self {
            id: -1,
            deck_id,
            name,
            front_blocks: vec![],
            back_blocks: vec![],
            created_at,
            times_seen: 0,
            times_correct: 0,
            cover_image: None,
            what_was_easy: None,
            what_was_difficult: None,
        })
    }

    /// UI convenience: creates a blank placeholder card before it is persisted.
    /// The real `created_at` and `id` are assigned by the backend on insert.
    pub fn new_empty(deck_id: i64) -> Self {
        Self {
            id: -1,
            deck_id,
            name: "New Card".into(),
            front_blocks: vec![],
            back_blocks: vec![],
            created_at: 0,
            times_seen: 0,
            times_correct: 0,
            cover_image: None,
            what_was_easy: None,
            what_was_difficult: None,
        }
    }

    /// Returns a 0–100 confidence score based on study history.
    pub fn progress_percent(&self) -> u8 {
        if self.times_seen == 0 {
            return 0;
        }
        let pct = self.times_correct as f64 / self.times_seen as f64 * 100.0;
        pct.round().clamp(0.0, 100.0) as u8
    }

    pub fn all_blocks(&self) -> impl Iterator<Item = &Block> {
        self.front_blocks.iter().chain(self.back_blocks.iter())
    }

    pub fn all_blocks_mut(&mut self) -> impl Iterator<Item = &mut Block> {
        self.front_blocks.iter_mut().chain(self.back_blocks.iter_mut())
    }
}

#[derive(Debug)]
pub enum CardError {
    EmptyName,
}

/// A deck — a named collection of cards.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Deck {
    pub id: i64,
    pub name: String,
    pub created_at: i64,
    pub card_count: u32,
    /// Virtual path to a cover image (e.g. "files/uuid.png"), if one has been set.
    pub cover_image: Option<String>,
    /// Owner user id — never sent to clients.
    #[serde(default, skip_serializing)]
    pub user_id: String,
}

impl Deck {
    /// Creates a new deck. Returns an error if the name is blank.
    pub fn new(name: impl Into<String>, created_at: i64) -> Result<Self, DeckError> {
        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err(DeckError::EmptyName);
        }
        Ok(Self {
            id: -1,
            name,
            created_at,
            card_count: 0,
            cover_image: None,
            user_id: String::new(),
        })
    }
}

#[derive(Debug)]
pub enum DeckError {
    EmptyName,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_name_cannot_be_empty() {
        assert!(Card::new(1, "", 0).is_err());
    }

    #[test]
    fn card_name_is_trimmed() {
        let card = Card::new(1, "  Derivatives  ", 0).unwrap();
        assert_eq!(card.name, "Derivatives");
    }

    #[test]
    fn deck_name_cannot_be_empty() {
        assert!(Deck::new("", 0).is_err());
    }

    #[test]
    fn card_blocks_iterate_front_then_back() {
        let mut card = Card::new(1, "Test", 0).unwrap();
        card.front_blocks.push(Block::Text { value: "front".into() });
        card.back_blocks.push(Block::Text { value: "back".into() });
        let values: Vec<&str> = card
            .all_blocks()
            .filter_map(|b| if let Block::Text { value } = b { Some(value.as_str()) } else { None })
            .collect();
        assert_eq!(values, ["front", "back"]);
    }
}
