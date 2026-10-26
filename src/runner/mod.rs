pub mod ffmpeg;
pub mod finder;
pub mod process;
pub mod ytdlp;

pub use finder::{resolver_binarios, BinaryPaths};
pub use ytdlp::YtDlpCommand;
