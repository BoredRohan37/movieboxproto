//! JSON API handlers for the GUI front-end, plus the playback proxy hop.

use std::sync::Arc;

use serde_json::json;

use super::http::{Request, Response, StreamSource};
use super::AppState;

fn ok(value: serde_json::Value) -> Response {
    Response::json(200, &value).no_store()
}

fn err(status: u16, message: impl Into<String>) -> Response {
    Response::json_error(status, &message.into()).no_store()
}

fn provider_of(req: &Request) -> crate::providers::models::ProviderKind {
    req.query_param("provider")
        .and_then(crate::providers::models::ProviderKind::parse)
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

pub fn health(req: &Request, state: &Arc<AppState>) -> Response {
    let _ = req;
    let knowledge = state.ai.lock().map(|a| a.knowledge_len()).unwrap_or(0);
    let llm = state.llm_config().is_some();
    ok(json!({
        "ok": true,
        "name": "MovieBox",
        "version": super::assets::APP_VERSION,
        "uptime_secs": state.started_at.elapsed().as_secs(),
        "ai": { "knowledge": knowledge, "llm": llm }
    }))
}

// ---------------------------------------------------------------------------
// Catalog
// ---------------------------------------------------------------------------

pub async fn home(req: &Request, state: &Arc<AppState>) -> Response {
    let page: usize = req
        .query_param("page")
        .and_then(|p| p.parse().ok())
        .unwrap_or(1);
    let payload = match state.service.client.get_homepage("2", page).await {
        Ok(p) => p,
        Err(e) => return err(502, format!("homepage unavailable: {e}")),
    };
    let (sections, metrics) =
        crate::providers::moviebox::adapt_gui::moviebox_homepage_json_to_sections(&payload);

    // Feed the AI knowledge base — items plus genres inferred from the
    // section titles they appeared in ("Sci-Fi Future" → science fiction).
    if let Ok(mut ai) = state.ai.lock() {
        for section in &sections {
            ai.learn_catalog(&section.items);
            let inferred = crate::ai::lexicon::expand_genres(&section.title.to_ascii_lowercase());
            ai.learn_section_genres(&section.items, &inferred);
        }
    }

    let sections_json: Vec<_> = sections
        .iter()
        .map(|s| {
            json!({
                "title": s.title,
                "items": s.items,
            })
        })
        .collect();

    ok(json!({ "page": page, "sections": sections_json, "metrics": metrics }))
}

pub async fn browse(req: &Request, state: &Arc<AppState>) -> Response {
    let preset = req.query_param("preset").unwrap_or("trending");
    let page: usize = req
        .query_param("page")
        .and_then(|p| p.parse().ok())
        .unwrap_or(1);

    let payload = match state.service.client.get_homepage("2", page).await {
        Ok(p) => p,
        Err(e) => return err(502, format!("browse unavailable: {e}")),
    };
    let (items, metrics) =
        crate::providers::moviebox::adapt::moviebox_homepage_json_to_catalog(&payload);

    let mut items = items;
    // Stable metric-based ordering per preset.
    match preset {
        "top" | "top_rated" => {
            items.sort_by(|a, b| {
                let ra = metrics
                    .get(&b.id.value)
                    .and_then(|m| m.rating)
                    .unwrap_or(0.0);
                let rb = metrics
                    .get(&a.id.value)
                    .and_then(|m| m.rating)
                    .unwrap_or(0.0);
                rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        "recent" => {
            items.sort_by(|a, b| {
                let ra = metrics
                    .get(&b.id.value)
                    .and_then(|m| m.recent_rating)
                    .unwrap_or(0.0);
                let rb = metrics
                    .get(&a.id.value)
                    .and_then(|m| m.recent_rating)
                    .unwrap_or(0.0);
                rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        "most" | "most_watched" => {
            items.sort_by(|a, b| {
                let ra = metrics
                    .get(&b.id.value)
                    .and_then(|m| m.popularity)
                    .unwrap_or(0.0);
                let rb = metrics
                    .get(&a.id.value)
                    .and_then(|m| m.popularity)
                    .unwrap_or(0.0);
                rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        _ => {} // trending: provider order already correct
    }

    if let Ok(mut ai) = state.ai.lock() {
        ai.learn_catalog(&items);
    }

    ok(json!({ "preset": preset, "page": page, "items": items }))
}

pub async fn search(req: &Request, state: &Arc<AppState>) -> Response {
    let q = req.query_param("q").unwrap_or("").trim();
    if q.is_empty() {
        return ok(json!({ "items": [] }));
    }
    let page: usize = req
        .query_param("page")
        .and_then(|p| p.parse().ok())
        .unwrap_or(1);
    let provider = provider_of(req);

    let items = match state.service.search_typed(provider, q, page).await {
        Ok(items) => items,
        Err(e) => return err(502, format!("search failed: {e}")),
    };

    if let Ok(mut ai) = state.ai.lock() {
        ai.learn_catalog(&items);
    }

    ok(json!({ "query": q, "page": page, "items": items }))
}

pub async fn suggest(req: &Request, state: &Arc<AppState>) -> Response {
    let q = req.query_param("q").unwrap_or("").trim();
    if q.is_empty() {
        return ok(json!({ "suggestions": [] }));
    }
    match state.service.suggest(q).await {
        Ok(list) => ok(json!({ "suggestions": list })),
        Err(e) => err(502, format!("suggest failed: {e}")),
    }
}

pub async fn details(req: &Request, state: &Arc<AppState>) -> Response {
    let id = req.query_param("id").unwrap_or("").trim();
    if id.is_empty() {
        return err(400, "missing id");
    }
    let provider = provider_of(req);
    let details = match state.service.details_typed(provider, id).await {
        Ok(d) => d,
        Err(e) => return err(502, format!("details failed: {e}")),
    };

    if let Ok(mut ai) = state.ai.lock() {
        ai.learn_details(&details);
    }

    let favorite = state
        .favorites
        .lock()
        .map(|f| {
            f.items
                .iter()
                .any(|i| i.subject_id == id && i.provider == provider.cache_key())
        })
        .unwrap_or(false);

    // History entries for this subject (any season/episode).
    let history_entries: Vec<_> = state
        .history
        .lock()
        .map(|h| {
            h.recent
                .iter()
                .filter(|i| i.subject_id == id)
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    ok(json!({
        "details": details,
        "favorite": favorite,
        "history": history_entries,
    }))
}

pub async fn releases(req: &Request, state: &Arc<AppState>) -> Response {
    let id = req.query_param("id").unwrap_or("").trim();
    if id.is_empty() {
        return err(400, "missing id");
    }
    let season: usize = req.query_param("season").and_then(|s| s.parse().ok()).unwrap_or(0);
    let episode: usize = req.query_param("episode").and_then(|s| s.parse().ok()).unwrap_or(0);
    let provider = provider_of(req);

    let releases = match releases_for(state, provider, id, season, episode).await {
        Ok(r) => r,
        Err(e) => return err(502, e),
    };

    ok(json!({ "releases": releases }))
}

async fn releases_for(
    state: &Arc<AppState>,
    provider: crate::providers::models::ProviderKind,
    id: &str,
    season: usize,
    episode: usize,
) -> Result<Vec<crate::providers::models::Release>, String> {
    use crate::providers::ReleaseProvider;
    let releases = match provider {
        crate::providers::models::ProviderKind::MovieBox => {
            ReleaseProvider::episode_streams(&state.service.client, id, season, episode)
                .await
                .map_err(|e| e.to_string())?
        }
        crate::providers::models::ProviderKind::FourKHdHub => {
            let client = state
                .service
                .fourk_client
                .as_ref()
                .ok_or_else(|| "4KHDHub unavailable".to_string())?;
            ReleaseProvider::episode_streams(client, id, season, episode)
                .await
                .map_err(|e| e.to_string())?
        }
        crate::providers::models::ProviderKind::BdixCircleFtp => {
            ReleaseProvider::episode_streams(&state.service.circleftp_client, id, season, episode)
                .await
                .map_err(|e| e.to_string())?
        }
        crate::providers::models::ProviderKind::BdixDhakaFlix => {
            ReleaseProvider::episode_streams(&state.service.dhakaflix_client, id, season, episode)
                .await
                .map_err(|e| e.to_string())?
        }
        crate::providers::models::ProviderKind::Dramachi => {
            ReleaseProvider::episode_streams(&state.service.dramachi_client, id, season, episode)
                .await
                .map_err(|e| e.to_string())?
        }
        crate::providers::models::ProviderKind::Addons => {
            return Err("addon catalogs do not expose direct releases".to_string());
        }
    };
    Ok(releases)
}

// ---------------------------------------------------------------------------
// Playback
// ---------------------------------------------------------------------------

pub async fn resolve(req: &Request, state: &Arc<AppState>) -> Response {
    let id = req.query_param("id").unwrap_or("").trim();
    if id.is_empty() {
        return err(400, "missing id");
    }
    let provider = provider_of(req);
    let season: usize = req.query_param("season").and_then(|s| s.parse().ok()).unwrap_or(0);
    let episode: usize = req.query_param("episode").and_then(|s| s.parse().ok()).unwrap_or(0);
    let release_idx: usize = req
        .query_param("release")
        .and_then(|r| r.parse().ok())
        .unwrap_or(0);
    let mirror_idx: usize = req
        .query_param("mirror")
        .and_then(|m| m.parse().ok())
        .unwrap_or(0);
    let max_height: Option<u64> = req
        .query_param("max_height")
        .and_then(|m| m.parse().ok());
    let _title = req.query_param("title").unwrap_or("").to_string();

    let releases = match releases_for(state, provider, id, season, episode).await {
        Ok(r) if !r.is_empty() => r,
        Ok(_) => return err(404, "no streams found for this title"),
        Err(e) => return err(502, e),
    };

    let release = releases.get(release_idx.min(releases.len() - 1));
    let Some(release) = release else {
        return err(404, "release index out of range");
    };
    let mirror = release
        .mirrors
        .get(mirror_idx.min(release.mirrors.len().saturating_sub(1)))
        .cloned()
        .or_else(|| release.mirrors.first().cloned());
    let Some(mirror) = mirror else {
        return err(404, "release has no mirrors");
    };

    let is_dash = crate::player::is_dash_url(&mirror.resolver_url);
    let subtitle_url = req.query_param("caption").map(|s| s.to_string());
    let target = mirror.resolver_url.clone();
    let headers = mirror.headers.clone();

    // Kill any previous sidecar, then spawn a fresh one for this stream.
    super::kill_playback(state);

    let spawn = tokio::task::spawn_blocking(move || {
        let sub_ref = subtitle_url.as_deref();
        crate::proxy::spawn_sidecar(&target, &headers, sub_ref, max_height)
    })
    .await
    .map_err(|e| format!("spawn join failed: {e}"));

    match spawn {
        Ok(Ok((proxied_url, child))) => {
            // Record sidecar origin (scheme://host:port) for /play forwarding.
            let origin = proxied_url
                .split("/https/")
                .next()
                .or_else(|| proxied_url.split("/http/").next())
                .unwrap_or("")
                .to_string();
            if let Ok(mut g) = state.playback_child.lock() {
                *g = Some(child);
            }
            if let Ok(mut g) = state.playback_url.lock() {
                *g = Some(proxied_url.clone());
            }

            // Map: sidecar URL http://127.0.0.1:PORT/https/host/...
            //   -> app URL    /play/https/host/...
            let play_url = proxied_url
                .trim_start_matches("http://127.0.0.1:")
                .to_string();
            let play_url = play_url.split_once('/').map(|(_, rest)| format!("/play/{rest}"));

            ok(json!({
                "play_url": play_url.unwrap_or_else(|| "/play".to_string()),
                "dash": is_dash,
                "quality": release.quality,
                "codec": release.codec,
                "size_bytes": release.size_bytes,
                "label": release.filename,
                "provider": provider.cache_key(),
                "resource_id": release.resource_id,
                "origin": origin,
            }))
        }
        Ok(Err(e)) => err(502, format!("failed to start playback proxy: {e}")),
        Err(e) => err(500, e),
    }
}

/// Forward /play/<scheme>/<rest> to the active sidecar with Range support
/// and CORS headers. DASH manifests get their absolute sidecar URLs
/// rewritten to app-relative /play/ URLs so dash.js stays same-origin.
pub async fn proxy_play(req: &Request, state: &Arc<AppState>) -> Response {
    let sidecar_origin = state
        .playback_url
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .and_then(|url| {
            url.split_once("/https/")
                .map(|(o, _)| o.to_string())
                .or_else(|| url.split_once("/http/").map(|(o, _)| o.to_string()))
        });

    let Some(origin) = sidecar_origin else {
        return err(412, "no active playback — resolve a stream first");
    };

    // Preserve the exact signed query string from the raw request target.
    let suffix = &req.raw_target["/play/".len().min(req.raw_target.len())..];
    if suffix.is_empty() {
        return err(400, "empty proxy path");
    }
    let target = format!("{origin}/{suffix}");

    let range = req.header("range").map(|s| s.to_string());
    let mut forward = state.http.get(&target).timeout(std::time::Duration::from_secs(60));
    if let Some(r) = &range {
        forward = forward.header("Range", r);
    }

    let resp = match forward.send().await {
        Ok(r) => r,
        Err(e) => return err(502, format!("playback upstream failed: {e}")),
    };

    let status = resp.status().as_u16();
    let mut out_headers: Vec<(String, String)> = Vec::new();
    for name in ["content-type", "content-range", "content-length", "accept-ranges", "etag", "last-modified"] {
        if let Some(v) = resp.headers().get(name).and_then(|v| v.to_str().ok()) {
            out_headers.push((name.to_string(), v.to_string()));
        }
    }
    out_headers.push(("Access-Control-Allow-Origin".into(), "*".into()));

    let content_type = out_headers
        .iter()
        .find(|(n, _)| n == "content-type")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    let is_manifest = content_type.contains("dash+xml")
        || req.path.ends_with(".mpd")
        || content_type.contains("mpegurl")
        || req.path.ends_with(".m3u8");

    if is_manifest {
        // Buffer manifests (small) and rewrite absolute sidecar URLs.
        let body = match resp.bytes().await {
            Ok(b) => b.to_vec(),
            Err(e) => return err(502, format!("manifest fetch failed: {e}")),
        };
        let body = rewrite_manifest(&body, &origin);
        let mut r = Response::bytes(status, &content_type_or(&content_type, "application/dash+xml"), body);
        for (n, v) in out_headers {
            if n != "content-length" {
                r = r.header(&n, &v);
            }
        }
        r = r.header("Access-Control-Allow-Origin", "*");
        return r;
    }

    // Stream segments/media through with backpressure-friendly chunks.
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, String>>(8);
    tokio::spawn(async move {
        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    if tx.send(Ok(bytes.to_vec())).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(format!("upstream: {e}"))).await;
                    break;
                }
            }
        }
    });

    let mut r = Response::new(status);
    r.stream = Some(StreamSource {
        status,
        content_type: content_type_or(&content_type, "application/octet-stream"),
        headers: out_headers,
        chunks: rx,
    });
    r
}

fn content_type_or(ct: &str, fallback: &str) -> String {
    if ct.trim().is_empty() {
        fallback.to_string()
    } else {
        ct.to_string()
    }
}

fn rewrite_manifest(body: &[u8], origin: &str) -> Vec<u8> {
    let text = String::from_utf8_lossy(body);
    let needle = format!("{origin}/");
    if text.contains(&needle) {
        text.replace(&needle, "/play/").into_bytes()
    } else {
        body.to_vec()
    }
}

// ---------------------------------------------------------------------------
// Captions
// ---------------------------------------------------------------------------

pub async fn captions(req: &Request, state: &Arc<AppState>) -> Response {
    let subject_id = req.query_param("id").unwrap_or("");
    let resource_id = req.query_param("resource").unwrap_or("");
    let season: usize = req.query_param("season").and_then(|s| s.parse().ok()).unwrap_or(0);
    let episode: usize = req.query_param("episode").and_then(|s| s.parse().ok()).unwrap_or(0);
    let siblings: Vec<String> = req
        .query_param("siblings")
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    match state
        .service
        .get_ext_captions(subject_id, resource_id, &siblings, season, episode)
        .await
    {
        Ok(list) => ok(json!({ "captions": list })),
        Err(e) => err(502, format!("captions failed: {e}")),
    }
}

pub async fn caption_file(req: &Request, state: &Arc<AppState>) -> Response {
    let url = req.query_param("u").unwrap_or("");
    if url.is_empty() || !url.starts_with("http") {
        return err(400, "invalid caption url");
    }
    let resp = match state.http.get(url).send().await {
        Ok(r) => r,
        Err(e) => return err(502, format!("caption fetch failed: {e}")),
    };
    let body = match resp.bytes().await {
        Ok(b) => b.to_vec(),
        Err(e) => return err(502, format!("caption read failed: {e}")),
    };
    let lower = url.to_ascii_lowercase();
    // Normalize newlines, then detect SRT by "1\n00:00:..." numbering.
    let text = String::from_utf8_lossy(&body).replace("\r\n", "\n");
    let is_srt = lower.ends_with(".srt")
        || (!lower.ends_with(".vtt") && !text.starts_with("WEBVTT") && text.contains("\n00:"));
    if is_srt {
        let mut vtt = String::from("WEBVTT\n\n");
        for block in text.split("\n\n") {
            let block = block.trim();
            if block.is_empty() {
                continue;
            }
            let lines: Vec<&str> = block.lines().collect();
            let start_idx = if lines.first().is_some_and(|l| l.contains("-->")) {
                0
            } else {
                1
            };
            for line in lines.iter().skip(start_idx) {
                vtt.push_str(&line.replace(',', "."));
                vtt.push('\n');
            }
            vtt.push('\n');
        }
        return Response::bytes(200, "text/vtt; charset=utf-8", vtt.into_bytes())
            .header("Access-Control-Allow-Origin", "*");
    }
    Response::bytes(200, "text/vtt; charset=utf-8", text.into_bytes())
        .header("Access-Control-Allow-Origin", "*")
}

// ---------------------------------------------------------------------------
// Posters
// ---------------------------------------------------------------------------

pub async fn poster(req: &Request, state: &Arc<AppState>) -> Response {
    let url = req.query_param("u").unwrap_or("").to_string();
    if url.is_empty() || !url.starts_with("http") {
        return err(400, "invalid poster url");
    }
    // Optional width cap: downscale server-side to cut bandwidth, decode
    // time and memory on low-spec machines (GMA 3100, 3GB RAM).
    let width: u32 = req
        .query_param("w")
        .and_then(|w| w.parse().ok())
        .unwrap_or(0);

    // Disk cache key includes the requested width.
    let cache_ns = "gui-poster";
    let cache_id = format!("{}:{}", width, md5_hex(&url));
    let cache_id_lookup = cache_id.clone();

    if width >= 16 && width <= 1024 {
        if let Some(cached) =
            tokio::task::spawn_blocking(move || {
                crate::cache::get_namespaced_image_cache(cache_ns, &cache_id_lookup)
            })
            .await
            .ok()
            .flatten()
        {
            return Response::bytes(200, "image/jpeg", cached)
                .header("Cache-Control", "public, max-age=86400");
        }
    }

    match state.service.fetch_poster_bytes(&url).await {
        Some(bytes) => {
            let ct = if url.to_ascii_lowercase().contains(".png") {
                "image/png"
            } else if url.to_ascii_lowercase().contains(".webp") {
                "image/webp"
            } else {
                "image/jpeg"
            };

            if width >= 16 && width <= 1024 {
                let bytes_for_resize = bytes.clone();
                let cache_id_owned = cache_id.clone();
                let resized = tokio::task::spawn_blocking(move || {
                    use image::{ImageFormat, ImageReader};
                    let reader = ImageReader::new(std::io::Cursor::new(&bytes_for_resize))
                        .with_guessed_format()
                        .ok()?;
                    let img = reader.decode().ok()?;
                    let mut out = Vec::with_capacity(48 * 1024);
                    img.thumbnail(width, width * 3)
                        .write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Jpeg)
                        .ok()?;
                    Some(out)
                })
                .await
                .ok()
                .flatten();
                if let Some(jpeg) = resized {
                    // Persist the resized variant for instant future loads.
                    let persist_id = cache_id_owned;
                    let jpeg_for_cache = jpeg.clone();
                    tokio::task::spawn_blocking(move || {
                        crate::cache::set_namespaced_image_cache(cache_ns, &persist_id, &jpeg_for_cache);
                    });
                    return Response::bytes(200, "image/jpeg", jpeg)
                        .header("Cache-Control", "public, max-age=86400");
                }
            }
            // Fall through: serve the original bytes when no resize applies.
            Response::bytes(200, ct, bytes).header("Cache-Control", "public, max-age=86400")
        }
        None => err(502, "poster unavailable"),
    }
}

fn md5_hex(input: &str) -> String {
    use md5::Digest;
    let mut hasher = md5::Md5::new();
    hasher.update(input.as_bytes());
    let out = hasher.finalize();
    out.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------
// Library: favorites & history
// ---------------------------------------------------------------------------

pub fn favorites(req: &Request, state: &Arc<AppState>) -> Response {
    match req.method.as_str() {
        "GET" => match state.favorites.lock() {
            Ok(f) => ok(json!({ "items": f.items })),
            Err(_) => err(500, "favorites lock failed"),
        },
        "POST" => {
            let payload: serde_json::Value = match serde_json::from_slice(&req.body) {
                Ok(p) => p,
                Err(e) => return err(400, format!("bad json: {e}")),
            };
            let provider = payload.get("provider").and_then(|v| v.as_str()).unwrap_or("moviebox");
            let subject_id = payload.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let title = payload.get("title").and_then(|v| v.as_str()).unwrap_or("");
            if subject_id.is_empty() || title.is_empty() {
                return err(400, "missing id or title");
            }
            let item = crate::favorites::FavoriteItem {
                provider: provider.to_string(),
                subject_id: subject_id.to_string(),
                title: title.to_string(),
                cover_url: payload
                    .get("cover_url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                stype: payload.get("stype").and_then(|v| v.as_i64()).unwrap_or(1),
                release_year: payload
                    .get("year")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                added_at: crate::ai::engine::now_secs(),
            };
            match state.favorites.lock() {
                Ok(mut f) => {
                    let added = f.toggle(item);
                    f.save();
                    ok(json!({ "added": added, "count": f.items.len() }))
                }
                Err(_) => err(500, "favorites lock failed"),
            }
        }
        "DELETE" => {
            let id = req.query_param("id").unwrap_or("");
            let provider = req.query_param("provider").unwrap_or("moviebox");
            match state.favorites.lock() {
                Ok(mut f) => {
                    f.items.retain(|i| !(i.subject_id == id && i.provider.eq_ignore_ascii_case(provider)));
                    f.rebuild_index();
                    f.save();
                    ok(json!({ "count": f.items.len() }))
                }
                Err(_) => err(500, "favorites lock failed"),
            }
        }
        _ => err(405, "method not allowed"),
    }
}

pub fn history(req: &Request, state: &Arc<AppState>) -> Response {
    match req.method.as_str() {
        "GET" => match state.history.lock() {
            Ok(h) => ok(json!({ "items": h.recent })),
            Err(_) => err(500, "history lock failed"),
        },
        "DELETE" => {
            let id = req.query_param("id").unwrap_or("");
            let season: usize = req.query_param("season").and_then(|s| s.parse().ok()).unwrap_or(0);
            let episode: usize = req.query_param("episode").and_then(|s| s.parse().ok()).unwrap_or(0);
            match state.history.lock() {
                Ok(mut h) => {
                    if id.is_empty() {
                        h.clear();
                    } else {
                        h.remove(req.query_param("provider").unwrap_or("moviebox"), id, season, episode);
                    }
                    h.save();
                    ok(json!({ "ok": true }))
                }
                Err(_) => err(500, "history lock failed"),
            }
        }
        _ => err(405, "method not allowed"),
    }
}

pub fn history_progress(req: &Request, state: &Arc<AppState>) -> Response {
    if req.method != "POST" {
        return err(405, "method not allowed");
    }
    let payload: serde_json::Value = match serde_json::from_slice(&req.body) {
        Ok(p) => p,
        Err(e) => return err(400, format!("bad json: {e}")),
    };
    let provider = payload.get("provider").and_then(|v| v.as_str()).unwrap_or("moviebox").to_string();
    let subject_id = payload.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let title = payload.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if subject_id.is_empty() || title.is_empty() {
        return err(400, "missing id or title");
    }

    let stype = payload.get("stype").and_then(|v| v.as_i64()).unwrap_or(1);
    let item = crate::history::WatchHistoryItem {
        provider,
        subject_id: subject_id.clone(),
        title,
        cover_url: payload
            .get("cover_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        stype,
        release_year: payload
            .get("year")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        season: payload.get("season").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
        episode: payload.get("episode").and_then(|v| v.as_u64()).unwrap_or(1) as usize,
        timestamp: crate::ai::engine::now_secs(),
        duration_seconds: payload.get("duration_seconds").and_then(|v| v.as_u64()),
        progress_seconds: payload.get("progress_seconds").and_then(|v| v.as_u64()).unwrap_or(0),
        completed: payload.get("completed").and_then(|v| v.as_bool()).unwrap_or(false),
        stream_filename: None,
    };

    let progress = payload.get("progress_seconds").and_then(|v| v.as_u64()).unwrap_or(0);
    let duration = payload.get("duration_seconds").and_then(|v| v.as_u64());
    let completed = payload.get("completed").and_then(|v| v.as_bool()).unwrap_or(false);

    match state.history.lock() {
        Ok(mut h) => {
            h.update_progress(item, progress, duration, completed);
            ok(json!({ "ok": true }))
        }
        Err(_) => err(500, "history lock failed"),
    }
}

// ---------------------------------------------------------------------------
// GUI settings
// ---------------------------------------------------------------------------

pub fn settings(req: &Request, state: &Arc<AppState>) -> Response {
    match req.method.as_str() {
        "GET" => match state.gui_settings.lock() {
            Ok(s) => ok(json!({ "settings": s.clone() })),
            Err(_) => err(500, "settings lock failed"),
        },
        "POST" => {
            let patch: serde_json::Value = match serde_json::from_slice(&req.body) {
                Ok(p) => p,
                Err(e) => return err(400, format!("bad json: {e}")),
            };
            match state.gui_settings.lock() {
                Ok(mut s) => {
                    if let (Some(obj), Some(patch_obj)) = (s.as_object_mut(), patch.as_object()) {
                        for (k, v) in patch_obj {
                            obj.insert(k.clone(), v.clone());
                        }
                    }
                    ok(json!({ "settings": s.clone() }))
                }
                Err(_) => err(500, "settings lock failed"),
            }
        }
        _ => err(405, "method not allowed"),
    }
}
