pub mod python;
pub mod rust;

use anyhow::{Result, bail};
use std::path::PathBuf;

use crate::api::types::QuestionDetail;

pub fn scaffold_problem(
    workspace: &PathBuf,
    detail: &QuestionDetail,
    language: &str,
) -> Result<PathBuf> {
    match language {
        "rust" => rust::scaffold_rust(workspace, detail),
        "python3" | "python" => python::scaffold_python(workspace, detail),
        _ => bail!("Unsupported language for scaffolding: {}", language),
    }
}
