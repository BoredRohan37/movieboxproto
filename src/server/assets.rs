//! Embedded GUI assets. Compiled into the binary via include_bytes! so the
//! release ships as a single self-contained executable.

use super::http::{Request, Response};

const INDEX_HTML: &str = include_str!("../../gui/index.html");
const APP_JS: &str = include_str!("../../gui/app.js");
const VIEWS_JS: &str = include_str!("../../gui/views.js");
const STYLE_CSS: &str = include_str!("../../gui/style.css");
const ICON_SVG: &str = include_str!("../../gui/icon.svg");
const DASH_JS: &str = include_str!("../../gui/dash.min.js");
const ICON_PNG: &[u8] = include_bytes!("../../gui/icon-192.png");

pub fn init() {
    // Reserved for future asset pre-warming; kept as a stable init hook.
}

fn cacheable(req: &Request, seconds: u32) -> Response {
    let _ = req;
    // Small immutable assets get light caching; API never cached.
    let mut r = Response::new(200);
    r.headers.push((
        "Cache-Control".into(),
        format!("public, max-age={seconds}"),
    ));
    r
}

pub fn index(req: &Request) -> Response {
    let mut r = Response::html(200, INDEX_HTML.as_bytes().to_vec());
    r.headers.push(("Cache-Control".into(), "no-cache".into()));
    let _ = cacheable(req, 0);
    r
}

pub fn asset(req: &Request) -> Response {
    let name = req.path.trim_start_matches("/assets/");
    match name {
        "app.js" => {
            let mut r = Response::bytes(200, "application/javascript; charset=utf-8", APP_JS.as_bytes().to_vec());
            r.headers.push(("Cache-Control".into(), "no-cache".into()));
            r
        }
        "views.js" => {
            let mut r = Response::bytes(200, "application/javascript; charset=utf-8", VIEWS_JS.as_bytes().to_vec());
            r.headers.push(("Cache-Control".into(), "no-cache".into()));
            r
        }
        "style.css" => {
            let mut r = Response::bytes(200, "text/css; charset=utf-8", STYLE_CSS.as_bytes().to_vec());
            r.headers.push(("Cache-Control".into(), "no-cache".into()));
            r
        }
        "icon.svg" => Response::bytes(
            200,
            "image/svg+xml",
            ICON_SVG.as_bytes().to_vec(),
        ),
        "icon-192.png" => Response::bytes(200, "image/png", ICON_PNG.to_vec()),
        "dash.min.js" => {
            let mut r = Response::bytes(
                200,
                "application/javascript; charset=utf-8",
                DASH_JS.as_bytes().to_vec(),
            );
            r.headers
                .push(("Cache-Control".into(), "public, max-age=604800".into()));
            r
        }
        _ => Response::json_error(404, "asset not found"),
    }
}

pub fn favicon(req: &Request) -> Response {
    let _ = req;
    Response::bytes(200, "image/png", ICON_PNG.to_vec()).header("Cache-Control", "public, max-age=86400")
}

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
