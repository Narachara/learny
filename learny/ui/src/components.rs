// Login flow only exists in the server build; the Tauri app has no auth.
#[cfg(feature = "server")]
pub mod login_page;
#[cfg(feature = "server")]
pub use login_page::LoginPage;

pub mod card_view;
pub use card_view::CardView;

pub mod deck_list;
pub use deck_list::DeckList;

pub mod block_view;
// pub use block_view::render_block;

pub mod card_list_page;
pub use card_list_page::CardListPage;

pub mod block_editor;
pub use block_editor::BlockEditor;

pub mod card_editor;
pub use card_editor::{CardEditorEdit, CardEditorNew};

pub mod create_deck;
pub use create_deck::CreateDeck;

pub mod loading_modal;
pub use loading_modal::LoadingModal;

pub mod knowledge_map_page;
pub use knowledge_map_page::KnowledgeMapPage;

pub mod tab_bar;
pub use tab_bar::{TabStrip, TabInstance};

pub mod find_bar;
pub use find_bar::FindBar;

pub mod media_upload_overlay;
pub use media_upload_overlay::MediaUploadOverlay;
