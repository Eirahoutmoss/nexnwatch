use serde::{Deserialize, Serialize};

use crate::paths;
use crate::units::DisplayUnit;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Ölçüm aralığı (milisaniye).
    pub tick_ms: u64,
    /// Otomatik hız testi açık mı.
    pub speedtest_auto: bool,
    /// Otomatik hız testi aralığı (dakika).
    pub speedtest_interval_min: u64,
    /// Rolling window ve grafik geçmişi (dakika).
    pub window_minutes: u64,
    /// Hız gösterim birimi.
    pub unit: DisplayUnit,
    pub theme: ThemeMode,
    /// Günlük / haftalık / aylık sayaçları diske yaz.
    pub persist_usage: bool,
    /// Windows oturum açılışında başlat (Görev Zamanlayıcı, en yüksek yetki).
    pub autostart: bool,
    /// Sanal adaptör grubu açık mı.
    pub virtual_group_open: bool,
    /// Son seçilen adaptör (arayüz adı).
    pub last_adapter: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tick_ms: 1000,
            speedtest_auto: true,
            speedtest_interval_min: 5,
            window_minutes: 10,
            unit: DisplayUnit::Auto,
            theme: ThemeMode::Dark,
            persist_usage: true,
            autostart: false,
            virtual_group_open: false,
            last_adapter: None,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = paths::config_file();
        match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<Config>(&text) {
                Ok(mut cfg) => {
                    cfg.sanitize();
                    cfg
                }
                Err(err) => {
                    tracing::warn!("config.toml okunamadı, varsayılanlar kullanılıyor: {err}");
                    Config::default()
                }
            },
            Err(_) => {
                let cfg = Config::default();
                cfg.save();
                cfg
            }
        }
    }

    pub fn save(&self) {
        match toml::to_string_pretty(self) {
            Ok(text) => {
                if let Err(err) = paths::write_atomic(&paths::config_file(), text.as_bytes()) {
                    tracing::warn!("config.toml yazılamadı: {err}");
                }
            }
            Err(err) => tracing::warn!("config serileştirilemedi: {err}"),
        }
    }

    fn sanitize(&mut self) {
        self.tick_ms = self.tick_ms.clamp(250, 10_000);
        self.speedtest_interval_min = self.speedtest_interval_min.clamp(1, 24 * 60);
        self.window_minutes = self.window_minutes.clamp(10, 120);
    }
}
