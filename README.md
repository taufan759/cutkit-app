# CutKit

Editor video desktop yang ringan untuk **Shorts 9:16 maupun video YouTube panjang 16:9/4K**: potong, susun, transisi, efek, filter warna, keyframe, musik, SFX, meme, stiker, teks judul, caption kata-per-kata otomatis dari suara, TTS, dan hapus background, lalu ekspor MP4. Semua diproses di komputermu dengan ffmpeg dan model AI lokal; internet hanya dipakai sekali untuk mengunduh model yang kamu pilih.

Dibangun dengan [Tauri 2](https://tauri.app) (Rust + WebView2 bawaan Windows) dan UI HTML/CSS/JS polos tanpa framework, jadi aplikasinya kecil dan tetap enteng di laptop tanpa GPU khusus.

## Mulai cepat (untuk pemula)

1. Pasang CutKit dari installer `CutKit_0.1.0_x64-setup.exe`, lalu buka dari Start Menu.
2. Kalau muncul banner **ffmpeg belum terpasang**, klik **Pasang otomatis**. CutKit memasang ffmpeg lewat winget (tanpa terminal, tanpa hak admin) dan menampilkan prosesnya. Status bisa dicek kapan saja di menu **Proyek → Cek sistem & ffmpeg**.
3. Di layar sambutan pilih salah satu:
   - **🎬 Coba contoh**: membuat proyek latihan berisi narasi, gambar, klip, musik, judul, dan caption. Cocok untuk belajar; panduan 7 langkah langsung menjelaskan tiap bagian layar.
   - **⚡ Buat video cepat**: pilih satu folder berisi voice over, gambar/klip, dan caption, lalu timeline tersusun sendiri (Shorts atau YouTube 16:9).
   - **📐 Dari template**: susunan siap pakai (Fakta Shorts, Reaksi meme, Intro YouTube), tinggal pilih media pengganti.
   - **＋ Proyek kosong**: susun sendiri dari awal.
4. Rapikan bila perlu (potong dengan S, hapus dengan Delete, geser gambar di preview), lalu klik **Ekspor**.

### Isi folder untuk Buat Shorts cepat

```
Episode 12\
  vo.mp3          voice over (nama berisi vo, voice, narasi, atau tts)
  01.png          gambar/klip, diurutkan menurut nama file
  02.png
  03.mp4
  words.json      waktu per kata (atau file .srt)
  musik.mp3       opsional, diputar pelan di belakang (nama berisi musik, music, bgm, atau backsound)
```

Gambar dibagi sepanjang voice over. Klip video yang lebih pendek dari jatahnya diputar utuh, dan titik pergantian gambar ditempel ke awal kata terdekat supaya pas dengan ucapan. Semua hasilnya tetap bisa diedit dan di-undo.

## Fitur

| Bagian | Isi |
|---|---|
| Video panjang | Preset YouTube 16:9 (1920×1080) dan 4K (3840×2160), timeline sampai berjam-jam (waktu jam:menit:detik), ekspor bertahap per bagian supaya RAM tetap kecil, daftar kata caption per halaman dengan pencarian |
| Auto caption | Suara (voice over atau semua klip satu track) jadi caption kata-per-kata dengan Whisper, offline setelah model diunduh sekali (base 60 MB, small 190 MB, medium 539 MB). Naskah opsional dipakai untuk membetulkan ejaan sambil mempertahankan waktu dari suara. Di i5-1345U model small sekitar 8x lebih cepat dari durasi audio |
| TTS | Tab Suara: naskah jadi narasi Bahasa Indonesia offline dengan Piper, kecepatan dan jeda antar kalimat bisa diatur, langsung masuk timeline beserta caption yang waktunya diselaraskan dengan Whisper |
| Ketukan musik | Deteksi tempo (BPM) dan ketukan, garis ketukan di klip musik, snap ke ketukan, tombol "Sinkronkan potongan", dan wizard yang mengganti gambar tepat di ketukan |
| Transisi | Crossfade, geser 4 arah, dan dorong kiri/kanan antar klip di track yang sama; klip menumpuk sepanjang transisi seperti CapCut dan suara ikut crossfade |
| Efek | Getar, zoom kejut, kilat, hitam putih, vignette, blur, glitch RGB, negatif, sensor piksel, letterbox — sebagai klip di track Efek yang berlaku pada lapisan di bawahnya |
| Filter warna | Kecerahan, kontras, saturasi, rona, suhu, file LUT `.cube`, dan preset (Hangat, Dingin, Cerah, Dramatis, Vintage, Pudar, Hitam putih) |
| Keyframe | Animasi posisi, skala, rotasi, dan opacity di antara keyframe; slider dan geser di preview mengubah keyframe di playhead |
| Stiker & template | 48 stiker emoji Twemoji, stiker teks (SUBSCRIBE, Tonton sampai habis, ...), template bawaan, dan template buatan sendiri |
| Hapus background | Tanpa green screen: model U²-Net (cepat, 4,6 MB) atau IS-Net (akurat, 179 MB) memisahkan orang/benda dari latarnya; gambar jadi PNG transparan, video jadi WebM beralpha |
| Untuk pemula | Contoh proyek siap putar, wizard Buat Shorts cepat, panduan langkah demi langkah, cek sistem, pasang ffmpeg otomatis, bersihkan cache, dan petunjuk saat layar masih kosong |
| Proyek | Buat dan buka proyek (file `.cutkit.json` di folder proyek), tersimpan otomatis, daftar proyek terakhir. Rasio 9:16, 1:1, 4:5, 3:4, 16:9; 24 sampai 60 fps |
| Impor | Video, gambar, dan audio lewat tombol Impor atau seret dari Explorer langsung ke timeline/preview. File `.srt` yang diseret langsung jadi caption |
| Timeline | Multi-track video dan audio, potong (split), pangkas kiri/kanan, geser dan pindah track, hapus (biasa atau tutup celah), tukar urutan, duplikat, lapisan track naik/turun, bisukan/sembunyikan track, zoom, snap ke tepi klip, playhead, dan awal kata caption |
| Audio | Volume per klip 0 sampai 400%, fade in/out, gelombang suara di klip, normalisasi loudness −14 LUFS (standar YouTube) saat ekspor |
| Kecepatan | 0,1x sampai 10x per klip video/audio; suara dipercepat tanpa mengubah nada, klip sesudahnya ikut bergeser |
| Gerak (Ken Burns) | Zoom masuk/keluar atau geser kiri/kanan/atas/bawah di dalam bingkai klip, dengan kekuatan yang bisa diatur. Wizard bisa memberi gerak otomatis ke semua gambar |
| Latar blur | Video lanskap atau gambar yang tidak memenuhi 9:16 diberi latar versi blur dirinya sendiri, bukan pinggiran hitam |
| Chroma key | Hapus latar hijau/biru (atau warna lain) pada meme green screen, dengan kemiripan dan kehalusan yang bisa diatur |
| Undo/redo | Semua perubahan, termasuk geser di preview dan slider |
| Overlay | Gambar/meme, GIF, klip video kecil, dan teks judul dengan posisi, skala, rotasi, dan opacity. Bisa diatur langsung di preview: seret untuk memindah, pojok untuk skala, bulatan atas untuk memutar (Shift = kelipatan 15°), scroll untuk skala |
| Pustaka | Jelajahi folder aset, bawaannya `D:\Master\Pictures\Meme Templates` dan `D:\Master\Music\Sound Effect Meme`, dengan pencarian dan dengar contoh SFX. Folder lain bisa ditambah |
| Teks | Preset judul (Impact bertepi, kotak hitam, kuning, merah, subjudul, kotak kuning), font, ukuran, warna, tepi, bayangan, kotak latar, tebal/miring |
| Caption | Kata-per-kata dengan sorotan kata aktif (warna, perbesaran, dan animasi pop), font, warna, tepi, kotak, huruf besar, jumlah kata per halaman, panjang baris, posisi (bisa diseret di preview). Sumber: ketik manual, impor `.srt`, atau impor JSON waktu per kata. Bisa diekspor lagi ke `.srt` atau JSON |
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

## Atribusi

- Stiker emoji: [Twemoji](https://github.com/jdecked/twemoji) © Twitter, Inc. dan kontributor, CC-BY 4.0 (lihat `ui/stickers`).
- Model AI pihak ketiga di atas diunduh saat dipakai dan tidak ikut di dalam installer.

## Cara kerja ekspor

Proyek diubah menjadi satu perintah ffmpeg (lihat `crates/cutkit-core/src/export.rs`):

1. Latar `color` seukuran kanvas, lalu tiap klip visual di-`scale`, `rotate`, diberi opacity, dan di-`overlay` pada rentang waktunya, dari track bawah ke atas. Klip dipangkas di sisi input (`-ss`/`-t`) supaya hanya bagian yang dipakai yang didekode.
2. Teks judul dan caption ditulis ke file ASS dan dirender libass (filter `ass`) di atas semuanya.
3. Audio tiap klip dipangkas, diatur volume dan fade-nya, ditunda ke posisinya (`adelay`), lalu dicampur (`amix`).

Filter graph ditulis ke file (`-/filter_complex`) sehingga proyek panjang tidak terbentur batas panjang baris perintah Windows. Rumus penempatan klip dan pembagian halaman caption ada di Rust dan JS sekaligus; tes `parity` memastikan keduanya sama sehingga preview cocok dengan hasil ekspor.

## MCP: pakai CutKit dari Claude

CutKit punya server [MCP](https://modelcontextprotocol.io) (`mcp/server.js`), jadi Claude Code, Claude Desktop, atau klien MCP lain bisa menyusun dan mengekspor video lewat percakapan, misalnya *"buat Shorts dari folder ini, kasih caption kuning, ekspor ke Desktop"*. Proyek yang dibuat lewat MCP adalah file `.cutkit.json` biasa, jadi bisa dibuka dan dirapikan lagi di aplikasi.

Server ini memakai modul timeline yang sama dengan aplikasi (`ui/core`) dan menjalankan pekerjaan berat lewat `cutkit-cli` (ffmpeg, Whisper, Piper, hapus background). Model AI dibaca dari folder model aplikasi, jadi model yang sudah diunduh di aplikasi langsung terpakai.

| Tool | Fungsi |
|---|---|
| `cutkit_status` | Lokasi ffmpeg, folder model, model yang sudah terpasang |
| `cutkit_new_project`, `cutkit_project_summary` | Buat proyek (9:16, 16:9, 4K, 1:1, ...) dan lihat isinya |
| `cutkit_import_media`, `cutkit_add_clip`, `cutkit_edit_clip`, `cutkit_split_clip`, `cutkit_delete_clip` | Susun timeline: posisi, durasi, kecepatan, volume, fade, Ken Burns, latar blur, chroma, filter warna, transisi, keyframe |
| `cutkit_add_text`, `cutkit_add_effect` | Teks judul dan efek (getar, kilat, glitch, ...) |
| `cutkit_quick_video` | Susun otomatis dari voice over + gambar/klip + musik + caption, bisa ganti gambar di ketukan |
| `cutkit_captions_from_audio`, `cutkit_set_captions` | Auto caption Whisper, atau dari `.srt`/JSON/teks, plus gaya (kuning, hijau, kotak, youtube) |
| `cutkit_tts` | Naskah jadi suara Piper + caption |
| `cutkit_detect_beats`, `cutkit_remove_background` | Ketukan musik dan hapus background tanpa green screen |
| `cutkit_list_templates`, `cutkit_apply_template` | Proyek dari template |
| `cutkit_export` | Ekspor MP4 dengan progres (notifikasi MCP) |
| `cutkit_download_model` | Unduh model AI |

Pasang (sekali):

```bash
cargo build --release -p cutkit-cli
```

Claude Code membaca `.mcp.json` di folder repo ini secara otomatis saat dibuka di sini. Untuk semua folder, tambahkan ke `~/.claude.json` (atau `claude_desktop_config.json` untuk Claude Desktop):

```json
{
  "mcpServers": {
    "cutkit": { "command": "node", "args": ["D:/Proyek Programming/cutkit-app/mcp/server.js"] }
  }
}
```

`cutkit-cli` juga bisa dipakai langsung dari terminal, hasilnya JSON:

```bash
target/release/cutkit-cli info
```

```bash
target/release/cutkit-cli export "D:\Proyek\Shorts.cutkit.json" "D:\hasil.mp4" --quality fast --loudnorm
```

## Model AI dan tempat penyimpanannya

Semua model diunduh dari sumber resmi lewat HTTPS dan dicek ukuran serta SHA-256-nya sebelum dipakai. Folder model bisa diganti di **Proyek → Cek sistem & ffmpeg** (di laptop pemilik: `D:\DevTools\cutkit-models`).

| Model | Sumber | Lisensi | Ukuran |
|---|---|---|---|
| Whisper base/small/medium (ggml) | huggingface.co/ggerganov/whisper.cpp | MIT | 60/190/539 MB |
| Silero VAD (opsional, lambat) | huggingface.co/ggml-org/whisper-vad | MIT | 0,9 MB |
| Piper 2023.11.14-2 (mesin TTS) | github.com/rhasspy/piper | MIT | 22 MB |
| Suara id_ID news_tts medium | huggingface.co/rhasspy/piper-voices | lihat MODEL_CARD | 63 MB |
| U²-Net kecil / IS-Net (hapus background) | github.com/danielgatis/rembg (rilis model) | Apache 2.0 | 4,6/179 MB |

Hapus background juga memasang lingkungan Python terpisah (`py\` di folder model, sekitar 150 MB) berisi onnxruntime, numpy, dan pillow; butuh Python 3 yang sudah terpasang.

Auto caption memakai filter `whisper` bawaan ffmpeg (build Gyan sudah menyertakan whisper.cpp). Deteksi suara (VAD) dimatikan secara bawaan karena di filter ini 25x lebih lambat tanpa menambah jumlah kata yang terbaca.

## Asal ide fitur

Beberapa fitur (kecepatan, Ken Burns, latar blur, chroma key, loudness, animasi pop caption, transisi, efek, filter warna, keyframe, ketukan, hapus background) dipilih setelah menginventaris model editing CapCut 4.6 yang terpasang di laptop dengan REA: hanya nama kelas dan field dari tabel export DLL-nya (misalnya `MaterialSpeed`, `MaterialCanvas`, `MaterialChroma`, `MaterialLoudness`, `KeyframeVideo`) untuk melihat fitur apa yang umum di editor Shorts. Tidak ada kode, aset, template, atau efek CapCut yang disalin; semua implementasi di sini ditulis sendiri di atas ffmpeg.

## Batasan saat ini

- Teks dan caption selalu berada di atas gambar/video (sama di preview dan ekspor).
- Volume di atas 100% hanya terdengar di hasil ekspor; preview dibatasi 100%.
- GIF animasi diputar satu kali; duplikat klipnya untuk mengulang.
- Font harus terpasang di Windows. Preview dan ekspor memakai font yang sama, tapi tebal tepi bisa beda tipis.
- Keyframe belum tersedia untuk klip teks (teks dirender libass); geser/skala teks tetap bisa diatur statis.
- Pustaka efek, stiker, dan template bersifat lokal (bawaan + buatan sendiri), belum ada katalog online.
- Hapus background video memproses tiap frame di CPU: klip 10 detik dengan model cepat butuh sekitar satu menit, model akurat lebih lama.

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
crates/cutkit-cli/    cutkit-cli: probe, ekspor, transkripsi, TTS, ketukan, hapus background dalam JSON
mcp/server.js         Server MCP (stdio, tanpa dependensi)
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
- `crates/cutkit-core/tests/export_real.rs`: membuat klip sintetis dengan ffmpeg, mengekspor proyek berisi potongan, overlay berputar, SFX, teks, dan caption, lalu memeriksa hasilnya dengan ffprobe. Juga membuat semua media proyek contoh sungguhan, dan satu proyek berisi semua efek (kecepatan, latar blur, zoom, chroma key, caption pop, loudness) yang hasilnya diperiksa piksel per piksel dan diukur dengan ebur128. Dilewati bila ffmpeg tidak ada.
- `tests/mcp.test.js`: menjalankan server MCP seperti klien sungguhan (initialize, tools/list, edit proyek), lalu menyusun video cepat dan mengekspornya lewat `cutkit-cli` (dilewati bila `cutkit-cli` belum dibuild).
- `tests/timeline.test.js` juga menguji susun otomatis: pembagian durasi, potongan menempel ke awal kata, musik, dan caption.

## Lisensi

MIT. CutKit dibangun dari nol berdasarkan perilaku editor video yang umum dikenal; tidak memakai kode editor lain.
