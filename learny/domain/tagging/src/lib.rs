/// A tag — a first-class label that can be assigned to many cards.
/// Stored in its own table with a card_tag join table.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    /// Persisted x position on the knowledge-map canvas. `None` until placed.
    #[serde(default)]
    pub pos_x: Option<f64>,
    /// Persisted y position on the knowledge-map canvas. `None` until placed.
    #[serde(default)]
    pub pos_y: Option<f64>,
}

impl Tag {
    /// Creates a new tag. Returns an error if the name is blank.
    pub fn new(name: impl Into<String>) -> Result<Self, TagError> {
        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err(TagError::EmptyName);
        }
        Ok(Self { id: -1, name, pos_x: None, pos_y: None })
    }
}

/// A tag as assigned to a specific card, carrying that pair's position — used
/// to build progressions (card 1, 2, 3… for a given tag).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CardTagInfo {
    pub tag: Tag,
    pub position: i64,
}

#[derive(Debug)]
pub enum TagError {
    EmptyName,
    DuplicateName,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_name_cannot_be_empty() {
        assert!(Tag::new("").is_err());
    }

    #[test]
    fn tag_name_is_trimmed() {
        let tag = Tag::new("  algebra  ").unwrap();
        assert_eq!(tag.name, "algebra");
    }

    #[test]
    fn tag_name_with_spaces_is_valid() {
        let tag = Tag::new("Linear Algebra").unwrap();
        assert_eq!(tag.name, "Linear Algebra");
    }
}
