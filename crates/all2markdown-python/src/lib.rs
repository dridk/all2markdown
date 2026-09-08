use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// Extract the text of a document file as Markdown.
///
/// Args:
///     file_path: Path to the document file
///     format: Optional format id ("doc", "docx", "rtf", "pdf")
///
/// Returns:
///     Markdown string
#[pyfunction]
#[pyo3(signature = (file_path, format=None))]
fn extract(file_path: &str, format: Option<&str>) -> PyResult<String> {
    let data = std::fs::read(file_path)
        .map_err(|e| PyValueError::new_err(format!("Cannot read file: {e}")))?;

    let options = match format {
        Some(id) => all2markdown_core::Options::forcing(
            all2markdown_core::Format::from_id(id)
                .ok_or_else(|| PyValueError::new_err(format!("unsupported format: {id}")))?,
        ),
        None => all2markdown_core::Options::default(),
    };

    let source = match std::path::Path::new(file_path).file_name().and_then(|n| n.to_str()) {
        Some(name) => all2markdown_core::SourceDocument::named(name, &data),
        None => all2markdown_core::SourceDocument::from_bytes(&data),
    };

    all2markdown_core::extract(source, &options)
        .map(|extraction| extraction.markdown)
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pymodule]
fn all2markdown(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(extract, m)?)?;
    Ok(())
}
