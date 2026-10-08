# NexNWatch RC1

Windows 10/11 x64 için Nex ailesinin ağ izleme uygulaması.

## RC1 kapsamı

- NexNWatch koyu tema arayüzü
- Konsept görsele sadık ana dashboard düzeni
- Windows `GetIfTable2` üzerinden gerçek NIC keşfi
- Adaptör durumu, MAC, MTU ve link hızı
- Seçili NIC için 1 saniyelik RX/TX ölçümü
- Son 60 saniyelik RX/TX çizgi grafiği
- 1 / 5 / 10 dakika trafik toplamları
- Process keşfi ve temel kaynak tablosu
- Admin manifesti
- ETW için ayrılmış mimari sınır

## RC1 notu

ETW PID attribution ve speedtest henüz etkin değildir. Bunlar RC2/RC3 kilometre taşlarında eklenecektir. Bu tercih, doğru veri çekirdeğini önce doğrulamak içindir.

## Derleme

Windows üzerinde Rust stable kurulu olmalıdır.

```powershell
cargo build
cargo run
```

Release:

```powershell
cargo build --release
```

Çıktı:

`target\\release\\nexnwatch.exe`

Uygulama manifest nedeniyle yönetici olarak çalışmayı talep eder.

## Teknik temel

- Rust 2024
- iced 0.14
- windows-sys 0.61
- sysinfo 0.39
- serde/toml
- tracing

Iced'in güncel Application/Subscription modeli ve Windows IP Helper `MIB_IF_ROW2` yüzeyi esas alınmıştır.
