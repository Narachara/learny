/// A card with progress >= this threshold is considered mastered.
pub const MASTERY_THRESHOLD: u8 = 80;

/// Progress snapshot for a single card — a read model for the dashboard.
/// Computed from study history, not persisted directly.
#[derive(Debug, Clone)]
pub struct CardProgress {
    pub card_id: i64,
    pub card_name: String,
    pub deck_id: i64,
    pub times_seen: u32,
    pub times_correct: u32,
    pub progress_percent: u8,
}

impl CardProgress {
    pub fn is_mastered(&self) -> bool {
        self.progress_percent >= MASTERY_THRESHOLD
    }

    pub fn has_been_studied(&self) -> bool {
        self.times_seen > 0
    }
}

/// Aggregate statistics for a deck — computed from all its CardProgress entries.
#[derive(Debug, Clone)]
pub struct DeckStats {
    pub deck_id: i64,
    pub deck_name: String,
    pub total_cards: u32,
    pub studied_cards: u32,  // times_seen > 0
    pub mastered_cards: u32, // progress_percent >= MASTERY_THRESHOLD
    pub average_progress: u8,
}

impl DeckStats {
    /// Builds DeckStats by aggregating a slice of CardProgress entries.
    /// All entries must belong to the same deck.
    pub fn from_cards(deck_id: i64, deck_name: String, cards: &[CardProgress]) -> Self {
        let total = cards.len() as u32;
        let studied = cards.iter().filter(|c| c.has_been_studied()).count() as u32;
        let mastered = cards.iter().filter(|c| c.is_mastered()).count() as u32;

        let average = if total == 0 {
            0
        } else {
            let sum: u32 = cards.iter().map(|c| c.progress_percent as u32).sum();
            (sum / total).min(100) as u8
        };

        Self {
            deck_id,
            deck_name,
            total_cards: total,
            studied_cards: studied,
            mastered_cards: mastered,
            average_progress: average,
        }
    }

    /// Fraction of cards that have been studied at least once (0.0–1.0).
    pub fn study_rate(&self) -> f32 {
        if self.total_cards == 0 {
            return 0.0;
        }
        self.studied_cards as f32 / self.total_cards as f32
    }

    /// Fraction of cards that are mastered (0.0–1.0).
    pub fn mastery_rate(&self) -> f32 {
        if self.total_cards == 0 {
            return 0.0;
        }
        self.mastered_cards as f32 / self.total_cards as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_card(id: i64, progress: u8) -> CardProgress {
        CardProgress {
            card_id: id,
            card_name: format!("Card {}", id),
            deck_id: 1,
            times_seen: if progress > 0 { 5 } else { 0 },
            times_correct: 0,
            progress_percent: progress,
        }
    }

    #[test]
    fn empty_deck_has_zero_stats() {
        let stats = DeckStats::from_cards(1, "Empty".into(), &[]);
        assert_eq!(stats.total_cards, 0);
        assert_eq!(stats.average_progress, 0);
        assert_eq!(stats.study_rate(), 0.0);
        assert_eq!(stats.mastery_rate(), 0.0);
    }

    #[test]
    fn average_progress_is_computed_correctly() {
        let cards = vec![make_card(1, 40), make_card(2, 60)];
        let stats = DeckStats::from_cards(1, "Test".into(), &cards);
        assert_eq!(stats.average_progress, 50);
    }

    #[test]
    fn mastered_cards_counted_at_threshold() {
        let cards = vec![
            make_card(1, MASTERY_THRESHOLD - 1),
            make_card(2, MASTERY_THRESHOLD),
            make_card(3, 100),
        ];
        let stats = DeckStats::from_cards(1, "Test".into(), &cards);
        assert_eq!(stats.mastered_cards, 2);
    }

    #[test]
    fn unstudied_card_is_not_mastered() {
        let card = make_card(1, 0);
        assert!(!card.is_mastered());
        assert!(!card.has_been_studied());
    }

    #[test]
    fn mastery_rate_is_fraction() {
        let cards = vec![make_card(1, 100), make_card(2, 0)];
        let stats = DeckStats::from_cards(1, "Test".into(), &cards);
        assert!((stats.mastery_rate() - 0.5).abs() < f32::EPSILON);
    }
}
