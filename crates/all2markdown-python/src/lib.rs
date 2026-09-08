use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// Parse a document file and return Markdown.
///
/// Args:
///     file_path: Path to the document file
///     format: Optional format string ("doc", "docx", "rtf", "pdf")
///
/// Returns:
///     Markdown string
#[pyfunction]
#[pyo3(signature = (file_path, format=None))]
fn parse(file_path: &str, format: Option<&str>) -> PyResult<String> {
    let data = std::fs::read(file_path)
        .map_err(|e| PyValueError::new_err(format!("Cannot read file: {e}")))?;
    let fmt = format
        .map(|f| all2md_core::Format::from_str_loose(f))
        .transpose()
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    all2md_core::parse(&data, fmt).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pymodule]
fn all2md(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    Ok(())
}
