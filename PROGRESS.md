# NexNWatch İlerleme

## RC1
- [x] Proje iskeleti
- [x] NexNWatch marka adı
- [x] Koyu tema / konsept dashboard
- [x] NIC keşfi
- [x] Link hızı / durum / MAC / MTU
- [x] RX/TX gerçek zamanlı ölçüm
- [x] 60 sn grafik
- [x] 1/5/10 dk toplamlar
- [x] Process listesi
- [x] Admin manifest
- [ ] IPv4/IPv6 adreslerinin tam keşfi
- [ ] ETW Kernel-Network
- [ ] PID bazlı trafik
- [ ] Recursive process tree attribution
- [ ] Speedtest
- [ ] Ayarlar
- [ ] Kalıcı günlük/haftalık/aylık sayaç
- [ ] Installer / imza / final ikon seti

## RC2 hedefi
ETW consumer + PID traffic attribution. `ferrisetw` ile Microsoft-Windows-Kernel-Network provider bağlanacak; callback yalnızca olayları toplayacak ve işleme ana state katmanında yapılacak.
