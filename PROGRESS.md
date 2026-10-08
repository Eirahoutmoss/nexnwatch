# NexNWatch İlerleme

## RC1
- [x] Proje iskeleti, marka, koyu tema dashboard
- [x] NIC keşfi, link hızı / durum / MAC / MTU
- [x] RX/TX ölçümü, 60 sn grafik, 1/5/10 dk toplamlar
- [x] Process listesi, admin manifest

## RC2
- [x] IPv4/IPv6, ağ geçidi, DNS (GetAdaptersAddresses)
- [x] Duplex (MSFT_NetAdapter.FullDuplex, arka planda)
- [x] Filtre sürücüsü / WAN Miniport / tünel gürültüsünün elenmesi
- [x] Sanal adaptörlerin ayrı grupta toplanması, "Tüm Adaptörler" = fiziksel toplam
- [x] Adaptör sayacı reset tespiti
- [x] ETW Kernel-Network → PID bazlı trafik
- [x] Recursive process ağacı, PID reuse kontrolü, ağaç toplamı
- [x] En çok trafik kullanan process'ler, aranabilir/sıralanabilir liste
- [x] Hız testi (manuel + otomatik), geçmiş, pencere toplamından hariç tutma
- [x] Kalıcı günlük/haftalık/aylık sayaç, Raporlar sayfası
- [x] Ayarlar (config.toml), açık/koyu tema, birim seçimi
- [x] Windows açılışında başlatma (Görev Zamanlayıcı)
- [x] Dosyaya log, panic hook + yeniden başlatma, kapanışta ETW StopTrace
- [x] GitHub Actions Windows derlemesi

## v0.3
- [x] ETW yedeği: GetExtendedTcpTable + GetPerTcp(6)ConnectionEStats (otomatik geçiş)
- [x] Process / ağaç başına bağlantı listesi, ters DNS, servis adları
- [x] Loopback trafiğinin process toplamlarından çıkarılması
- [x] Uygulama bazında kalıcı günlük kullanım, Raporlar'da bugün / bu ay
- [x] Sistem tepsisi, tepsiye küçültme, gizli başlangıç (--minimized)
- [x] Kota ve düşük hız Windows bildirimleri
- [x] İkon seti (.ico, pencere, tepsi), sürüm bilgisi, DPI manifesti
- [x] Inno Setup kurulum dosyası, taşınabilir zip, etiketle GitHub Release
- [x] CI: fmt + clippy -D warnings (Linux + Windows), birim testleri
- [x] Bellek dayanıklılık ölçümü (Linux, demo kaynak, 250 ms tick)

## Sizin makinenizde doğrulanacaklar
- [ ] ETW ile process trafiği ve Görev Yöneticisi karşılaştırması (±%5)
- [ ] 24 saat açık kalma (Windows)
- [ ] Kod imzalama sertifikası (SmartScreen uyarısını kaldırmak için)
