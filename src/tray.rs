//! Sistem tepsisi simgesi + Windows bildirimleri.
//!
//! Tepsi simgesi ana iş parçacığında oluşturulur (Win32 mesaj döngüsü iced/winit
//! tarafından pompalanır). Olaylar global kanallardan okunduğu için `poll()`
//! hangi iş parçacığından çağrılırsa çağrılsın çalışır.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum TrayAction {
    Show,
    SpeedTest,
    Quit,
}

#[cfg(windows)]
mod imp {
    use super::TrayAction;
    use std::cell::RefCell;
    use std::sync::OnceLock;

    use tray_icon::menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{
        Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    };

    thread_local! {
        static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
    }

    struct Ids {
        show: String,
        speed: String,
        quit: String,
    }
    static IDS: OnceLock<Ids> = OnceLock::new();

    pub fn init() {
        let rgba = include_bytes!("../assets/icon-32.rgba").to_vec();
        let icon = match Icon::from_rgba(rgba, 32, 32) {
            Ok(i) => i,
            Err(e) => {
                tracing::warn!("Tepsi ikonu oluşturulamadı: {e}");
                return;
            }
        };
        let menu = Menu::new();
        let show = MenuItem::new("NexNWatch'u Göster", true, None);
        let speed = MenuItem::new("Hız Testi Başlat", true, None);
        let quit = MenuItem::new("Çıkış", true, None);
        let sep = PredefinedMenuItem::separator();
        let items: [&dyn IsMenuItem; 4] = [&show, &speed, &sep, &quit];
        let _ = menu.append_items(&items);
        let _ = IDS.set(Ids {
            show: show.id().0.clone(),
            speed: speed.id().0.clone(),
            quit: quit.id().0.clone(),
        });

        match TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip("NexNWatch")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
        {
            Ok(tray) => TRAY.with(|t| *t.borrow_mut() = Some(tray)),
            Err(e) => tracing::warn!("Tepsi simgesi oluşturulamadı: {e}"),
        }
    }

    pub fn available() -> bool {
        IDS.get().is_some()
    }

    pub fn poll() -> Vec<TrayAction> {
        let mut out = Vec::new();
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            match ev {
                TrayIconEvent::DoubleClick { .. } => out.push(TrayAction::Show),
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => out.push(TrayAction::Show),
                _ => {}
            }
        }
        if let Some(ids) = IDS.get() {
            while let Ok(ev) = MenuEvent::receiver().try_recv() {
                let id = ev.id.0;
                if id == ids.show {
                    out.push(TrayAction::Show);
                } else if id == ids.speed {
                    out.push(TrayAction::SpeedTest);
                } else if id == ids.quit {
                    out.push(TrayAction::Quit);
                }
            }
        }
        out
    }

    pub fn set_tooltip(text: &str) {
        TRAY.with(|t| {
            if let Some(tray) = t.borrow().as_ref() {
                let _ = tray.set_tooltip(Some(text));
            }
        });
    }

    pub fn notify(title: &str, body: &str) {
        let (title, body) = (title.to_string(), body.to_string());
        std::thread::spawn(move || {
            use tauri_winrt_notification::Toast;
            if let Err(e) = Toast::new(Toast::POWERSHELL_APP_ID)
                .title(&title)
                .text1(&body)
                .show()
            {
                tracing::warn!("Bildirim gösterilemedi: {e:?}");
            }
        });
    }
}

#[cfg(not(windows))]
mod imp {
    use super::TrayAction;

    pub fn init() {}
    pub fn available() -> bool {
        false
    }
    pub fn poll() -> Vec<TrayAction> {
        Vec::new()
    }
    pub fn set_tooltip(_text: &str) {}
    pub fn notify(title: &str, body: &str) {
        tracing::info!("Bildirim: {title} — {body}");
    }
}

pub use imp::{available, init, notify, poll, set_tooltip};
