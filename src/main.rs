#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod collectors;
mod config;
mod paths;
mod state;
mod theme;
mod tray;
mod ui;
mod units;
mod workers;

use std::sync::Mutex;

fn init_logging() {
    let file_name = format!("nexnwatch-{}.log", chrono::Local::now().format("%Y-%m-%d"));
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("nexnwatch=info,warn"));

    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths::logs_dir().join(file_name))
    {
        Ok(file) => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_ansi(false)
                .with_writer(Mutex::new(file))
                .init();
        }
        Err(_) => {
            tracing_subscriber::fmt().with_env_filter(filter).init();
        }
    }
    prune_old_logs();
}

/// 14 günden eski log dosyalarını sil.
fn prune_old_logs() {
    let Ok(entries) = std::fs::read_dir(paths::logs_dir()) else {
        return;
    };
    let limit = std::time::Duration::from_secs(14 * 24 * 3600);
    for e in entries.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| m.elapsed().ok())
            .is_some_and(|age| age > limit);
        if old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let msg = match info.payload().downcast_ref::<&str>() {
            Some(s) => s.to_string(),
            None => info
                .payload()
                .downcast_ref::<String>()
                .cloned()
                .unwrap_or_else(|| "bilinmeyen hata".into()),
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        tracing::error!("PANIC: {msg} @ {location}");
        // ETW oturumu açık kalmasın.
        collectors::etw::shutdown();
        workers::system::crash_dialog(&format!("{msg}\n({location})"));
        std::process::exit(1);
    }));
}

/// `--speedtest [auto|cloudflare|ookla]`: arayüz açmadan bir hız testi yapar,
/// sonucu JSON olarak yazar (teşhis ve CI doğrulaması için).
fn cli_speedtest() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    let i = args.iter().position(|a| a == "--speedtest")?;
    use workers::speedtest::{SpeedProvider, SpeedTester};
    let provider = match args.get(i + 1).map(|s| s.to_lowercase()).as_deref() {
        Some("cloudflare") => SpeedProvider::Cloudflare,
        Some("ookla") | Some("speedtest") => SpeedProvider::Ookla,
        _ => SpeedProvider::Auto,
    };
    match SpeedTester::new().run_blocking(provider) {
        Ok(r) => {
            println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default());
            Some(0)
        }
        Err(e) => {
            eprintln!("HATA: {e}");
            println!("HATA: {e}");
            Some(1)
        }
    }
}

/// LAN testi komut satırı:
///   `--lan-server`                                   sunucu modunda bekler
///   `--lan-test <ip[:port]> [up|down|bidir|udp] [sn] [akış] [udp_mbps]`
///   `--share-test <klasör veya \\sunucu\paylasim> [MB]`
fn cli_lan() -> Option<i32> {
    use workers::lan::{LanMode, LanTester, PORT};
    let args: Vec<String> = std::env::args().collect();
    let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
    let t = LanTester::new();
    if let Some(i) = args.iter().position(|a| a == "--lan-server") {
        let secs: u64 = arg(i + 1).parse().unwrap_or(0);
        if let Err(e) = t.set_server(true) {
            println!("HATA: {e}");
            return Some(1);
        }
        println!("LAN sunucusu TCP {PORT} üzerinde bekliyor");
        let until = std::time::Instant::now()
            + std::time::Duration::from_secs(if secs == 0 { u64::MAX / 4 } else { secs });
        while std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        return Some(0);
    }
    if let Some(i) = args.iter().position(|a| a == "--lan-test") {
        let host = arg(i + 1);
        let target: std::net::SocketAddr = host
            .parse()
            .or_else(|_| format!("{host}:{PORT}").parse())
            .or_else(|_| {
                use std::net::ToSocketAddrs;
                (host.as_str(), PORT)
                    .to_socket_addrs()
                    .map_err(|_| ())
                    .and_then(|mut a| a.next().ok_or(()))
            })
            .ok()?;
        let mode = match arg(i + 2).as_str() {
            "up" => LanMode::Upload,
            "down" => LanMode::Download,
            "udp" => LanMode::Udp,
            _ => LanMode::Bidir,
        };
        let secs = arg(i + 3).parse().unwrap_or(10);
        let streams = arg(i + 4).parse().unwrap_or(4);
        let rate = arg(i + 5).parse().unwrap_or(100.0);
        return Some(match t.run_test(target, mode, secs, streams, rate) {
            Ok(r) => {
                println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default());
                0
            }
            Err(e) => {
                println!("HATA: {e}");
                1
            }
        });
    }
    if let Some(i) = args.iter().position(|a| a == "--share-test") {
        let mb = arg(i + 2).parse().unwrap_or(256);
        return Some(match t.run_share_test(&arg(i + 1), mb) {
            Ok(r) => {
                println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default());
                0
            }
            Err(e) => {
                println!("HATA: {e}");
                1
            }
        });
    }
    None
}

fn main() -> iced::Result {
    if let Some(code) = cli_speedtest().or_else(cli_lan) {
        std::process::exit(code);
    }
    init_logging();
    install_panic_hook();
    tracing::info!("NexNWatch {} başlatılıyor", env!("CARGO_PKG_VERSION"));
    let result = app::run();
    collectors::etw::shutdown();
    result
}
