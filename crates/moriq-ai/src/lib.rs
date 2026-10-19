//! Local model lifecycle and isolated ONNX inference, independent of a library DB.
mod models;
mod runtime;
pub use models::*;
pub use runtime::{infer, set_keep_loaded, unload};
pub type Result<T> = std::result::Result<T, String>;
