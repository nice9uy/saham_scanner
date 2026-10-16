# Scanner Saham IDX

Aplikasi web untuk memindai seluruh saham Bursa Efek Indonesia dengan analisa teknikal. Semua bagiannya ditulis dalam Rust: backend Axum, frontend Leptos, dan engine analisa berbasis Polars. Data harga harian diambil dari Yahoo Finance dan disimpan di PostgreSQL.

> Aplikasi ini alat bantu analisa pribadi, **bukan rekomendasi jual/beli**. Harga yang ditampilkan sudah disesuaikan dengan dividen dan stock split, jadi bisa sedikit berbeda dari harga yang tercatat di bursa.

## Fitur

- **Scanner berbasis aturan.** Gabungkan sampai 10 kondisi dengan logika "semua terpenuhi" atau "salah satu", misalnya *SMA 50 memotong ke atas SMA 200 dalam 3 hari terakhir*. Perbandingan yang tersedia: di atas, di bawah, memotong ke atas, memotong ke bawah. Setiap sisi bisa diberi pengali (mis. 2 × rata-rata volume) dan mundur beberapa hari.
- **37 jenis indikator:** SMA, EMA, RSI, MACD, Bollinger Bands, ATR, Stochastic, Supertrend, ADX/±DI, Ichimoku, Parabolic SAR, Highest High/Lowest Low, perubahan %, rata-rata volume, dan indikator aliran dana: OBV, Accumulation/Distribution, Chaikin Money Flow, Money Flow Index, Volume relatif.
- **Rating sinyal Best Buy, Buy, Hold, Netral, Sell, Best Sell** untuk setiap saham, dengan skor −100..+100 dan alasannya (lihat [Rating sinyal](#rating-sinyal)). Bisa dipakai sebagai filter scan, dan tampil di Dashboard, Scanner, halaman saham, serta Watchlist.
- **13 preset strategi:** Best Buy, Best Sell, Golden Cross, RSI Oversold dalam Uptrend, MACD Bullish Cross, Breakout 52 Minggu, Volume Spike, Bollinger Squeeze, Supertrend Flip Bullish, Akumulasi Senyap, Divergensi OBV, Breakout Bervolume, dan Distribusi (waspada).
- **Peringatan risiko** di hasil scan dan halaman saham, plus saran stop loss berbasis ATR (lihat [Peringatan risiko](#peringatan-risiko)).
- **Filter saham:** harga min/maks, nilai transaksi rata-rata 20 hari, sektor, atau hanya saham di watchlist tertentu.
- **Chart interaktif** (TradingView Lightweight Charts): candlestick, volume, overlay indikator, serta panel RSI/MACD/Stochastic/ADX.
- **Ringkasan pasar:** jumlah saham naik/turun, top gainer/loser, nilai transaksi terbesar, dan running trade di bagian atas layar.
- **Watchlist dan scan tersimpan.**
- **Update data otomatis** setiap Senin–Jumat pukul 17.00 WIB, plus tombol "Update sekarang".

## Teknologi

| Bagian | Teknologi |
|---|---|
| Frontend | Leptos 0.8 (CSR/WASM), Tailwind CSS v4, Trunk, Lightweight Charts 5 |
| Backend | Axum 0.8, sqlx 0.8, reqwest, Tokio |
| Analisa | Polars 0.55, yata 0.7 |
| Database | PostgreSQL 16 |
| Container | Podman + podman-compose |

## Prasyarat

| Kebutuhan | Cara memasang |
|---|---|
| Rust 1.88 atau lebih baru | [rustup.rs](https://rustup.rs) |
| Target WebAssembly | `rustup target add wasm32-unknown-unknown` |
| Trunk | `cargo install trunk --locked` |
| Podman dan podman-compose | `sudo apt install podman podman-compose` (Debian/Ubuntu) |
| curl | biasanya sudah ada |

Tailwind CSS, wasm-bindgen, dan wasm-opt diunduh otomatis oleh Trunk saat build pertama.

## Menjalankan

```bash
./prod.sh
```

Perintah ini akan:

1. Build frontend (`trunk build --release`) dan backend (`cargo build --release`) di host.
2. Menjalankan container database `scanner-saham-db`.
3. Membuat image aplikasi dan menjalankan container `scanner-saham-app`.
4. Menunggu sampai aplikasi siap, lalu menampilkan alamatnya.

Buka **http://localhost:3000**.

Saat pertama kali dijalankan, aplikasi mengisi daftar emiten dari `data/idx_stocks.csv`, lalu langsung menjalankan update: daftar lengkap seluruh saham IDX diambil dari TradingView dan harga 5 tahun terakhir diunduh untuk semuanya (±840 saham, sekitar satu menit). Progresnya bisa dilihat di halaman **Data**.

### Perintah `prod.sh`

| Perintah | Fungsi |
|---|---|
| `./prod.sh` | Build lalu jalankan semuanya (sama dengan `./prod.sh start`) |
| `./prod.sh restart` | Build ulang dan jalankan ulang container aplikasi |
| `./prod.sh stop` | Hentikan semua container. Data database tetap tersimpan |
| `./prod.sh status` | Lihat status container |
| `./prod.sh logs` | Ikuti log aplikasi (keluar dengan `Ctrl+C`) |

### Perintah database

| Perintah | Fungsi |
|---|---|
| `scripts/db.sh start` | Jalankan database saja (untuk development dan test) |
| `scripts/db.sh stop` | Hentikan database |
| `scripts/db.sh status` | Cek apakah database berjalan |
| `scripts/db.sh psql` | Buka konsol `psql` ke database |

Data disimpan di volume Podman `scanner-saham-pgdata`. Volume ini **tidak** ikut terhapus oleh `./prod.sh stop`. Untuk menghapus seluruh data secara permanen: `./prod.sh stop && podman volume rm scanner-saham-pgdata`.

## Mode development

Untuk mengembangkan aplikasi tanpa membangun image, jalankan setiap bagian langsung di host:

```bash
# Terminal 1: database
scripts/db.sh start

# Terminal 2: backend di http://localhost:3000
cargo run -p backend

# Terminal 3: frontend dengan hot reload di http://localhost:8080
cd crates/frontend && trunk serve
```

`trunk serve` meneruskan semua request `/api` ke backend di port 3000 (lihat `crates/frontend/Trunk.toml`). Kalau container `scanner-saham-app` sedang berjalan, hentikan dulu dengan `podman stop scanner-saham-app` supaya port 3000 tidak bentrok.

## Test

Test backend membutuhkan database yang berjalan. Setiap test membuat database sementara sendiri, jadi data aslimu tidak tersentuh.

```bash
scripts/db.sh start
export DATABASE_URL=postgres://scanner:scanner@localhost:5440/scanner

cargo test                                       # semua test (shared, scanner-core, backend)
cargo clippy --all-targets -- -D warnings        # lint backend dan engine
cargo clippy -p frontend --target wasm32-unknown-unknown -- -D warnings   # lint frontend
```

## Konfigurasi

Salin `.env.example` menjadi `.env`, lalu ubah sesuai kebutuhan. `./prod.sh` membaca file ini secara otomatis.

| Variabel | Default | Keterangan |
|---|---|---|
| `APP_PORT` | `3000` | Port aplikasi di host |
| `DB_PORT` | `5440` | Port PostgreSQL di host |
| `DB_PASSWORD` | `scanner` | Password database. Ganti sebelum database pertama kali dibuat; sesudahnya perlu juga `ALTER USER scanner PASSWORD '...'` |
| `INGEST_AT` | `17:00` | Jam update otomatis (WIB), Senin–Jumat |
| `INGEST_CONCURRENCY` | `4` | Jumlah saham yang diunduh bersamaan |
| `BACKFILL_YEARS` | `5` | Panjang riwayat untuk saham yang belum punya data |
| `CACHE_BARS` | `300` | Jumlah hari terakhir per saham yang dipakai untuk scan |
| `LIVE_UPDATE` | `1` | Update intraday selama jam bursa. Isi `0` untuk mematikan |
| `LIVE_GAP_SECS` | `3` | Jeda antar request intraday (detik; satu request berisi 100 saham). Makin besar, makin aman dari rate limit |
| `LIVE_INTERVAL_MIN` | `2` | Jarak minimum antar awal putaran intraday (menit) |
| `RUST_LOG` | `info,sqlx=warn,tower_http=warn` | Level log |
| `DATABASE_URL` | `postgres://scanner:scanner@localhost:5440/scanner` | Hanya untuk mode development dan test. Di dalam container alamatnya diatur oleh `compose.yaml` |

Jalankan aplikasi lewat `./prod.sh`, bukan `podman compose up` langsung. Script ini mengisi semua variabel beserta nilai default-nya, karena podman-compose 1.3 tidak bisa mengisi default yang mengandung titik dua, seperti `17:00`.

## Data saham

- **Daftar emiten (otomatis).** Setiap update data, termasuk update terjadwal, aplikasi mengambil daftar seluruh saham IDX (±840 emiten) dari scanner publik TradingView, lalu menyinkronkannya ke database sebelum mengunduh harga:
  - Emiten baru, termasuk IPO, otomatis ditambahkan dan langsung di-backfill 5 tahun.
  - Nama dan sektor yang sudah ada tidak ditimpa; hanya yang masih kosong yang diisi. Sektor TradingView dipetakan ke nama sektor IDX (Keuangan, Energi, Barang Baku, dan seterusnya).
  - Emiten yang tidak lagi tercantum (mis. delisting atau suspensi panjang) dinonaktifkan dan tidak ikut di-scan. Datanya tetap disimpan, dan emiten itu aktif lagi otomatis kalau muncul kembali. Penonaktifan hanya dilakukan kalau daftar yang diterima berisi minimal 500 emiten, supaya gangguan di TradingView tidak menghapus ratusan saham.
  - Kalau TradingView tidak bisa diakses, update tetap berjalan memakai daftar yang sudah tersimpan.

  Backfill penuh ±840 saham selesai dalam sekitar satu menit.
- **Seed dan impor manual.** `data/idx_stocks.csv` (±120 saham likuid) dipakai hanya saat database masih kosong. Kalau ada emiten yang belum tercantum, unduh "Daftar Saham" dari [idx.co.id](https://www.idx.co.id/id/data-pasar/data-saham/daftar-saham/) lewat browser, simpan sebagai CSV, lalu impor di halaman **Data**. Kolom yang dikenali: `Kode`/`ticker`, `Nama Perusahaan`/`name`, `sektor`/`sector`, `Papan Pencatatan`/`board`.
- **Sumber harga.** Yahoo Finance, kode `KODE.JK`, data harian. Setiap update mengambil ulang 5 hari terakhir supaya koreksi data ikut masuk.
- **Update intraday.** Selama jam bursa (Senin–Jumat 09:00–16:15 WIB), candle hari ini untuk **semua saham** diambil lewat endpoint quote Yahoo (`/v7/finance/quote`), **100 saham per request**. Satu putaran untuk ±840 saham hanya butuh ±9 request dan selesai dalam ±30 detik, lalu diulang paling cepat tiap `LIVE_INTERVAL_MIN` menit (default 2). Scanner, rating sinyal, peringatan risiko, dan running trade ikut memakai harga terbaru. Status update ini tampil di halaman **Data**.
  - **Bukan realtime:** harga BEI dari Yahoo adalah data tertunda (umumnya ±15 menit).
  - **Pengaman rate limit:** hanya satu request berjalan dalam satu waktu, dengan jeda `LIVE_GAP_SECS` detik. Dengan default, ±270 request per jam. Kalau Yahoo membalas 429, putaran berhenti dan update dijeda 15 menit, lalu 30, 60, sampai maksimal 120 menit kalau terulang. Endpoint ini butuh cookie dan *crumb* sesi Yahoo; aplikasi memintanya otomatis dan memperbaruinya kalau ditolak.
  - **Hari libur:** IHSG ikut di request pertama. Sebelum pukul 10:30, putaran yang belum menemukan candle IHSG hari ini hanya menunggu dan mencoba lagi. Bursa baru dianggap libur kalau sampai pukul 10:30 datanya belum juga ada.
  - Saham yang belum bertransaksi hari ini tetap memakai data kemarin. Update intraday juga berhenti selama update harian berjalan. Update harian pukul 17:00 menimpa candle intraday dengan data final. Selama jam bursa, jumlah saham naik/turun di dashboard hanya menghitung saham yang sudah punya candle hari ini.
- **Saham yang gagal diunduh** (mis. sedang disuspensi atau sudah delisting) dicatat di riwayat update dan tidak menghentikan update saham lainnya.

## Rating sinyal

Setiap saham diberi satu rating, dihitung ulang setiap kali data diperbarui.

**Skor gabungan (−100..+100).** Dua belas indikator memberi suara +1 (bullish), −1 (bearish), atau 0:

| Kelompok | Indikator dan aturannya |
|---|---|
| Tren | Harga vs SMA 20, SMA 50, SMA 200 · SMA 50 vs SMA 200 · harga vs Supertrend(10, 3) · MACD vs signal · ADX(14) ≥ 20 dengan arah +DI/−DI |
| Momentum | RSI(14) < 30 / > 70 · Stochastic berbalik dari < 20 / > 80 · MFI(14) < 20 / > 80 |
| Aliran dana | CMF(20) > 0,05 / < −0,05 · OBV dibanding 20 hari lalu |

Skor = jumlah suara ÷ 12 × 100.

**Rating:**

| Rating | Syarat |
|---|---|
| **Best Buy** | Pemicu naik terjadi dalam 3 hari terakhir (MACD memotong ke atas signal, harga menembus ke atas Supertrend, atau menembus ke atas SMA 50), skor ≥ 25, harga ≤ 8% di atas SMA 20, RSI < 70, harga hari ini tidak turun > 5%, IHSG di atas SMA 50 (pasar kuat), **dan** tidak ada peringatan pergerakan tidak wajar, riwayat volatilitas ekstrem, likuiditas tidak konsisten, harga di bawah Rp100, distribusi, atau tren turun |
| **Buy** | Skor ≥ 35 |
| **Hold** | Skor 10..34: cenderung positif, layak ditahan kalau sudah punya |
| **Netral** | Skor −9..9 |
| **Sell** | Skor ≤ −10 |
| **Best Sell** | Pemicu turun dalam 3 hari terakhir (kebalikan pemicu naik), skor ≤ −10, dan harga masih ≤ 8% di bawah high 20 hari; **atau** RSI baru turun dari atas 70 dengan histogram MACD mengecil dan skor < 35 |

Saham yang sebenarnya memenuhi Best Buy tetapi punya peringatan berat, atau muncul saat IHSG di bawah SMA 50, diturunkan ke Buy/Hold sesuai skornya, dengan alasan tertulis "Bukan Best Buy: …". Dashboard menampilkan kondisi pasar saat ini (pasar kuat atau melemah). Data IHSG (`^JKSE`) diambil dari Yahoo bersama update harian dan update intraday.

**Cara memakai:** jalankan preset **Best Buy** atau **Best Sell**, atau centang rating di filter "Sinyal" pada Scanner (bisa tanpa kondisi lain). Halaman saham menampilkan rating, skor, dan alasannya. Rating adalah ringkasan kondisi teknikal, **bukan jaminan arah harga**. Selalu cek chart dan pasang stop loss.

### Uji sinyal (backtest)

Halaman **Uji Sinyal** menguji rating di atas pada seluruh riwayat harga (±5 tahun, termasuk saham yang sudah tidak aktif):

- Sinyal dihitung pada hari rating sebuah saham **baru berubah** menjadi level itu, termasuk pembatalan Best Buy oleh peringatan risiko.
- Posisi dianggap dibeli di harga **pembukaan hari berikutnya**, lalu dijual di penutupan hari ke-5, 10, atau 20 setelah sinyal.
- **Pembanding:** semua hari bursa pada saham yang sama. Sinyal beli dianggap berguna kalau rata-ratanya minimal 0,5 poin di atas pembanding; sinyal jual kalau minimal 0,5 poin di bawahnya.
- "Tepat" untuk sinyal beli berarti untung setelah biaya 0,4% (beli 0,15% + jual 0,25%); untuk Sell dan Best Sell berarti harganya turun.
- Pilihan **Saham likuid** hanya menghitung hari ketika rata-rata nilai transaksi 20 hari ≥ Rp1 miliar.
- Bagian **Cara keluar setelah Best Buy** membandingkan jual tetap setelah 20 hari dengan trailing stop (high tertinggi sejak beli − 3 × ATR 22, paling lama 60 hari) pada sinyal yang sama.
- Hasil disimpan dan baru dihitung ulang setelah update harian berikutnya. Perhitungan pertama butuh beberapa detik.

**Hasil per 8 Oktober 2026 (saham likuid, dipegang 20 hari):**

| Rating | Sinyal | Rata-rata | Median | Tepat |
|---|---|---|---|---|
| Best Buy (dengan filter IHSG) | 2.642 | +1,52% | −0,94% | 43% untung |
| Best Buy (tanpa filter IHSG, aturan lama) | 4.071 | +0,35% | −1,40% | 41% untung |
| Buy | 10.750 | +0,74% | −2,16% | 40% untung |
| Sell | 12.341 | −0,89% | −1,59% | 56% turun |
| Best Sell | 3.572 | −0,12% | −0,74% | 54% turun |
| Pembanding (semua hari) | 294.049 | +0,64% | −1,42% | 42% untung |

Catatan dari pengujian ini:
- **Filter IHSG di atas SMA 50** adalah satu-satunya aturan tambahan yang memperbaiki Best Buy di setiap tahun, pada horizon 10 maupun 20 hari. Aturan lain yang diuji tidak konsisten atau justru memperburuk: syarat volume ≥ 1,5–2× rata-rata, kekuatan relatif terhadap IHSG, dan rasio untung-rugi ≥ 2.
- Perbaikan itu terutama datang dari **kondisi pasar**. Saat IHSG di atas SMA 50, saham likuid acak pun rata-rata naik +1,91% dalam 20 hari. Jadi Best Buy membantu memilih *waktu* masuk, belum terbukti lebih jeli memilih *saham*.
- Rating **Sell** paling konsisten: saham yang baru masuk Sell lebih lemah daripada rata-rata di setiap tahun. Rating ini berguna untuk memutuskan saham mana yang dihindari atau dijual.
- Median hampir selalu negatif: sebagian besar saham turun dalam 20 hari, dan rata-rata positif ditopang segelintir saham yang melonjak. Stop loss tetap penting.

## Fundamental

Setiap saham diberi label kesehatan fundamental: **Sehat, Cukup, Lemah, Buruk**, atau **Data kurang** kalau data labanya tidak tersedia. Datanya laba per saham 12 bulan terakhir, nilai buku per saham, dividen, dan kapitalisasi pasar dari Yahoo Finance (endpoint quote, 100 saham per request). Data ini diperbarui bersama update harian, dan diambil sekali saat aplikasi start kalau masih kosong.

| Kondisi | Hasil |
|---|---|
| Rugi 12 bulan terakhir, atau ekuitas negatif | **Buruk** |
| ROE ≥ 15% | +2 poin |
| ROE 8–15% | +1 |
| PER di bawah median sektornya (saham berlaba; median pasar kalau sektornya kurang dari 5 saham) | +1 |
| PER lebih dari 2× median sektornya | −1 |
| Dividen ≥ 2% per tahun | +1 |
| Total ≥ 3 → **Sehat**, 1–2 → **Cukup**, ≤ 0 → **Lemah** | |

PER, PBV, dan ROE dihitung dari harga terakhir. Untuk emiten yang laporan keuangannya dalam dolar (mis. ADRO, AADI, ITMG), nilai buku dikonversi dengan kurs USD/IDR yang diambil di request yang sama; dividennya tidak dipakai karena Yahoo mengisinya 0.

Label ini tampil di panel **Fundamental** halaman saham, sebagai kolom dan filter **Fundamental** di Scanner (bisa dipakai tanpa kondisi lain), dan dalam preset **Fundamental Sehat + Tren Naik** (label Sehat, harga di atas SMA 200, SMA 50 di atas SMA 200).

**Batasan:**
- Belum memeriksa utang, arus kas, atau kualitas laba (mis. laba dari penjualan aset). ROE sangat tinggi bisa berasal dari ekuitas yang kecil.
- **Tidak bisa diuji dengan backtest.** Yang tersedia hanya data fundamental terkini, dan memakainya untuk menilai masa lalu akan membuat hasilnya tampak lebih bagus dari kenyataan.

## Win rate sinyal per saham

Setiap saham punya **win rate**: dari seluruh riwayat (±5 tahun), setiap kali rating baru berubah menjadi Buy atau Best Buy, apakah posisi untung setelah 20 hari bursa (masuk di open hari berikutnya, setelah biaya 0,4%). Contoh: `52% (25/48)` artinya 25 dari 48 sinyal beli untung. Angka ini dihitung ulang setelah setiap update harian (±4 detik untuk semua saham), dan sekali saat aplikasi start kalau masih kosong.

Win rate tampil di halaman saham dan sebagai kolom di Scanner. Di Scanner juga ada filter **Win rate min (%)** dengan **Minimal sinyal** (default 5), supaya saham dengan 1 dari 1 sinyal tidak lolos sebagai 100%.

**Peringatan:** dalam uji 2022–2024 → 2025–2026, win rate masa lalu **tidak memprediksi** hasil berikutnya. Korelasinya ±0,04. Saham dengan win rate ≥ 60% di 2022–2024 hanya untung 43,6% di 2025–2026, sama dengan rata-rata (43,4%). Penyebabnya, jumlah sinyal per saham terlalu sedikit untuk membedakan kebetulan dari sifat saham. Pakai win rate sebagai informasi riwayat, bukan jaminan.

**Momentum (kekuatan relatif) sudah diuji, tapi tidak dipakai.** Pada saham likuid 2022–2026, 20% saham dengan kenaikan 3 atau 6 bulan tertinggi justru tertinggal dari rata-rata. Kenaikan 12 bulan hanya unggul di 3 dari 5 tahun, dan sebagian besar keunggulannya datang dari 2025.

## Berita (ringkasan harian)

Halaman **Berita** berisi ringkasan pasar yang disusun otomatis dari data aplikasi sendiri, jadi angkanya selalu sama dengan halaman lain. Isinya ikut diperbarui setiap putaran intraday dan setiap update harian, tanpa perlu refresh:

- **Judul:** perubahan IHSG, kondisi pasar (di atas/bawah SMA 50), jumlah Best Buy dan Best Sell.
- **Kondisi pasar:** IHSG, jumlah saham naik/turun/stagnan, saham yang turun ≥ 14% (mendekati auto reject bawah) atau naik ≥ 20%.
- **Sektor:** rata-rata perubahan saham likuid per sektor (minimal 3 saham), dari yang paling tahan sampai yang paling tertekan.
- **Peluang beli:** daftar Best Buy. Saat IHSG di bawah SMA 50, Best Buy ditahan; yang ditampilkan adalah saham likuid bertren positif (Buy/Hold) yang paling tahan hari ini (turun kurang dari 3% atau naik), sebagai pantauan saat pasar pulih.
- **Sebaiknya dihindari atau dijual:** Best Sell, lalu saham Sell dengan skor terlemah.
- **Waspada:** saham likuid dengan pergerakan tidak wajar.
- **Watchlist kamu:** saham watchlist yang menembus trailing stop atau mendapat rating Best Buy, Sell, atau Best Sell.

Ringkasan ini **tidak** membaca berita, laporan keuangan, pengumuman bursa, atau aliran dana asing, dan bukan rekomendasi jual/beli.

## Membaca aliran dana dan risiko

### Preset aliran dana

| Preset | Kondisi | Cara membaca |
|---|---|---|
| **Akumulasi Senyap** | Volume > 2× rata-rata 20 hari, harga hanya bergerak ±2%, CMF(20) > 0,1 | Barang dikumpulkan tanpa harga dibiarkan naik. Kandidat untuk dipantau, bukan langsung dibeli |
| **Divergensi OBV** | Harga lebih rendah daripada 20 hari lalu, tetapi OBV lebih tinggi dan CMF > 0 | Ada pembelian saat harga masih lesu |
| **Breakout Bervolume** | Harga menembus high 20 hari dengan volume > 2× rata-rata, di atas SMA 50 | Breakout yang didukung volume lebih bisa dipercaya |
| **Distribusi (waspada)** | CMF(20) < −0,1 dan OBV turun 10 hari, padahal harga bertahan | Tanda pemegang besar melepas barang. Pertimbangkan mengurangi posisi |

### Peringatan risiko

Setiap saham di hasil scan dan di halaman detail diberi peringatan berikut kalau polanya cocok:

| Peringatan | Level | Aturan |
|---|---|---|
| Pergerakan tidak wajar | Tinggi | Harga naik/turun ≥ 25% dalam 5 hari, **atau** volume ≥ 5× rata-rata dengan harga bergerak ≥ 7% dalam sehari (mirip pemicu UMA bursa / pola "gorengan") |
| Riwayat volatilitas ekstrem | Sedang | Pernah naik/turun ≥ 20% dalam sehari selama 60 hari bursa terakhir (mendekati batas ARA/ARB) |
| Likuiditas tidak konsisten | Sedang | Rata-rata nilai transaksi 20 hari ≥ Rp1 miliar, tetapi median 60 hari < Rp1 miliar: biasanya sepi lalu tiba-tiba ramai |
| Kurang likuid | Sedang | Nilai transaksi rata-rata 20 hari < Rp1 miliar, atau riwayat transaksi kurang dari 20 hari |
| Harga di bawah Rp100 | Sedang | Mudah digerakkan; satu fraksi harga bisa ≥ 1% |
| Tanda distribusi | Sedang | CMF(20) < −0,1, OBV turun 10 hari, harga bertahan |
| Tren turun | Sedang | Harga di bawah SMA 200 dan SMA 50 di bawah SMA 200 |
| Jenuh beli | Info | RSI(14) > 80 |
| Di bawah trailing stop | Info | Penutupan di bawah high 22 hari − 3 × ATR(22). Batas keluar untuk posisi yang sedang dipegang |

Halaman detail saham juga menampilkan dua batas keluar:
- **Saran stop loss awal = harga penutupan − 2 × ATR(14)**, untuk posisi baru.
- **Trailing stop = high tertinggi 22 hari − 3 × ATR(22)** (*chandelier exit*), untuk posisi yang sudah berjalan. Batasnya naik mengikuti harga. Watchlist menampilkan trailing stop setiap saham, dan menandainya merah ("di bawah · keluar") kalau harga penutupan sudah di bawahnya.

Backtest menunjukkan trailing stop berguna sebagai **cara keluar**, bukan sebagai sinyal jual. Setelah Best Buy, keluar dengan trailing stop menghasilkan rata-rata +2,02% dibanding +1,52% kalau dijual tetap setelah 20 hari. Rugi terbesar juga turun dari −56,6% menjadi −30,4%. Konsekuensinya, hanya 37% transaksi yang untung (sebelumnya 44%): lebih sering rugi kecil, tapi untung besar dibiarkan berjalan. Sebaliknya, saham yang baru menembus ke bawah trailing stop tidak melemah lebih dari rata-rata. Untuk memilih saham yang dihindari, rating Sell lebih tepat.

**Batasan penting:**
- Peringatan yang membatalkan Best Buy: pergerakan tidak wajar, riwayat volatilitas ekstrem, likuiditas tidak konsisten, harga di bawah Rp100, tanda distribusi, dan tren turun.
- Pengumuman UMA resmi, suspensi, notasi khusus emiten (E, X, M, dan lain-lain), serta kasus hukum **tidak** diperiksa karena datanya tidak tersedia. Cek di aplikasi broker atau idx.co.id sebelum membeli.
- Semua peringatan dihitung dari harga dan volume harian saja. Peringatan ini adalah tanda yang perlu diwaspadai, **bukan bukti** adanya bandar atau manipulasi.
- Analisa broker (siapa yang membeli dan menjual, *broker summary*) dan aliran dana asing **belum tersedia**, karena datanya tidak ada di Yahoo Finance.
- Tidak ada indikator yang bisa memastikan harga akan naik. Selalu pakai stop loss dan atur besar posisi sesuai modal.

## REST API

Semua endpoint berada di bawah `/api` dan memakai JSON. Respons error berbentuk `{"error": "...", "message": "..."}`.

| Method & path | Fungsi |
|---|---|
| `GET /api/stocks?q=&limit=` | Cari saham berdasarkan kode atau nama |
| `GET /api/stocks/{kode}` | Detail dan statistik saham |
| `GET /api/stocks/{kode}/candles?range=1y&indicators=sma:50,rsi:14` | Data candle dan indikator untuk chart |
| `POST /api/stocks/import` | Impor CSV emiten (multipart, field `file`) |
| `POST /api/scan` | Jalankan scan |
| `GET /api/presets` | Daftar preset strategi |
| `GET/POST /api/scans`, `PUT/DELETE /api/scans/{id}` | Scan tersimpan |
| `GET/POST /api/watchlists`, `DELETE /api/watchlists/{id}` | Watchlist |
| `POST/DELETE /api/watchlists/{id}/items/{kode}` | Tambah/hapus saham di watchlist |
| `GET /api/market/summary` | Ringkasan pasar |
| `GET /api/backtest?liquid=true` | Uji historis rating sinyal |
| `GET /api/brief` | Ringkasan pasar harian (halaman Berita) |
| `GET /api/server` | Kondisi server: CPU per core, memori, disk, database, cache, status update (halaman Server) |
| `POST /api/ingest/run`, `GET /api/ingest/status` | Mulai update data dan lihat progresnya |

Pilihan `range`: `3m`, `6m`, `1y`, `3y`, `5y`. Format `indicators`: `sma:N`, `ema:N`, `rsi:N`, `atr:N`, `volume_sma:N`, `bb:N:σ×10`, `macd:fast:slow:signal`, `stoch:N:smooth:d`, `supertrend:N:mult×10`, `adx:N`, `psar:step×1000:max×1000`, `ichimoku:tenkan:kijun:senkouB`, `obv`, `ad`, `cmf:N`, `mfi:N`, `rvol:N`.

Contoh scan (RSI 14 memotong ke atas 30, hanya saham dengan nilai transaksi minimal Rp1 miliar per hari):

```bash
curl -s -X POST http://localhost:3000/api/scan \
  -H 'content-type: application/json' \
  -d '{
    "conditions": [{
      "left":  {"source": {"indicator": {"kind": "rsi", "period": 14}}},
      "cmp":   {"op": "cross_above", "within": 3},
      "right": {"source": {"constant": 30}}
    }],
    "universe": {"min_avg_value_20d": 1000000000}
  }'
```

## Struktur proyek

```
crates/
  shared/      tipe data bersama backend dan frontend (aturan scan, DTO API)
  core/        engine analisa: indikator, validasi, scan, preset (package scanner-core)
  backend/     REST API, database, update data Yahoo, jadwal
  frontend/    aplikasi web Leptos + Tailwind
migrations/    skema database
data/          seed daftar emiten
scripts/db.sh  kelola container database
compose.yaml   definisi container (db + app)
Containerfile  image aplikasi
prod.sh        build dan jalankan semuanya
docs/          spec desain, rencana implementasi, peta arsitektur
```

Penjelasan arsitektur lengkap ada di [docs/architecture.md](docs/architecture.md). Peta kode interaktif dibuat dengan graphify (`graphify update . --force`) dan bisa dibuka di `graphify-out/graph.html`.

## Mengatasi masalah

| Gejala | Penyebab dan solusi |
|---|---|
| `podman compose tidak tersedia` | Pasang dengan `sudo apt install podman-compose` |
| `trunk tidak ditemukan` | `cargo install trunk --locked` |
| Port 3000 atau 5440 sudah dipakai | Ubah `APP_PORT` atau `DB_PORT` di `.env`. Kalau `DB_PORT` diubah, sesuaikan juga `DATABASE_URL` |
| Halaman menampilkan "Belum ada data harga" | Buka halaman **Data** lalu klik **Update sekarang** |
| Beberapa saham gagal di riwayat update | Biasanya saham yang disuspensi atau delisting, sehingga Yahoo tidak punya datanya. Saham lain tetap diperbarui |
| `apt update` gagal karena repository `docker.com` | Repository Docker tidak dibutuhkan proyek ini. Hapus `/etc/apt/sources.list.d/docker.list` |
| Aplikasi berhenti saat startup | Lihat penyebabnya dengan `./prod.sh logs` atau `podman logs scanner-saham-app` |
