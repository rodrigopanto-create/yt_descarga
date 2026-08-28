pub mod ffmpeg;
pub mod finder;
pub mod ytdlp;

pub use finder::{resolver_binarios, BinaryPaths};
pub use ytdlp::YtDlpCommand;