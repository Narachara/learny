mod api;
mod auth_backend;
mod state;

use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use axum::{
    extract::{DefaultBodyLimit, Request},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Extension, Router,
};
use tower_http::{cors::CorsLayer, services::{ServeDir, ServeFile}};
use tower_sessions::{MemoryStore, SessionManagerLayer, cookie::SameSite};
use axum_login::AuthManagerLayerBuilder;

use adapter_sqlite::{
    open_db, SqliteCardRepository, SqliteDeckRepository,
    SqliteTagRepository, SqliteUserRepository,
};
use application::{
    analytics_service::AnalyticsService,
    flashcard_service::FlashcardService,
    study_service::StudyService,
    tagging_service::TaggingService,
};
use auth_backend::{AuthBackend, AuthSession};
use state::{ServiceFactory, SharedServiceFactory};
use api::files::DataDir;
use ports::user_repo::UserRepository;

// ---------------------------------------------------------------------------
// Auth middleware — returns 401 for unauthenticated API requests
// ---------------------------------------------------------------------------

async fn require_auth(auth_session: AuthSession, request: Request, next: Next) -> Response {
    if auth_session.user.is_none() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(request).await
}

#[tokio::main]
async fn main() {
    let data_dir: PathBuf = std::env::var("DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("learny-web")
        });

    std::fs::create_dir_all(&data_dir).expect("cannot create data directory");
    let db_path = data_dir.join("cards.db");

    // -----------------------------------------------------------------------
    // Composition root
    // -----------------------------------------------------------------------
    let conn = open_db(&db_path).expect("failed to open database");
    let data_dir_ext: DataDir = Arc::new(data_dir.clone());

    let factory: SharedServiceFactory = Arc::new(ServiceFactory::new({
        let conn = conn.clone();
        let data_dir = data_dir.clone();
        move |user_id: &str| {
            let cards = || Box::new(SqliteCardRepository::new(conn.clone(), data_dir.clone()));
            let decks = || Box::new(SqliteDeckRepository::new(conn.clone(), data_dir.clone()));
            let tags  = || Box::new(SqliteTagRepository::new(conn.clone()));
            let uid   = user_id.to_string();
            state::Services {
                flashcard: FlashcardService::new(cards(), decks(), uid.clone()),
                study:     StudyService::new(cards()),
                tagging:   TaggingService::new(tags(), cards()),
                analytics: AnalyticsService::new(cards(), decks(), uid),
                tx:        Box::new(adapter_sqlite::SqliteTransactionScope::new(conn.clone())),
            }
        }
    }));

    // User repository — shared via Extension and also used by the auth backend.
    let user_repo: Arc<dyn UserRepository> =
        Arc::new(SqliteUserRepository::new(conn.clone()));

    // -----------------------------------------------------------------------
    // Auth layer (tower-sessions + axum-login)
    // -----------------------------------------------------------------------
    // NOTE: MemoryStore loses all sessions on restart. Fine for a single-user
    // or dev setup; replace with a SQLite-backed store for zero-logout deploys.
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)          // set true behind HTTPS in production
        .with_same_site(SameSite::Lax);

    let backend    = AuthBackend::new(user_repo.clone());
    let auth_layer = AuthManagerLayerBuilder::new(backend, session_layer).build();

    // -----------------------------------------------------------------------
    // Static assets
    // -----------------------------------------------------------------------
    const WORKSPACE_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");

    let dist_dir = std::env::var("DIST_DIR").unwrap_or_else(|_| {
        format!("{}/target/dx/ui/release/web/public", WORKSPACE_ROOT)
    });

    if !std::path::Path::new(&dist_dir).exists() {
        eprintln!("⚠️  dist dir not found: {}", dist_dir);
        eprintln!("   Run: cd ui && dx build --no-default-features --features server --release");
        eprintln!("   Or set DIST_DIR to point at your build output.");
        std::process::exit(1);
    }

    // -----------------------------------------------------------------------
    // Router
    // -----------------------------------------------------------------------

    // Public auth routes — no login required.
    let auth_routes = Router::new()
        .route("/auth/login",    post(api::auth::login))
        .route("/auth/logout",   post(api::auth::logout))
        .route("/auth/register", post(api::auth::register))
        .route("/auth/me",       get(api::auth::me));

    // Protected API routes — require a valid session.
    let protected_routes = Router::new()
        .route("/decks",                     get(api::decks::list))
        .route("/decks",                     post(api::decks::create))
        .route("/decks/import",              post(api::decks::import_deck)
            .layer(DefaultBodyLimit::max(100 * 1024 * 1024)))
        .route("/decks/:id/export",          get(api::decks::export))
        .route("/decks/:id/rename",          put(api::decks::rename))
        .route("/decks/:id",                 delete(api::decks::delete))
        .route("/decks/:id/cover",           put(api::decks::set_cover))
        .route("/decks/:deck_id/cards",      get(api::cards::list))
        .route("/decks/:deck_id/cards",      post(api::cards::create))
        .route("/cards/:id",                 get(api::cards::get))
        .route("/cards/:id/name",            put(api::cards::rename))
        .route("/cards/:id/blocks",          put(api::cards::save_blocks))
        .route("/cards/:id",                 delete(api::cards::delete))
        .route("/cards/:id/answer",          post(api::cards::answer))
        .route("/cards/:id/cover",           put(api::cards::set_cover))
        .route("/cards/:id/notes",           put(api::cards::save_notes))
        .route("/tags-full",                 get(api::cards::get_all_tags_full))
        .route("/tags",                      get(api::cards::get_all_tags))
        .route("/tags",                      post(api::cards::create_tag))
        .route("/tags/:id",                  put(api::cards::rename_tag))
        .route("/tags/:id",                  delete(api::cards::delete_tag))
        .route("/tags/:id/cards",            delete(api::cards::detach_tag_from_cards))
        .route("/cards/by-tags",             get(api::cards::get_cards_by_tags))
        .route("/cards/search",              get(api::cards::search_cards))
        .route("/cards/:id/tags",            get(api::cards::get_tags))
        .route("/cards/:id/tags",            put(api::cards::sync_tags))
        .route("/cards/:id/tags-with-position", get(api::cards::get_tags_with_position))
        .route("/cards/:id/tags/:tag_id/position", put(api::cards::set_card_tag_position))
        .route("/tags/edges",                get(api::cards::get_tag_edges))
        .route("/tags/edges",                post(api::cards::add_tag_edge))
        .route("/tags/edges",                delete(api::cards::remove_tag_edge))
        .route("/tags/deck-pairs",           get(api::cards::get_tag_deck_pairs))
        .route("/tags/card-positions",       get(api::cards::get_card_tag_positions))
        .route("/tags/deck",                 post(api::cards::assign_tag_to_deck))
        .route("/tags/deck",                 delete(api::cards::unassign_tag_from_deck))
        .route("/tags/:id/position",         put(api::cards::set_tag_position))
        .route("/tags/:id/position",         delete(api::cards::clear_tag_position))
        .route("/tags/reset-map",            post(api::cards::reset_tag_map))
        .route("/cards/move",                post(api::cards::move_cards))
        .route("/decks/:id",                 get(api::decks::get))
        .route("/decks/:id/reset-progress",  post(api::decks::reset_progress))
        .route("/decks/:id/deck-tags",        get(api::decks::get_deck_tags))
        .route("/decks/:id/card-tag-pairs",   get(api::decks::get_card_tag_pairs))
        .route("/files/upload",              post(api::files::upload)
            .layer(DefaultBodyLimit::max(api::files::UPLOAD_LIMIT)))
        .route("/files/upload-cover",        post(api::files::upload_cover)
            .layer(DefaultBodyLimit::max(api::files::UPLOAD_LIMIT)))
        .route("/files/*path",               delete(api::files::delete))
        .route_layer(middleware::from_fn(require_auth));

    let app = Router::new()
        .nest("/api", auth_routes)
        .nest("/api", protected_routes)
        .route("/files/*path", get(api::files::serve))
        .fallback_service(
            ServeDir::new(&dist_dir)
                .fallback(ServeFile::new(format!("{}/index.html", dist_dir)))
        )
        .layer(auth_layer)
        .layer(Extension(factory))
        .layer(Extension(data_dir_ext))
        .layer(Extension(user_repo))
        .layer(CorsLayer::permissive());

    // -----------------------------------------------------------------------
    // Bind and serve
    // -----------------------------------------------------------------------
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("learny-web listening on http://{addr}");
    println!("  data dir : {}", data_dir.display());
    println!("  dist dir : {}", dist_dir);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
