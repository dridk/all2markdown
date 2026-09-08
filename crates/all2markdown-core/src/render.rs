use crate::batch::BatchItem;
use serde::Serialize;

/// One Batch result as a JSON line.
///
/// Every key is always present, `null` where there is nothing to say. An
/// output whose shape varies with its content is hostile to whatever parses
/// it downstream, and the whole point of this format is that one pass over a
/// corpus answers both what worked and what did not.
#[derive(Serialize)]
struct Line<'a> {
    source: &'a str,
    format: Option<&'a str>,
    encoding: Option<&'a str>,
    text: Option<&'a str>,
    warnings: Vec<String>,
    error: Option<String>,
}

/// Render one Batch result as a JSONL line, without its newline.
pub fn to_jsonl(item: &BatchItem) -> String {
    let line = match &item.result {
        Ok(extraction) => Line {
            source: &item.source,
            format: Some(extraction.format.id()),
            encoding: extraction.encoding.as_deref(),
            text: Some(&extraction.markdown),
            warnings: extraction
                .warnings
                .iter()
                .map(ToString::to_string)
                .collect(),
            error: None,
        },
        Err(failure) => Line {
            source: &item.source,
            format: None,
            encoding: None,
            text: None,
            warnings: Vec::new(),
            error: Some(failure.to_string()),
        },
    };

    // Serialising a struct of strings cannot fail; falling back rather than
    // unwrapping keeps the promise that one document never stops a Batch.
    serde_json::to_string(&line).unwrap_or_else(|e| {
        format!(
            r#"{{"source":{},"format":null,"encoding":null,"text":null,"warnings":[],"error":{}}}"#,
            serde_json::to_string(&item.source).unwrap_or_else(|_| "\"?\"".to_owned()),
            serde_json::to_string(&e.to_string()).unwrap_or_else(|_| "\"?\"".to_owned()),
        )
    })
}
