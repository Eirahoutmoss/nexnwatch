//! Merkezi uygulama durumu, mesajlar ve tick mantığı.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use iced::{application, window, Subscription, Task, Theme};

use crate::collectors::etw::{self, EtwHandle};
use crate::collectors::{nic, process};
use crate::config::{Config, ThemeMode};
use crate::state::rolling_window::{RollingWindow, Sample};
use crate::state::traffic_map::TrafficMap;
use crate::state::usage_store::UsageStore;
use crate::theme;
use crate::units::DisplayUnit;
use crate::workers::speedtest::SpeedTester;
use crate::workers::system;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Dashboard,
    Adapters,
    Processes,
    Speed,
    Reports,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcSort {
    Traffic,
    Rate,
    Name,
    Cpu,
    Memory,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick(Instant),
    Navigate(Page),
    SelectAdapter(Option<u32>),
    RefreshAdapters,
    ToggleVirtualGroup,
    SetUnit(DisplayUnit),
    SetChartRange(u64),
    SpeedTest,
    ProcessSearch(String),
    SelectRoot(u32),
    AutoRoot,
    SetProcSort(ProcSort),
    SetTick(u64),
    SetSpeedAuto(bool),
    SetSpeedInterval(u64),
    SetWindow(u64),
    SetTheme(ThemeMode),
    SetPersist(bool),
    SetAutostart(bool),
    OpenDataFolder,
    DuplexLoaded(HashMap<u32, bool>),
    DismissNotice,
    CloseRequested,
}

/// Bir adaptörün anlık hızı (sidebar ve adaptör sayfası için).
#[derive(Debug, Clone, Copy, Default)]
pub struct Rate {
    pub rx: f64,
    pub tx: f64,
}

pub struct App {
    pub cfg: Config,
    pub page: Page,

    // --- adaptörler
    pub adapters: Vec<nic::AdapterInfo>,
    /// None = tüm fiziksel adaptörler.
    pub selected: Option<u32>,
    pub duplex: HashMap<u32, bool>,
    pub rates: HashMap<u32, Rate>,
    prev_counters: HashMap<u32, (u64, u64)>,
    last_counter_at: Option<Instant>,
    last_adapter_refresh: Instant,

    // --- seçili adaptör trafiği
    pub rolling: RollingWindow,
    pub history: VecDeque<(f64, f64)>,
    pub rx_speed: f64,
    pub tx_speed: f64,
    pub chart_secs: u64,
    prev_selected_raw: Option<(u64, u64)>,
    /// Hız testi sırasında pencere toplamlarından düşülen trafik.
    excluded: (u64, u64),
    pub excluded_total: u64,
    pub started: Instant,

    // --- process + ETW
    process_system: sysinfo::System,
    pub processes: Vec<process::ProcessInfo>,
    pub proc_index: HashMap<u32, usize>,
    last_process_scan: Option<Instant>,
    pub etw: EtwHandle,
    pub traffic: TrafficMap,
    pub root: Option<(u32, u64)>,
    pub search: String,
    pub sort: ProcSort,

    // --- hız testi
    pub speed: SpeedTester,
    last_auto_check: Instant,

    // --- kalıcı sayaç
    pub usage: UsageStore,
    last_usage_save: Instant,

    pub notice: Option<String>,
}

pub const ALL_LABEL: &str = "Tüm Adaptörler";

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let cfg = Config::load();
        theme::set_light(cfg.theme == ThemeMode::Light);

        let adapters = nic::list_adapters().unwrap_or_else(|e| {
            tracing::error!("Adaptör listesi alınamadı: {e}");
            Vec::new()
        });
        let selected = cfg
            .last_adapter
            .as_ref()
            .and_then(|name| adapters.iter().find(|a| &a.name == name))
            .or_else(|| {
                adapters.iter().find(|a| a.status == nic::AdapterStatus::Up && !a.kind.is_virtual())
            })
            .map(|a| a.index);

        let mut process_system = sysinfo::System::new();
        let processes = process::snapshot(&mut process_system);
        let window = Duration::from_secs(cfg.window_minutes * 60);

        let mut app = Self {
            page: Page::Dashboard,
            adapters,
            selected,
            duplex: HashMap::new(),
            rates: HashMap::new(),
            prev_counters: HashMap::new(),
            last_counter_at: None,
            last_adapter_refresh: Instant::now(),
            rolling: RollingWindow::new(window),
            history: VecDeque::new(),
            rx_speed: 0.0,
            tx_speed: 0.0,
            chart_secs: 60,
            prev_selected_raw: None,
            excluded: (0, 0),
            excluded_total: 0,
            started: Instant::now(),
            process_system,
            processes: Vec::new(),
            proc_index: HashMap::new(),
            last_process_scan: None,
            etw: etw::start(),
            traffic: TrafficMap::default(),
            root: None,
            search: String::new(),
            sort: ProcSort::Traffic,
            speed: SpeedTester::new(),
            last_auto_check: Instant::now(),
            usage: UsageStore::load(),
            last_usage_save: Instant::now(),
            notice: None,
            cfg,
        };
        app.set_processes(processes);

        // Duplex bilgisi (PowerShell) arka planda.
        let duplex = Task::perform(
            async { tokio::task::spawn_blocking(nic::duplex_map).await.unwrap_or_default() },
            Message::DuplexLoaded,
        );
        (app, duplex)
    }

    pub fn selected_adapter(&self) -> Option<&nic::AdapterInfo> {
        self.selected.and_then(|i| self.adapters.iter().find(|a| a.index == i))
    }

    pub fn selected_label(&self) -> String {
        self.selected_adapter().map(|a| a.name.clone()).unwrap_or_else(|| ALL_LABEL.into())
    }

    pub fn tick_period(&self) -> Duration {
        Duration::from_millis(self.cfg.tick_ms)
    }

    /// Grafikte gösterilecek nokta sayısı.
    pub fn chart_points(&self) -> usize {
        ((self.chart_secs * 1000) / self.cfg.tick_ms.max(1)).max(2) as usize
    }

    fn history_cap(&self) -> usize {
        ((self.cfg.window_minutes * 60 * 1000) / self.cfg.tick_ms.max(1)) as usize
    }

    fn set_processes(&mut self, list: Vec<process::ProcessInfo>) {
        self.proc_index = list.iter().enumerate().map(|(i, p)| (p.pid, i)).collect();
        self.processes = list;
        // Seçili kök öldüyse ya da PID başka process'e geçtiyse bırak.
        if let Some((pid, start)) = self.root {
            let ok = self.process(pid).is_some_and(|p| p.start_time == start);
            if !ok {
                self.root = None;
            }
        }
    }

    pub fn process(&self, pid: u32) -> Option<&process::ProcessInfo> {
        self.proc_index.get(&pid).and_then(|i| self.processes.get(*i))
    }

    fn reset_selected_stream(&mut self) {
        self.rolling.clear();
        self.history.clear();
        self.rx_speed = 0.0;
        self.tx_speed = 0.0;
        self.prev_selected_raw = None;
        self.excluded = (0, 0);
    }

    fn refresh_adapters(&mut self) {
        match nic::list_adapters() {
            Ok(list) => {
                self.adapters = list;
                if let Some(i) = self.selected {
                    if !self.adapters.iter().any(|a| a.index == i) {
                        self.notice = Some("Seçili adaptör kayboldu; tüm adaptörlere geçildi.".into());
                        self.selected = None;
                        self.reset_selected_stream();
                    }
                }
            }
            Err(e) => tracing::warn!("Adaptör yenileme hatası: {e}"),
        }
        self.last_adapter_refresh = Instant::now();
    }

    fn on_tick(&mut self, now: Instant) {
        let speedtest_running = self.speed.is_running();

        // ---- 1) NIC sayaçları (tek GetIfTable2 çağrısı)
        match nic::counters() {
            Ok(rows) => {
                let elapsed = self
                    .last_counter_at
                    .map(|t| now.duration_since(t).as_secs_f64())
                    .unwrap_or(0.0);
                self.last_counter_at = Some(now);

                let mut physical_delta = (0u64, 0u64);
                let mut new_prev = HashMap::with_capacity(rows.len());
                for r in &rows {
                    if let Some(&(prx, ptx)) = self.prev_counters.get(&r.index) {
                        let (drx, dtx) = (r.rx.saturating_sub(prx), r.tx.saturating_sub(ptx));
                        if elapsed > 0.0 {
                            self.rates.insert(r.index, Rate { rx: drx as f64 / elapsed, tx: dtx as f64 / elapsed });
                        }
                        if !r.is_virtual && r.rx >= prx && r.tx >= ptx {
                            physical_delta.0 += drx;
                            physical_delta.1 += dtx;
                        }
                    }
                    new_prev.insert(r.index, (r.rx, r.tx));
                }
                self.prev_counters = new_prev;

                if self.cfg.persist_usage {
                    self.usage.add(physical_delta.0, physical_delta.1);
                }

                // Seçili kaynak: tek adaptör ya da fiziksel adaptörlerin toplamı.
                let raw = match self.selected {
                    Some(i) => rows.iter().find(|r| r.index == i).map(|r| (r.rx, r.tx)),
                    None => Some(rows.iter().filter(|r| !r.is_virtual).fold((0u64, 0u64), |a, r| {
                        (a.0.saturating_add(r.rx), a.1.saturating_add(r.tx))
                    })),
                };

                if let Some((rx, tx)) = raw {
                    if let Some((prx, ptx)) = self.prev_selected_raw {
                        if rx < prx || tx < ptx {
                            // Adaptör sıfırlandı (yeniden başlatma / sürücü reset).
                            tracing::warn!("Adaptör sayacı sıfırlandı; pencere temizleniyor");
                            self.notice = Some("Adaptör sayacı sıfırlandı — pencere toplamları yeniden başladı.".into());
                            self.rolling.clear();
                            self.excluded = (0, 0);
                        } else {
                            let (drx, dtx) = (rx - prx, tx - ptx);
                            if elapsed > 0.0 {
                                self.rx_speed = drx as f64 / elapsed;
                                self.tx_speed = dtx as f64 / elapsed;
                            }
                            if speedtest_running {
                                self.excluded.0 += drx;
                                self.excluded.1 += dtx;
                                self.excluded_total += drx + dtx;
                            }
                            self.history.push_back((self.rx_speed, self.tx_speed));
                            let cap = self.history_cap();
                            while self.history.len() > cap {
                                self.history.pop_front();
                            }
                        }
                    }
                    self.prev_selected_raw = Some((rx, tx));
                    self.rolling.push(Sample {
                        at: now,
                        rx_bytes: rx.saturating_sub(self.excluded.0),
                        tx_bytes: tx.saturating_sub(self.excluded.1),
                    });
                }
            }
            Err(e) => tracing::warn!("Sayaç okunamadı: {e}"),
        }

        // ---- 2) Adaptör listesi (durum/IP değişiklikleri) 10 sn'de bir
        if now.duration_since(self.last_adapter_refresh) >= Duration::from_secs(10) {
            self.refresh_adapters();
        }

        // ---- 3) Process taraması 2 sn'de bir
        if self.last_process_scan.is_none_or(|t| now.duration_since(t) >= Duration::from_secs(2)) {
            let list = process::snapshot(&mut self.process_system);
            self.set_processes(list);
            self.last_process_scan = Some(now);
        }

        // ---- 4) ETW → PID trafik
        let alive: HashMap<u32, u64> = self.processes.iter().map(|p| (p.pid, p.start_time)).collect();
        let dead = self.traffic.update(now, &self.etw.snapshot(), &alive);
        if !dead.is_empty() {
            self.etw.forget(&dead);
        }

        // ---- 5) Otomatik hız testi
        if self.cfg.speedtest_auto && now.duration_since(self.last_auto_check) >= Duration::from_secs(5) {
            self.last_auto_check = now;
            let interval = self.cfg.speedtest_interval_min as i64 * 60;
            let due = match self.speed.last() {
                Some(last) => chrono::Local::now().timestamp() - last.timestamp >= interval,
                // İlk test: açılıştan 30 sn sonra (ölçümler otursun).
                None => now.duration_since(self.started) >= Duration::from_secs(30),
            };
            if due && !speedtest_running {
                self.speed.start(true);
            }
        }

        // ---- 6) Kalıcı sayaç: dakikada bir diske
        if now.duration_since(self.last_usage_save) >= Duration::from_secs(60) {
            self.usage.save();
            self.last_usage_save = now;
        }
    }

    fn shutdown(&mut self) {
        tracing::info!("Kapatılıyor");
        self.usage.save();
        self.cfg.save();
        etw::shutdown();
    }
}

pub fn run() -> iced::Result {
    application(App::new, update, crate::ui::view)
        .title("NexNWatch | Gerçek Zamanlı Ağ İzleme")
        .subscription(subscription)
        .theme(|_: &App| if theme::is_light() { Theme::Light } else { Theme::Dark })
        .exit_on_close_request(false)
        .window(window::Settings {
            size: iced::Size::new(1480.0, 940.0),
            min_size: Some(iced::Size::new(1180.0, 720.0)),
            ..Default::default()
        })
        .run()
}

fn subscription(app: &App) -> Subscription<Message> {
    Subscription::batch([
        iced::time::every(app.tick_period()).map(Message::Tick),
        window::close_requests().map(|_| Message::CloseRequested),
    ])
}

fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Tick(now) => app.on_tick(now),
        Message::Navigate(page) => app.page = page,
        Message::SelectAdapter(index) => {
            if app.selected != index {
                app.selected = index;
                app.reset_selected_stream();
                app.cfg.last_adapter = app.selected_adapter().map(|a| a.name.clone());
                app.cfg.save();
            }
        }
        Message::RefreshAdapters => app.refresh_adapters(),
        Message::ToggleVirtualGroup => {
            app.cfg.virtual_group_open = !app.cfg.virtual_group_open;
            app.cfg.save();
        }
        Message::SetUnit(u) => {
            app.cfg.unit = u;
            app.cfg.save();
        }
        Message::SetChartRange(secs) => app.chart_secs = secs,
        Message::SpeedTest => app.speed.start(false),
        Message::ProcessSearch(s) => app.search = s,
        Message::SelectRoot(pid) => {
            if let Some(p) = app.process(pid) {
                app.root = Some((p.pid, p.start_time));
            }
        }
        Message::AutoRoot => app.root = None,
        Message::SetProcSort(s) => app.sort = s,
        Message::SetTick(ms) => {
            app.cfg.tick_ms = ms;
            app.history.clear();
            app.cfg.save();
        }
        Message::SetSpeedAuto(v) => {
            app.cfg.speedtest_auto = v;
            app.cfg.save();
        }
        Message::SetSpeedInterval(m) => {
            app.cfg.speedtest_interval_min = m;
            app.cfg.save();
        }
        Message::SetWindow(m) => {
            app.cfg.window_minutes = m;
            app.rolling.set_capacity(Duration::from_secs(m * 60));
            app.cfg.save();
        }
        Message::SetTheme(t) => {
            app.cfg.theme = t;
            theme::set_light(t == ThemeMode::Light);
            app.cfg.save();
        }
        Message::SetPersist(v) => {
            app.cfg.persist_usage = v;
            app.cfg.save();
        }
        Message::SetAutostart(v) => match system::set_autostart(v) {
            Ok(()) => {
                app.cfg.autostart = v;
                app.cfg.save();
                app.notice = Some(if v {
                    "Oturum açılışında yönetici olarak başlayacak (Görev Zamanlayıcı).".into()
                } else {
                    "Otomatik başlatma kapatıldı.".into()
                });
            }
            Err(e) => app.notice = Some(format!("Otomatik başlatma ayarlanamadı: {e}")),
        },
        Message::OpenDataFolder => {
            let dir = crate::paths::app_dir();
            #[cfg(windows)]
            let _ = std::process::Command::new("explorer.exe").arg(dir).spawn();
            #[cfg(not(windows))]
            let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
        }
        Message::DuplexLoaded(map) => app.duplex = map,
        Message::DismissNotice => app.notice = None,
        Message::CloseRequested => {
            app.shutdown();
            return iced::exit();
        }
    }
    Task::none()
}
