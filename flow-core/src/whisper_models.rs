//! Whisper model management utilities

use crate::error::{Error, Result};
use std::path::PathBuf;

/// Get default model directory (~/Library/Application Support/FlowWispr/models)
pub fn get_models_dir() -> Result<PathBuf> {
    let app_support = dirs::data_local_dir()
        .ok_or_else(|| Error::Config("Failed to get application support directory".to_string()))?;

    let models_dir = app_support.join("FlowWispr").join("models");

    if !models_dir.exists() {
        std::fs::create_dir_all(&models_dir)?;
    }

    Ok(models_dir)
}
