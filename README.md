# NexNWatch

Windows 10/11 x64 için Nex ailesinin gerçek zamanlı ağ izleme ve process bazlı trafik analizi uygulaması.

## Özellikler (v0.3.1)

**Ağ adaptörleri**
- `GetIfTable2` ile NIC keşfi; loopback, tünel, WAN Miniport ve filtre sürücüsü kopyaları (WFP, QoS, Npcap…) elenir
- MAC, IPv4/IPv6 (önek ile), ağ geçidi, DNS (`GetAdaptersAddresses`), link hızı (↓/↑), duplex, MTU
- **Sanal adaptörler ayrı, katlanabilir bir grupta**: VPN, Hyper-V, WSL/Docker, VMware, VirtualBox, Wi-Fi Direct
- "Tüm Adaptörler" = yalnızca fiziksel adaptörlerin toplamı (VPN trafiği iki kez sayılmaz)
- Adaptör sayacı sıfırlanırsa pencere temizlenir ve kullanıcıya bildirilir

**Trafik**
- Seçili adaptör için anlık RX/TX, link kullanım yüzdesi
- Birim seçimi: Oto B/s, KB/s, MB/s, Oto bit/s, Mbps, Gbps
- 60 sn / 5 dk / 10 dk grafik
- Son 1 / 5 / 10 dk toplamları + kalıcı bugün / bu hafta / bu ay

**Process bazlı trafik (ETW)**
- `Microsoft-Windows-Kernel-Network` (ferrisetw) — TCP/UDP, IPv4/IPv6 gönderme/alma olayları (10, 11, 26, 27, 42, 43, 58, 59)
- Callback yalnızca toplar; hesaplama UI tick'inde yapılır
- En çok trafik kullanan process'ler, aranabilir/sıralanabilir tüm process listesi
- Process ağacı: kök + tüm child'lar (recursive), satır başına anlık ve toplam RX/TX, ağaç toplamı
- PID yeniden kullanımı başlangıç zamanıyla doğrulanır; ölen process'ler ağaçtan çıkar
- Kapanışta ETW oturumu durdurulur; çökmeden kalan oturum açılışta temizlenir
- Makine içi (loopback) trafik sayılmaz — process toplamları adaptör toplamıyla tutarlı kalır
- **Yedek kaynak**: ETW başlatılamazsa ya da trafik varken olay gelmezse otomatik olarak IP Helper'a (GetExtendedTcpTable + GetPerTcpConnectionEStats) geçilir; yalnızca TCP sayılır

**Bağlantılar**
- Process (veya seçili ağaç) başına uzak IP:port, protokol, yerel port, alınan/gönderilen bayt, son etkinlik
- Arka planda ters DNS (önbellekli) ve bilinen port → servis adı (https, dns, smb, rdp…)

**Hız testi**
- Sağlayıcı: Otomatik (önce Cloudflare, olmazsa Speedtest.net) / Cloudflare / Speedtest.net (en yakın, en düşük gecikmeli sunucu)
- Ping (TCP RTT medyanı), jitter, 4 akışlı download/upload
- Sunucu, ISS, genel IP, testin harcadığı veri
- Komut satırı teşhisi: `nexnwatch.exe --speedtest [auto|cloudflare|ookla]` (sonucu JSON yazar)
- Manuel + otomatik (varsayılan her 5 dk), geçmiş tablosu ve grafiği
- Test sürerken oluşan trafik 1/5/10 dk pencere toplamlarına katılmaz

**Raporlar** — kullanım özeti, son 30 gün grafiği, günlük döküm, aylık toplam, **uygulama bazında kalıcı kullanım (bugün / bu ay)**, oturumdaki process'ler

**Tepsi ve bildirimler**
- Sistem tepsisi simgesi (anlık hız ipucu; menü: Göster / Hız Testi / Çıkış)
- Kapatınca tepsiye küçült (izleme sürer), Windows açılışında `--minimized` ile gizli başlar
- Windows bildirimleri: günlük kota %80 / %100, hız testinde düşük indirme hızı

**Ayarlar** — ölçüm aralığı, rolling window, birim, koyu/açık tema, otomatik hız testi ve aralığı, kalıcı sayaç, Windows açılışında başlatma (Görev Zamanlayıcı, en yüksek yetki)

## Dosyalar

`%APPDATA%\NexNWatch\`
- `config.toml` — ayarlar
- `usage.json` — günlük kullanım
- `speedtest_history.json` — hız testi geçmişi
- `logs\nexnwatch-YYYY-MM-DD.log` — 14 gün saklanır

## Kurulum

[Releases](https://github.com/Eirahoutmoss/nexnwatch/releases) sayfasından:
- `NexNWatch-Setup-x.y.z.exe` — kurulum (Program Files, Başlat menüsü, isteğe bağlı masaüstü kısayolu ve Windows açılışında başlatma; kaldırırken zamanlanmış görev ve ETW oturumu temizlenir)
- `NexNWatch-x.y.z-portable.zip` — kurulumsuz tek .exe

Exe imzasız olduğu için SmartScreen ilk açılışta "Ek bilgi → Yine de çalıştır" isteyebilir.

## Derleme

Windows üzerinde Rust stable (MSVC) kurulu olmalıdır.

```powershell
cargo build --release
```

Çıktı: `target\release\nexnwatch.exe` — manifest nedeniyle yönetici olarak çalışmayı ister (ETW için gerekli).

Her push'ta GitHub Actions: Linux'ta `fmt` + `clippy -D warnings` + testler; Windows'ta clippy + testler + release derlemesi + Inno Setup kurulum dosyası + taşınabilir zip. `v*` etiketi atılınca bunlar GitHub Release'e eklenir.

Linux'ta da derlenir ve çalışır (geliştirme kolaylığı / ileride port): NIC verisi `/proc/net/dev` ve `/sys/class/net`'ten okunur, ETW yerine "kullanılamıyor" durumu gösterilir. `NEXNWATCH_DEMO=1` ile process/bağlantı ekranları sentetik trafikle denenebilir.

## Teknik temel

Rust 2024 · iced 0.14 · windows-sys 0.61 · ferrisetw 1.2 · tray-icon · tauri-winrt-notification · sysinfo 0.39 · ureq 3 · dns-lookup · chrono · serde/toml · tracing · Inno Setup 6
