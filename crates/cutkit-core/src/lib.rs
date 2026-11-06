//! cutkit-core: model proyek CutKit, pembuat ASS, penyusun perintah ffmpeg, dan pelarinya.
//! Tidak bergantung pada Tauri sehingga semua logika bisa diuji dengan `cargo test`.

pub mod ass;
pub mod demo;
pub mod export;
pub mod ffmpeg;
pub mod layout;
pub mod model;
pub mod probe;
pub mod proxy;

pub use export::{build_plan, Encoder, ExportOptions, ExportPlan, Quality};
pub use ffmpeg::{find_tools, Progress, Tools};
pub use model::*;
pub use probe::MediaInfo;
