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

## Sonraki adaylar
- [ ] ETW çalışmazsa yedek: GetExtendedTcpTable + GetPerTcpConnectionEStats
- [ ] Process başına bağlantı listesi (uzak IP/port, ETW daddr/dport alanlarından)
- [ ] Process bazlı kalıcı kullanım (uygulama adına göre günlük)
- [ ] Sistem tepsisine küçültme, eşik aşımında bildirim
- [ ] Final ikon seti (.ico), imzalama, installer
- [ ] 24 saat dayanıklılık testi
