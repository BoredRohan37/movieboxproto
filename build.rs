//! Build script: embeds the MovieBox icon and version metadata into the
//! Windows executables (both TUI and GUI server). Skipped gracefully when
//! the resource compiler isn't available (e.g. Linux cross builds).

fn main() {
    println!("cargo:rerun-if-changed=gui/moviebox.ico");
    println!("cargo:rerun-if-changed=build.rs");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    match winresource::WindowsResource::new()
        .set_icon("gui/moviebox.ico")
        .set("FileDescription", "MovieBox — your private cinema")
        .set("ProductName", "MovieBox")
        .set("LegalCopyright", "MIT OR Apache-2.0")
        .set("OriginalFilename", "moviebox.exe")
        .compile()
    {
        Ok(()) => {}
        Err(e) => {
            println!("cargo:warning=Windows icon embedding skipped: {e}");
        }
    }
}
