use std::sync::Arc;
use application::analytics_service::AnalyticsService;
use application::flashcard_service::FlashcardService;
use application::study_service::StudyService;
use application::tagging_service::TaggingService;
use ports::transaction::TransactionScope;

pub struct Services {
    pub flashcard:  FlashcardService,
    pub study:      StudyService,
    pub tagging:    TaggingService,
    /// No route reads deck stats yet; kept so the registry matches the Tauri host.
    #[allow(dead_code)]
    pub analytics:  AnalyticsService,
    /// Explicit transaction control for bulk operations (deck import).
    pub tx:         Box<dyn TransactionScope + Send + Sync>,
}

/// Creates per-request service instances scoped to the authenticated user.
pub struct ServiceFactory {
    make: Arc<dyn Fn(&str) -> Services + Send + Sync>,
}

impl ServiceFactory {
    pub fn new(make: impl Fn(&str) -> Services + Send + Sync + 'static) -> Self {
        Self { make: Arc::new(make) }
    }

    pub fn for_user(&self, user_id: &str) -> Services {
        (self.make)(user_id)
    }
}

pub type SharedServiceFactory = Arc<ServiceFactory>;
