//! MovieBox GUI server binary.
//!
//! Serves the OTT web app + JSON API on 127.0.0.1 and opens the default
//! browser. One lightweight process — ideal for Windows 7/8.1 hardware.

use moviebox_tui::logging;
use moviebox_tui::server;

#[tokio::main]
async fn main() {
    moviebox_tui::logging::init();

    // Playback sidecar mode: re-invoked by spawn_sidecar with a target URL.
    // Prints "PORT <n>" on stdout exactly like the upstream TUI expects.
    {
        let args: Vec<String> = std::env::args().collect();
        if let Some(pos) = args.iter().position(|a| a == "--proxy-for-vlc") {
            let target_url = args.get(pos + 1).cloned().unwrap_or_default();
            let headers_json = args
                .get(pos + 2)
                .cloned()
                .unwrap_or_else(|| "[]".to_string());
            let sub_url = args.get(pos + 3).cloned().filter(|s| !s.is_empty());
            let max_height = args
                .get(pos + 4)
                .and_then(|s| s.parse::<u64>().ok())
                .filter(|&h| h > 0);
            let headers: Vec<(String, String)> =
                serde_json::from_str(&headers_json).unwrap_or_default();
            moviebox_tui::proxy::run_sidecar(target_url, headers, sub_url, max_height).await;
            return;
        }
    }

    let mut port: u16 = 7676;
    let mut open_browser = true;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                if let Some(p) = args.next().and_then(|v| v.parse().ok()) {
                    port = p;
                }
            }
            "--no-browser" => open_browser = false,
            "--help" | "-h" => {
                println!("MovieBox GUI server");
                println!("Usage: moviebox-server [--port N] [--no-browser]");
                println!("Serves the MovieBox OTT app at http://127.0.0.1:PORT/");
                std::process::exit(0);
            }
            _ => {}
        }
    }

    eprintln!(
        "MovieBox v{} — starting GUI server…",
        server::assets::APP_VERSION
    );
    if let Err(e) = server::run(port, open_browser).await {
        eprintln!("fatal: {e}");
        std::process::exit(1);
    }
}
