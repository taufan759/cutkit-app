# CutKit

Editor video desktop yang ringan untuk **Shorts 9:16**: potong, susun, tambah musik, SFX, meme, teks judul, dan caption kata-per-kata, lalu ekspor MP4 1080×1920. Semua diproses di komputermu dengan ffmpeg, tanpa akun dan tanpa internet.

Dibangun dengan [Tauri 2](https://tauri.app) (Rust + WebView2 bawaan Windows) dan UI HTML/CSS/JS polos tanpa framework, jadi aplikasinya kecil dan tetap enteng di laptop tanpa GPU khusus.

## Fitur

| Bagian | Isi |
|---|---|
| Proyek | Buat dan buka proyek (file `.cutkit.json` di folder proyek), tersimpan otomatis, daftar proyek terakhir. Rasio 9:16, 1:1, 4:5, 3:4, 16:9; 24 sampai 60 fps |
| Impor | Video, gambar, dan audio lewat tombol Impor atau seret dari Explorer langsung ke timeline/preview. File `.srt` yang diseret langsung jadi caption |
| Timeline | Multi-track video dan audio, potong (split), pangkas kiri/kanan, geser dan pindah track, hapus (biasa atau tutup celah), tukar urutan, duplikat, lapisan track naik/turun, bisukan/sembunyikan track, zoom, snap ke tepi klip, playhead, dan awal kata caption |
| Audio | Volume per klip 0 sampai 400%, fade in/out, gelombang suara di klip |
| Undo/redo | Semua perubahan, termasuk geser di preview dan slider |
| Overlay | Gambar/meme, GIF, klip video kecil, dan teks judul dengan posisi, skala, rotasi, dan opacity. Bisa diatur langsung di preview: seret untuk memindah, pojok untuk skala, bulatan atas untuk memutar (Shift = kelipatan 15°), scroll untuk skala |
| Pustaka | Jelajahi folder aset, bawaannya `D:\Master\Pictures\Meme Templates` dan `D:\Master\Music\Sound Effect Meme`, dengan pencarian dan dengar contoh SFX. Folder lain bisa ditambah |
| Teks | Preset judul (Impact bertepi, kotak hitam, kuning, merah, subjudul, kotak kuning), font, ukuran, warna, tepi, bayangan, kotak latar, tebal/miring |
| Caption | Kata-per-kata dengan sorotan kata aktif (warna dan perbesaran), font, warna, tepi, kotak, huruf besar, jumlah kata per halaman, panjang baris, posisi (bisa diseret di preview). Sumber: ketik manual, impor `.srt`, atau impor JSON waktu per kata. Bisa diekspor lagi ke `.srt` atau JSON |
| Preview | Video HTML5 + canvas untuk overlay, teks, dan caption. Format yang tidak bisa diputar WebView2 (MOV/HEVC, MKV, AVI, GIF, video di atas 1080p, dll.) otomatis dibuatkan proxy MP4 720p di cache |
| Ekspor | MP4 H.264 + AAC 192 kbps lewat ffmpeg dengan progress bar, perkiraan sisa waktu, dan tombol batal. Encoder `libx264` (Cepat/Seimbang/Terbaik) atau Intel Quick Sync (`h264_qsv`) bila terdeteksi |

## Format JSON waktu per kata

Untuk caption yang waktunya sudah pas (misalnya dari VoiceKit atau alat transkripsi):

```json
{
  "words": [
    { "text": "Sebab", "start": 0.00, "end": 0.38 },
    { "text": "sejarah", "start": 0.42, "end": 0.95 }
  ]
}
```

Waktu dalam detik. Yang juga diterima:

- Larik langsung `[{ "text" | "word", "start", "end" }]`.
- `{ "beats": [...] }` atau `{ "segments": [...] }` berisi kalimat/beat; waktunya dibagi ke tiap kata sesuai panjang kata.
- Alignment ElevenLabs (`characters`, `character_start_times_seconds`, `character_end_times_seconds`, boleh dibungkus `alignment`), langsung dirakit jadi kata.

## Pintasan keyboard

| Tombol | Aksi |
|---|---|
| Spasi | Putar / jeda |
| ← / → | Mundur / maju 1 frame (Shift: 1 detik) |
| S atau Ctrl+B | Potong di playhead |
| Q / W | Pangkas awal / akhir klip ke playhead |
| Delete / Shift+Delete | Hapus / hapus dan tutup celah |
| Ctrl+D | Duplikat |
| Ctrl+Z / Ctrl+Y | Urungkan / ulangi |
| Ctrl+S / Ctrl+E | Simpan / ekspor |
| + / −, Ctrl+scroll | Zoom timeline |
| N, tahan Alt | Snap aktif/mati, matikan sementara saat menyeret |

## Cara kerja ekspor

Proyek diubah menjadi satu perintah ffmpeg (lihat `crates/cutkit-core/src/export.rs`):

1. Latar `color` seukuran kanvas, lalu tiap klip visual di-`scale`, `rotate`, diberi opacity, dan di-`overlay` pada rentang waktunya, dari track bawah ke atas. Klip dipangkas di sisi input (`-ss`/`-t`) supaya hanya bagian yang dipakai yang didekode.
2. Teks judul dan caption ditulis ke file ASS dan dirender libass (filter `ass`) di atas semuanya.
3. Audio tiap klip dipangkas, diatur volume dan fade-nya, ditunda ke posisinya (`adelay`), lalu dicampur (`amix`).

Filter graph ditulis ke file (`-/filter_complex`) sehingga proyek panjang tidak terbentur batas panjang baris perintah Windows. Rumus penempatan klip dan pembagian halaman caption ada di Rust dan JS sekaligus; tes `parity` memastikan keduanya sama sehingga preview cocok dengan hasil ekspor.

## Batasan saat ini

- Teks dan caption selalu berada di atas gambar/video (sama di preview dan ekspor).
- Volume di atas 100% hanya terdengar di hasil ekspor; preview dibatasi 100%.
- GIF animasi diputar satu kali; duplikat klipnya untuk mengulang.
- Font harus terpasang di Windows. Preview dan ekspor memakai font yang sama, tapi tebal tepi bisa beda tipis.
- Belum ada transisi antar klip, kecepatan putar, dan keyframe animasi.

## Struktur

```
crates/cutkit-core/   Model proyek, probe, pembuat ASS, penyusun perintah ffmpeg, pelari ekspor (+ tes)
  examples/render.rs  Ekspor proyek dari baris perintah tanpa UI
src-tauri/            Aplikasi Tauri: perintah impor, proxy, filmstrip, simpan/buka, ekspor di thread latar
ui/                   HTML, CSS, JS polos
  core/               Logika murni yang juga dites di Node: timeline, caption, geometri
  preview.js          Penggambar preview (video, gambar, teks, caption)
  timeline-view.js    Timeline: klip, geser, pangkas, snap, zoom
  app.js              Panel, inspector, impor, ekspor, pintasan
tests/                Tes Node untuk logika UI + fixture parity bersama Rust
tests/harness/        Server pengembangan untuk menguji UI di browser biasa dengan mock Tauri
```

## Menjalankan dari kode

Butuh [Rust](https://rustup.rs), [Node.js](https://nodejs.org), Visual Studio Build Tools (workload "Desktop development with C++"), dan ffmpeg:

```bash
winget install Gyan.FFmpeg
```

CutKit mencari ffmpeg di PATH lalu di folder paket winget, jadi tidak perlu diatur manual.

```bash
npm install
npm run dev        # jalankan aplikasi
npm test           # tes JS (node --test) + tes Rust, termasuk satu ekspor sungguhan
npm run build      # installer NSIS di target/release/bundle/nsis/
```

Ekspor proyek tanpa membuka aplikasi:

```bash
cargo run -p cutkit-core --example render -- "D:\Proyek\Shorts.cutkit.json" "D:\hasil.mp4" fast
```

Menguji UI di browser (perintah diteruskan ke implementasi Node sederhana, ekspor disimulasikan):

```bash
node tests/harness/server.js 5187
```

## Tes

- `tests/timeline.test.js`: split, trim, geser, hapus/ripple, tukar, snap, undo/redo, track.
- `tests/captions.test.js`: parser SRT (BOM, CRLF, tag, blok rusak), JSON kata, alignment ElevenLabs, ekspor SRT/JSON.
- `tests/parity.test.js` dan `crates/cutkit-core/tests/parity.rs`: fixture yang sama untuk penempatan klip dan halaman caption di JS dan Rust.
- Tes unit Rust: model, probe (video HP diputar, GIF, MP3 bersampul), pembuat ASS, penyusun argumen ffmpeg (diperiksa tanpa menjalankan ffmpeg).
- `crates/cutkit-core/tests/export_real.rs`: membuat klip sintetis dengan ffmpeg, mengekspor proyek berisi potongan, overlay berputar, SFX, teks, dan caption, lalu memeriksa hasilnya dengan ffprobe. Dilewati bila ffmpeg tidak ada.

## Lisensi

MIT. CutKit dibangun dari nol berdasarkan perilaku editor video yang umum dikenal; tidak memakai kode editor lain.
