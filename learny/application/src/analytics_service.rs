use domain_analytics::{CardProgress, DeckStats};
use domain_flashcard::Card;
use ports::card_repo::CardRepository;
use ports::deck_repo::DeckRepository;
use crate::AppError;

pub struct AnalyticsService {
    card_repo: Box<dyn CardRepository + Send + Sync>,
    deck_repo: Box<dyn DeckRepository + Send + Sync>,
    user_id:   String,
}

impl AnalyticsService {
    pub fn new(
        card_repo: Box<dyn CardRepository + Send + Sync>,
        deck_repo: Box<dyn DeckRepository + Send + Sync>,
        user_id:   String,
    ) -> Self {
        Self { card_repo, deck_repo, user_id }
    }

    /// Stats for a single deck — used on the deck detail view.
    pub fn get_deck_stats(&self, deck_id: i64) -> Result<DeckStats, AppError> {
        let deck = self.deck_repo
            .find_by_id(deck_id)?
            .ok_or(AppError::NotFound)?;

        let cards = self.card_repo.find_by_deck(deck_id)?;
        let progress = cards_to_progress(cards);

        Ok(DeckStats::from_cards(deck.id, deck.name, &progress))
    }

    /// Stats for every deck — feeds the global dashboard.
    pub fn get_global_progress(&self) -> Result<Vec<DeckStats>, AppError> {
        let decks = self.deck_repo.find_all_for_user(&self.user_id)?;

        let mut result = Vec::with_capacity(decks.len());
        for deck in decks {
            let cards = self.card_repo.find_by_deck(deck.id)?;
            let progress = cards_to_progress(cards);
            result.push(DeckStats::from_cards(deck.id, deck.name, &progress));
        }
        Ok(result)
    }
}

// -------------------------------------------------------------------------
// Private helpers
// -------------------------------------------------------------------------

fn cards_to_progress(cards: Vec<Card>) -> Vec<CardProgress> {
    cards
        .into_iter()
        .map(|c| {
            let progress = c.progress_percent();
            CardProgress {
                card_id:          c.id,
                card_name:        c.name,
                deck_id:          c.deck_id,
                times_seen:       c.times_seen,
                times_correct:    c.times_correct,
                progress_percent: progress,
            }
        })
        .collect()
}
