//! MovieBox GUI server — a tiny local HTTP server that serves the OTT web
//! front-end and a JSON API backed by the upstream provider stack.
//!
//! Runs on 127.0.0.1 only. Designed for low-spec Windows 7/8.1 machines:
//! no Electron, no WebView2 runtime, works in any modern browser.

pub mod ai_endpoints;
pub mod api;
pub mod assets;
pub mod http;

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

use crate::favorites::FavoritesManager;
use crate::history::HistoryManager;
use crate::service::MovieBoxService;

use self::http::{read_request, write_simple_response, Request, Response};
use self::http::{ParseError};

pub struct AppState {
    pub service: MovieBoxService,
    pub favorites: Mutex<FavoritesManager>,
    pub history: Mutex<HistoryManager>,
    pub ai: Mutex<crate::ai::AiEngine>,
    pub ai_settings: Mutex<crate::ai::AiSettings>,
    pub gui_settings: Mutex<serde_json::Value>,
    pub ai_settings_path: std::path::PathBuf,
    /// Active playback sidecar process (killed when a new one starts).
    pub playback_child: Mutex<Option<std::process::Child>>,
    /// The proxied playback URL handed to the most recent resolve.
    pub playback_url: Mutex<Option<String>>,
    pub http: reqwest::Client,
    pub started_at: std::time::Instant,
}

impl AppState {
    pub fn new() -> Arc<Self> {
        let service = MovieBoxService::new();
        let data_dir = crate::config::data_dir().unwrap_or_else(std::env::temp_dir);
        let ai_settings_path = data_dir.join("ai-settings.json");
        let ai_settings = crate::ai::AiSettings::load(&ai_settings_path);

        let http = crate::net::http_client_builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Arc::new(Self {
            service,
            favorites: Mutex::new(FavoritesManager::new()),
            history: Mutex::new(HistoryManager::new()),
            ai: Mutex::new(crate::ai::AiEngine::with_persistence(
                data_dir.join("ai-knowledge.json"),
            )),
            ai_settings: Mutex::new(ai_settings),
            gui_settings: Mutex::new(serde_json::json!({
                "theme": "dark",
                "max_height": 1080,
                "data_saver": false,
                "reduce_motion": false,
                "kids_mode": false,
                "autoplay_next": true,
                "default_caption": ""
            })),
            ai_settings_path,
            playback_child: Mutex::new(None),
            playback_url: Mutex::new(None),
            http,
            started_at: std::time::Instant::now(),
        })
    }

    pub fn llm_config(&self) -> Option<crate::ai::llm::LlmConfig> {
        let s = self.ai_settings.lock().ok()?;
        if !s.llm_configured() {
            return None;
        }
        Some(crate::ai::llm::LlmConfig {
            endpoint: s.llm_endpoint.clone(),
            api_key: s.llm_api_key.clone(),
            model: s.llm_model.clone(),
        })
    }
}

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

pub async fn run(port: u16, open_browser: bool) -> Result<(), String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| format!("failed to bind {addr}: {e}"))?;

    let state = AppState::new();
    assets::init();

    let url = format!("http://127.0.0.1:{port}/");
    log::info!("MovieBox GUI server listening on {url}");

    if open_browser {
        let url_open = url.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
            let _ = open::that_detached(&url_open);
        });
    }

    // Persist AI knowledge periodically.
    {
        let state_persist = Arc::clone(&state);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(120)).await;
                if let Ok(ai) = state_persist.ai.lock() {
                    ai.save();
                }
            }
        });
    }

    // Ctrl+C handling (server mode).
    {
        let state_shutdown = Arc::clone(&state);
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            log::info!("shutdown signal received");
            kill_playback(&state_shutdown);
            SHUTDOWN.store(true, Ordering::SeqCst);
            std::process::exit(0);
        });
    }

    loop {
        let (stream, _peer) = match listener.accept().await {
            Ok(x) => x,
            Err(e) => {
                log::warn!("accept error: {e}");
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                continue;
            }
        };
        let _ = stream.set_nodelay(true);
        let state_conn = Arc::clone(&state);
        tokio::spawn(async move {
            handle_connection(stream, state_conn).await;
        });
    }
}

pub fn kill_playback(state: &AppState) {
    if let Ok(mut guard) = state.playback_child.lock() {
        if let Some(child) = guard.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        *guard = None;
    }
}

async fn handle_connection(stream: TcpStream, state: Arc<AppState>) {
    let (reader, mut writer) = stream.into_split();
    let mut reader = tokio::io::BufReader::with_capacity(64 * 1024, reader);

    loop {
        let req = match read_request(&mut reader).await {
            Ok(r) => r,
            Err(ParseError::Eof) => return,
            Err(ParseError::TooLarge) => {
                let resp = Response::json_error(431, "request too large");
                let _ = write_simple_response(&mut writer, &resp, false).await;
                return;
            }
            Err(ParseError::Malformed(m)) => {
                let resp = Response::json_error(400, m);
                let _ = write_simple_response(&mut writer, &resp, false).await;
                return;
            }
            Err(ParseError::Io(_)) => return,
        };

        let head_only = req.method == "HEAD";
        let is_assets = req.path.starts_with("/assets/");
        let keep_alive = req.keep_alive;

        let response = route(&req, &state).await;

        // Streaming responses bypass write_simple_response.
        if let Some(mut src) = response.stream {
            let status = src.status;
            let mut headers = src.headers.clone();
            headers.push(("Server".into(), "MovieBox".into()));
            if !headers
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case("content-type"))
            {
                headers.push(("Content-Type".into(), src.content_type.clone()));
            }
            if let Err(e) =
                http::write_response_head(&mut writer, status, "OK", &headers, None).await
            {
                log::debug!("stream write head failed: {e}");
                return;
            }
            while let Some(chunk) = src.chunks.recv().await {
                match chunk {
                    Ok(bytes) => {
                        if head_only {
                            break;
                        }
                        if let Err(e) = writer.write_all(&bytes).await {
                            log::debug!("stream client gone: {e}");
                            return;
                        }
                    }
                    Err(e) => {
                        log::warn!("stream source error: {e}");
                        return;
                    }
                }
            }
            let _ = writer.flush().await;
        } else {
            let resp_for_len = response;
            if write_simple_response(&mut writer, &resp_for_len, head_only).await.is_err() {
                return;
            }
        }

        if !keep_alive {
            return;
        }
        let _ = is_assets;
    }
}

async fn route(req: &Request, state: &Arc<AppState>) -> Response {
    let path = req.path.as_str();

    // Playback proxy hop: /play/<scheme>/<host>/<path...>
    if path.starts_with("/play/") {
        return api::proxy_play(req, state).await;
    }

    match path {
        "/" | "/index.html" => assets::index(req),
        "/favicon.ico" => assets::favicon(req),
        _ if path.starts_with("/assets/") => assets::asset(req),
        "/api/health" => api::health(req, state),
        "/api/home" => api::home(req, state).await,
        "/api/browse" => api::browse(req, state).await,
        "/api/search" => api::search(req, state).await,
        "/api/suggest" => api::suggest(req, state).await,
        "/api/details" => api::details(req, state).await,
        "/api/releases" => api::releases(req, state).await,
        "/api/resolve" => api::resolve(req, state).await,
        "/api/stop-playback" => {
            kill_playback(state);
            Response::json(200, &serde_json::json!({"ok": true}))
        }
        "/api/captions" => api::captions(req, state).await,
        "/api/caption-file" => api::caption_file(req, state).await,
        "/api/poster" => api::poster(req, state).await,
        "/api/favorites" => api::favorites(req, state),
        "/api/history" => api::history(req, state),
        "/api/history/progress" => api::history_progress(req, state),
        "/api/settings" => api::settings(req, state),
        "/api/ai/recommendations" => ai_endpoints::recommendations(req, state),
        "/api/ai/mood" => ai_endpoints::mood(req, state),
        "/api/ai/search" => ai_endpoints::ai_search(req, state).await,
        "/api/ai/insights" => ai_endpoints::insights(req, state),
        "/api/ai/chat" => ai_endpoints::chat(req, state).await,
        "/api/ai/settings" => ai_endpoints::ai_settings(req, state),
        _ => Response::json_error(404, "not found"),
    }
}
