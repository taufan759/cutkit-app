//! cutkit-core: model proyek CutKit, pembuat ASS, penyusun perintah ffmpeg, dan pelarinya.
//! Tidak bergantung pada Tauri sehingga semua logika bisa diuji dengan `cargo test`.

pub mod model;

pub use model::*;
