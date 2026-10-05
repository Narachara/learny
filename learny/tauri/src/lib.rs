mod commands;

use tauri::Manager;
use tauri::http;
use mime_guess;
use urlencoding;

use adapter_sqlite::{
    open_db, SqliteCardRepository, SqliteDeckRepository,
    SqliteTagRepository,
    SqliteTransactionScope,
};
use ports::transaction::TransactionScope;
use application::analytics_service::AnalyticsService;
use application::flashcard_service::FlashcardService;
use application::study_service::StudyService;
use application::tagging_service::TaggingService;

use crate::commands::{
    export_deck,
    import_deck,
    add_deck,
    get_deck,
    get_decks,
    rename_deck,
    delete_deck,
    add_card,
    get_card,
    get_cards,
    search_cards,
    save_card_blocks,
    update_card_metadata,
    delete_card,
    update_score,
    download_file,
    delete_block_from_app_data,
    set_deck_cover,
    set_card_cover,
    save_card_notes,
    get_all_tags_full,
    get_all_tags,
    create_tag,
    rename_tag,
    delete_tag,
    get_cards_by_tags,
    get_card_tags,
    get_card_tags_with_position,
    get_card_tag_positions,
    set_card_tag_position,
    get_card_tag_pairs,
    sync_card_tags,
    add_tag_edge,
    remove_tag_edge,
    get_tag_edges,
    assign_tag_to_deck,
    unassign_tag_from_deck,
    get_tag_deck_pairs,
    detach_tag_from_cards,
    set_tag_position,
    clear_tag_position,
    reset_tag_map,
    reset_deck_progress,
    move_cards,
    get_deck_tags,
    render_markdown,
};

/// All application services bundled together for managed state.
/// Wrapped in Mutex so Tauri can share it across threads.
pub struct Services {
    pub flashcard:  FlashcardService,
    pub study:      StudyService,
    pub tagging:    TaggingService,
    pub analytics:  AnalyticsService,
    /// Explicit transaction control for bulk operations (deck import).
    pub tx:         Box<dyn TransactionScope + Send + Sync>,
}

/// Max bytes served per Range response. The webview follows up with further
/// Range requests, so large videos stream from disk chunk by chunk instead of
/// being read into memory whole.
const APPIMG_CHUNK_SIZE: u64 = 8 * 1024 * 1024;

/// Parses a single-range `Range` header value.
/// Returns (start, end): `bytes=a-b` → (Some(a), Some(b)), `bytes=a-` →
/// (Some(a), None), suffix form `bytes=-n` → (None, Some(n)).
fn parse_range_header(value: &str) -> Option<(Option<u64>, Option<u64>)> {
    let spec = value.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (start, end) = spec.split_once('-')?;
    let start = if start.is_empty() { None } else { Some(start.parse().ok()?) };
    let end = if end.is_empty() { None } else { Some(end.parse().ok()?) };
    if start.is_none() && end.is_none() {
        return None;
    }
    Some((start, end))
}

/// Serves a file from app data over the appimg:// protocol, honoring Range
/// requests (206) so `<video>`/`<audio>` can stream from disk and seek without
/// downloading the entire file first. WKWebView in particular won't start
/// playback of a custom-scheme video until ranges are supported.
fn serve_appimg<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    request: &http::Request<Vec<u8>>,
) -> http::Response<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};

    let uri = request.uri();
    let raw_path = uri.path();
    let decoded = urlencoding::decode(raw_path).unwrap();
    let mut virtual_path = decoded.trim_start_matches('/').to_string();

    if let Some(host) = uri.host() {
        // macOS/Linux deliver appimg://<first-seg>/<rest>, putting the first
        // path segment into the host — splice it back onto the path. Windows
        // uses http://appimg.localhost/<path>, where the host is synthetic and the
        // full path is already present, so leave those (and plain "localhost") alone.
        if host != "localhost"
            && !host.ends_with(".localhost")
            && !virtual_path.starts_with(&format!("{}/", host))
        {
            virtual_path = format!("{}/{}", host, virtual_path);
        }
    }

    let app_data_dir = app.path().app_data_dir().unwrap();
    let full_path = app_data_dir.join(&virtual_path);

    let mut file = match std::fs::File::open(&full_path) {
        Ok(f) => f,
        Err(e) => {
            return http::Response::builder()
                .status(404)
                .body(format!("missing file: {}", e).into_bytes())
                .unwrap();
        }
    };
    let size = match file.metadata() {
        Ok(m) => m.len(),
        Err(e) => {
            return http::Response::builder()
                .status(500)
                .body(format!("metadata error: {}", e).into_bytes())
                .unwrap();
        }
    };
    let mime = mime_guess::from_path(&full_path)
        .first_or_octet_stream()
        .to_string();

    let range = request
        .headers()
        .get("range")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_range_header);

    let Some((start, end)) = range else {
        // No Range header: whole file (images, small audio, initial probes).
        return match std::fs::read(&full_path) {
            Ok(bytes) => http::Response::builder()
                .status(200)
                .header("Content-Type", mime)
                .header("Content-Length", bytes.len().to_string())
                .header("Accept-Ranges", "bytes")
                .body(bytes)
                .unwrap(),
            Err(e) => http::Response::builder()
                .status(500)
                .body(format!("read error: {}", e).into_bytes())
                .unwrap(),
        };
    };

    let (start, requested_end) = match (start, end) {
        (Some(s), e) => (s, e.unwrap_or(size.saturating_sub(1))),
        // Suffix form `bytes=-n`: the final n bytes.
        (None, Some(n)) => (size.saturating_sub(n), size.saturating_sub(1)),
        (None, None) => unreachable!(),
    };

    if size == 0 || start >= size {
        return http::Response::builder()
            .status(416)
            .header("Content-Range", format!("bytes */{}", size))
            .body(Vec::new())
            .unwrap();
    }

    let end = requested_end
        .min(size - 1)
        .min(start + APPIMG_CHUNK_SIZE - 1);
    let len = (end - start + 1) as usize;

    let mut buf = vec![0u8; len];
    let read_result = file
        .seek(SeekFrom::Start(start))
        .and_then(|_| file.read_exact(&mut buf));
    if let Err(e) = read_result {
        return http::Response::builder()
            .status(500)
            .body(format!("read error: {}", e).into_bytes())
            .unwrap();
    }

    http::Response::builder()
        .status(206)
        .header("Content-Type", mime)
        .header("Content-Length", len.to_string())
        .header("Content-Range", format!("bytes {}-{}/{}", start, end, size))
        .header("Accept-Ranges", "bytes")
        .body(buf)
        .unwrap()
}

pub fn run() {
    tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol("appimg", |ctx, request, responder| {
            let app_handle = ctx.app_handle().clone();
            // File I/O on its own thread so a large media read never blocks
            // the protocol pipeline (and the asset requests queued behind it).
            std::thread::spawn(move || {
                responder.respond(serve_appimg(&app_handle, &request));
            });
        })
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
                window.close_devtools();
            }

            // --- Composition root ---
            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;
            let db_path = app_data_dir.join("cards.db");

            let conn = open_db(&db_path)
                .map_err(|e| format!("failed to open database: {}", e))?;

            let cards = || Box::new(SqliteCardRepository::new(conn.clone(), app_data_dir.clone()));
            let decks = || Box::new(SqliteDeckRepository::new(conn.clone(), app_data_dir.clone()));
            let tags  = || Box::new(SqliteTagRepository::new(conn.clone()));
            let uid   = "local".to_string();

            app.manage(Services {
                flashcard: FlashcardService::new(cards(), decks(), uid.clone()),
                study:     StudyService::new(cards()),
                tagging:   TaggingService::new(tags(), cards()),
                analytics: AnalyticsService::new(cards(), decks(), uid.clone()),
                tx:        Box::new(SqliteTransactionScope::new(conn.clone())),
            });

            Ok(())
        })
        .plugin(tauri_plugin_fs_adapter::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            add_deck,
            get_deck,
            get_decks,
            rename_deck,
            delete_deck,
            add_card,
            get_card,
            get_cards,
            search_cards,
            save_card_blocks,
            update_card_metadata,
            delete_card,
            update_score,
            download_file,
            delete_block_from_app_data,
            set_deck_cover,
            set_card_cover,
            export_deck,
            import_deck,
            save_card_notes,
            get_all_tags_full,
            get_all_tags,
            create_tag,
            rename_tag,
            delete_tag,
            get_cards_by_tags,
            get_card_tags,
            get_card_tags_with_position,
            get_card_tag_positions,
            set_card_tag_position,
            sync_card_tags,
            add_tag_edge,
            remove_tag_edge,
            get_tag_edges,
            assign_tag_to_deck,
            unassign_tag_from_deck,
            get_tag_deck_pairs,
            detach_tag_from_cards,
            set_tag_position,
            clear_tag_position,
            reset_tag_map,
            reset_deck_progress,
            move_cards,
            get_deck_tags,
            get_card_tag_pairs,
                    render_markdown,
                                                                                                        ])
        .run(tauri::generate_context!())
        .expect("error running app");
}
