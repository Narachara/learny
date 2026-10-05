use domain_flashcard::Card;
use ports::card_repo::CardRepository;
use crate::AppError;

/// Records study answers. Sessions were removed; rating a card happens
/// directly from the card view now.
pub struct StudyService {
    card_repo: Box<dyn CardRepository + Send + Sync>,
}

impl StudyService {
    pub fn new(card_repo: Box<dyn CardRepository + Send + Sync>) -> Self {
        Self { card_repo }
    }

    /// Record the result of a study answer and return the updated card.
    pub fn rate_card(&self, card_id: i64, correct: bool) -> Result<Card, AppError> {
        Ok(self.card_repo.record_answer(card_id, correct)?)
    }
}
