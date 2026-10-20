//! Local model lifecycle and isolated ONNX inference, independent of a library DB.
mod integrity;
mod models;
pub use integrity::verify_runtime;
mod runtime;
pub use models::*;
pub use runtime::{infer, infer_controlled, set_keep_loaded, unload};
pub type Result<T> = std::result::Result<T, String>;
