//! Uygulama veri klasörü: %APPDATA%\NexNWatch (Windows) veya ~/.config/nexnwatch.

use std::path::PathBuf;

pub fn app_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("NexNWatch");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn logs_dir() -> PathBuf {
    let dir = app_dir().join("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn config_file() -> PathBuf {
    app_dir().join("config.toml")
}

pub fn usage_file() -> PathBuf {
    app_dir().join("usage.json")
}

pub fn speedtest_file() -> PathBuf {
    app_dir().join("speedtest_history.json")
}

/// JSON dosyasını önce geçici dosyaya yazıp sonra yerine taşır; yarım yazılmış
/// dosya bırakmaz.
pub fn write_atomic(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}
