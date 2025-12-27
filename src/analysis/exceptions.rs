use crate::core::types::{CodeLocation, RaiseStatement};
use crate::plugins::python::extractor;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExceptionError {
    #[error("Failed to extract exception: {0}")]
    ExtractionFailed(String),

    #[error("Extractor error: {0}")]
    Extractor(#[from] extractor::ExtractorError),
}

pub fn extract_exceptions(
    tree: &tree_sitter::Tree,
    content: &str,
    file_path: &Path,
    line_start: u32,
    line_end: u32,
) -> Result<Vec<RaiseStatement>, ExceptionError> {
    Ok(extractor::extract_raises_in_range(tree, content, file_path, line_start, line_end)?)
}

pub fn extract_all_exceptions(
    tree: &tree_sitter::Tree,
    content: &str,
    file_path: &Path,
) -> Result<Vec<RaiseStatement>, ExceptionError> {
    Ok(extractor::extract_raises(tree, content, file_path)?)
}

pub fn find_exception_definition(
    _exc_type: &str,
    _qualified_type: &str,
) -> Option<CodeLocation> {
    extractor::find_exception_definition(_exc_type)
}
