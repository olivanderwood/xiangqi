//! Windows-only build script (spec 9.2): embed `assets/icon.ico` and the
//! version info from Cargo.toml into `xiangqi.exe` via `winresource`.
//! On other platforms this is a no-op (the `CARGO_CFG_TARGET_OS`
//! check below); `winresource` itself compiles everywhere. `rc.exe`
//! from the Windows SDK is needed when building on Windows (MSVC).

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("FileDescription", "Cờ Tướng AI — game cờ tướng với AI");
        res.set("ProductName", "Cờ Tướng AI");
        res.compile().unwrap();
    }
}
