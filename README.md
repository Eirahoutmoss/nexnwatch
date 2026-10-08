# NexNWatch

Windows 10/11 x64 için Nex ailesinin gerçek zamanlı ağ izleme ve process bazlı trafik analizi uygulaması.

## Özellikler (RC2)

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

**Hız testi**
- Cloudflare uç noktaları: ping (TCP RTT medyanı), jitter, 4 akışlı download/upload
- Sunucu (colo/şehir), ISS, genel IP, testin harcadığı veri
- Manuel + otomatik (varsayılan her 5 dk), geçmiş tablosu ve grafiği
- Test sürerken oluşan trafik 1/5/10 dk pencere toplamlarına katılmaz

**Raporlar** — kullanım özeti, son 30 gün grafiği, günlük döküm, aylık toplam, oturumdaki process'ler

**Ayarlar** — ölçüm aralığı, rolling window, birim, koyu/açık tema, otomatik hız testi ve aralığı, kalıcı sayaç, Windows açılışında başlatma (Görev Zamanlayıcı, en yüksek yetki)

## Dosyalar

`%APPDATA%\NexNWatch\`
- `config.toml` — ayarlar
- `usage.json` — günlük kullanım
- `speedtest_history.json` — hız testi geçmişi
- `logs\nexnwatch-YYYY-MM-DD.log` — 14 gün saklanır

## Derleme

Windows üzerinde Rust stable (MSVC) kurulu olmalıdır.

```powershell
cargo build --release
```

Çıktı: `target\release\nexnwatch.exe` — manifest nedeniyle yönetici olarak çalışmayı ister (ETW için gerekli).

Her push'ta GitHub Actions `windows-latest` üzerinde release derlemesi yapar ve `nexnwatch.exe`'yi artifact olarak yükler.

Linux'ta da derlenir ve çalışır (geliştirme kolaylığı / ileride port): NIC verisi `/proc/net/dev` ve `/sys/class/net`'ten okunur, ETW yerine "kullanılamıyor" durumu gösterilir.

## Teknik temel

Rust 2024 · iced 0.14 · windows-sys 0.61 · ferrisetw 1.2 · sysinfo 0.39 · ureq 3 · chrono · serde/toml · tracing
