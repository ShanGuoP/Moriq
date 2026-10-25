//! Qwen model management and native llama.cpp inference.
mod llama;
mod models;
pub use moriq_runtime::verify_runtime;
mod runtime;
pub use llama::infer_with_profile as llama_infer_profile;
pub use llama::{infer as llama_infer, unload as unload_llama};
pub use models::{
    DownloadControl, Hardware, ModelFile, ModelInfo, ModelSpec, hardware, install,
    install_with_mirror, installed_path, inventory, model_root, registry, uninstall,
};
pub use runtime::{infer, infer_controlled, set_keep_loaded, unload};
pub type Result<T> = std::result::Result<T, String>;
