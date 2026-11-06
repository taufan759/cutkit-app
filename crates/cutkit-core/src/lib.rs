//! cutkit-core: model proyek CutKit, pembuat ASS, penyusun perintah ffmpeg, dan pelarinya.
//! Tidak bergantung pada Tauri sehingga semua logika bisa diuji dengan `cargo test`.

pub mod ass;
pub mod model;
pub mod probe;
pub mod proxy;

pub use model::*;
pub use probe::MediaInfo;
