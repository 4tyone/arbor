use crate::core::types::NoneSource;
use crate::plugins::python::extractor;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum NoneSourceError {
    #[error("Failed to extract None source: {0}")]
    ExtractionFailed(String),

    #[error("Extractor error: {0}")]
    Extractor(#[from] extractor::ExtractorError),
}

pub fn extract_none_sources(
    tree: &tree_sitter::Tree,
    content: &str,
    file_path: &Path,
    line_start: u32,
    line_end: u32,
) -> Result<Vec<NoneSource>, NoneSourceError> {
    Ok(extractor::extract_none_sources_in_range(tree, content, file_path, line_start, line_end)?)
}

pub fn extract_all_none_sources(
    tree: &tree_sitter::Tree,
    content: &str,
    file_path: &Path,
) -> Result<Vec<NoneSource>, NoneSourceError> {
    Ok(extractor::extract_none_sources(tree, content, file_path)?)
}
