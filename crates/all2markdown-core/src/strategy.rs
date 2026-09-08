use crate::error::Error;

pub trait FormatParser {
    fn to_markdown(&self, data: &[u8]) -> Result<String, Error>;
}
