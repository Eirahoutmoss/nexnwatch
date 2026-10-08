//! Sisteme dokunan yardımcılar: başlangıçta çalıştırma, çökme bildirimi.

/// Windows oturum açılışında başlat. Uygulama yönetici manifesti taşıdığı için
/// klasik Run anahtarı UAC'a takılır; bu yüzden "en yüksek yetki" ile çalışan
/// bir Görev Zamanlayıcı görevi kullanılır.
#[cfg(windows)]
pub fn set_autostart(enable: bool) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new("schtasks.exe");
    if enable {
        cmd.args([
            "/Create",
            "/F",
            "/TN",
            "NexNWatch",
            "/SC",
            "ONLOGON",
            "/RL",
            "HIGHEST",
            "/TR",
            &format!("\"{}\" --minimized", exe.display()),
        ]);
    } else {
        cmd.args(["/Delete", "/F", "/TN", "NexNWatch"]);
    }
    let out = cmd
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() || !enable {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(not(windows))]
pub fn set_autostart(_enable: bool) -> Result<(), String> {
    Err("Yalnızca Windows'ta desteklenir".into())
}

/// Panic durumunda kullanıcıya bilgi verir; "Evet" derse uygulamayı yeniden başlatır.
#[cfg(windows)]
pub fn crash_dialog(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IDYES, MB_ICONERROR, MB_TOPMOST, MB_YESNO, MessageBoxW,
    };
    let text = format!(
        "NexNWatch beklenmeyen bir hatayla karşılaştı:\n\n{message}\n\nAyrıntılar: %APPDATA%\\NexNWatch\\logs\n\nUygulama yeniden başlatılsın mı?"
    );
    let wide = |s: &str| {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };
    let (t, c) = (wide(&text), wide("NexNWatch — Hata"));
    let answer = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            t.as_ptr(),
            c.as_ptr(),
            MB_YESNO | MB_ICONERROR | MB_TOPMOST,
        )
    };
    if answer == IDYES
        && let Ok(exe) = std::env::current_exe()
    {
        let _ = std::process::Command::new(exe).spawn();
    }
}

#[cfg(not(windows))]
pub fn crash_dialog(message: &str) {
    eprintln!("NexNWatch çöktü: {message}");
}
