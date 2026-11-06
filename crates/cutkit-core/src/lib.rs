//! cutkit-core: model proyek CutKit, pembuat ASS, penyusun perintah ffmpeg, dan pelarinya.
//! Tidak bergantung pada Tauri sehingga semua logika bisa diuji dengan `cargo test`.

pub mod ai;
pub mod ass;
pub mod beats;
pub mod color;
pub mod demo;
pub mod export;
pub mod ffmpeg;
pub mod layout;
pub mod model;
pub mod probe;
pub mod proxy;
pub mod tts;

pub use export::{build_job, build_plan, Encoder, ExportJob, ExportOptions, ExportPlan, ExportStep, Quality};
pub use ffmpeg::{find_tools, Progress, Tools};
pub use model::*;
pub use probe::MediaInfo;
