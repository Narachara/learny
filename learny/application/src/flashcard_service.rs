use domain_flashcard::{Block, Card, CardError, Deck, DeckError};
use ports::card_repo::CardRepository;
use ports::deck_repo::DeckRepository;
use crate::AppError;

pub struct FlashcardService {
    card_repo: Box<dyn CardRepository + Send + Sync>,
    deck_repo: Box<dyn DeckRepository + Send + Sync>,
    user_id:   String,
}

impl FlashcardService {
    pub fn new(
        card_repo: Box<dyn CardRepository + Send + Sync>,
        deck_repo: Box<dyn DeckRepository + Send + Sync>,
        user_id:   String,
    ) -> Self {
        Self { card_repo, deck_repo, user_id }
    }

    // --- Ownership helpers ---

    fn owned_deck(&self, deck_id: i64) -> Result<Deck, AppError> {
        let deck = self.deck_repo.find_by_id(deck_id)?.ok_or(AppError::NotFound)?;
        if deck.user_id != self.user_id {
            return Err(AppError::NotFound);
        }
        Ok(deck)
    }

    fn check_card_owner(&self, card_id: i64) -> Result<(), AppError> {
        let card = self.card_repo.find_by_id(card_id)?.ok_or(AppError::NotFound)?;
        self.owned_deck(card.deck_id)?;
        Ok(())
    }

    // --- Deck use cases ---

    pub fn get_deck(&self, deck_id: i64) -> Result<Deck, AppError> {
        self.owned_deck(deck_id)
    }

    pub fn create_deck(&self, name: &str) -> Result<Deck, AppError> {
        let created_at = chrono::Utc::now().timestamp();
        let mut deck = Deck::new(name, created_at)
            .map_err(|e| match e {
                DeckError::EmptyName => AppError::ValidationError("Deck name cannot be empty".into()),
            })?;
        deck.user_id = self.user_id.clone();
        self.deck_repo.save(&mut deck)?;
        Ok(deck)
    }

    pub fn get_decks(&self) -> Result<Vec<Deck>, AppError> {
        Ok(self.deck_repo.find_all_for_user(&self.user_id)?)
    }

    pub fn rename_deck(&self, deck_id: i64, name: &str) -> Result<Deck, AppError> {
        let mut deck = self.owned_deck(deck_id)?;

        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            return Err(AppError::ValidationError("Deck name cannot be empty".into()));
        }
        deck.name = trimmed;
        self.deck_repo.save(&mut deck)?;
        Ok(deck)
    }

    pub fn delete_deck(&self, deck_id: i64) -> Result<(), AppError> {
        self.owned_deck(deck_id)?;
        let cards = self.card_repo.find_by_deck(deck_id)?;
        for card in cards {
            self.card_repo.delete(card.id)?;
        }
        self.deck_repo.delete(deck_id)?;
        Ok(())
    }

    // --- Card use cases ---

    pub fn add_card(&self, deck_id: i64, name: &str) -> Result<Card, AppError> {
        self.owned_deck(deck_id)?;
        let created_at = chrono::Utc::now().timestamp();
        let mut card = Card::new(deck_id, name, created_at)
            .map_err(|e| match e {
                CardError::EmptyName => AppError::ValidationError("Card name cannot be empty".into()),
            })?;
        self.card_repo.save(&mut card)?;
        Ok(card)
    }

    pub fn get_card(&self, id: i64) -> Result<Card, AppError> {
        let card = self.card_repo.find_by_id(id)?.ok_or(AppError::NotFound)?;
        self.owned_deck(card.deck_id)?;
        Ok(card)
    }

    pub fn get_cards(&self, deck_id: i64) -> Result<Vec<Card>, AppError> {
        self.owned_deck(deck_id)?;
        Ok(self.card_repo.find_by_deck(deck_id)?)
    }

    pub fn search_cards(&self, query: &str) -> Result<Vec<Card>, AppError> {
        Ok(self.card_repo.search(query)?)
    }

    /// Like get_cards but with blocks populated — use for export.
    pub fn get_cards_full(&self, deck_id: i64) -> Result<Vec<Card>, AppError> {
        self.owned_deck(deck_id)?;
        Ok(self.card_repo.find_by_deck_full(deck_id)?)
    }

    pub fn rename_card(&self, card_id: i64, name: &str) -> Result<Card, AppError> {
        let mut card = self.card_repo
            .find_by_id(card_id)?
            .ok_or(AppError::NotFound)?;
        self.owned_deck(card.deck_id)?;

        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            return Err(AppError::ValidationError("Card name cannot be empty".into()));
        }
        card.name = trimmed;
        self.card_repo.save(&mut card)?;
        Ok(card)
    }

    pub fn save_blocks(
        &self,
        card_id: i64,
        front: Vec<Block>,
        back: Vec<Block>,
    ) -> Result<(), AppError> {
        let mut card = self.card_repo
            .find_by_id(card_id)?
            .ok_or(AppError::NotFound)?;
        self.owned_deck(card.deck_id)?;
        card.front_blocks = front;
        card.back_blocks = back;
        self.card_repo.save(&mut card)?;
        Ok(())
    }

    pub fn delete_card(&self, id: i64) -> Result<(), AppError> {
        self.check_card_owner(id)?;
        self.card_repo.delete(id)?;
        Ok(())
    }

    pub fn restore_card(&self, deck_id: i64, mut card: Card) -> Result<Card, AppError> {
        self.owned_deck(deck_id)?;
        card.id = -1;
        card.deck_id = deck_id;
        card.cover_image = None;
        card.what_was_easy = None;
        card.what_was_difficult = None;
        self.card_repo.save(&mut card)?;
        Ok(card)
    }


    pub fn set_deck_cover(&self, deck_id: i64, virtual_path: Option<String>) -> Result<(), AppError> {
        self.owned_deck(deck_id)?;
        self.deck_repo.set_cover_image(deck_id, virtual_path)?;
        Ok(())
    }

    pub fn set_card_cover(&self, card_id: i64, virtual_path: Option<String>) -> Result<(), AppError> {
        self.check_card_owner(card_id)?;
        self.card_repo.set_cover_image(card_id, virtual_path)?;
        Ok(())
    }

    pub fn save_card_notes(&self, card_id: i64, easy: Option<String>, difficult: Option<String>) -> Result<(), AppError> {
        self.check_card_owner(card_id)?;
        self.card_repo.save_notes(card_id, easy, difficult)?;
        Ok(())
    }

    pub fn move_cards(&self, card_ids: Vec<i64>, target_deck_id: i64) -> Result<(), AppError> {
        self.owned_deck(target_deck_id)?;
        self.card_repo.move_to_deck(&card_ids, target_deck_id)?;
        Ok(())
    }

    pub fn reset_deck_progress(&self, deck_id: i64) -> Result<(), AppError> {
        self.owned_deck(deck_id)?;
        self.card_repo.reset_progress(deck_id)?;
        Ok(())
    }
}
