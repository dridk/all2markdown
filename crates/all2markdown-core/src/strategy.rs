use crate::failure::Failure;

pub trait FormatParser {
    fn to_markdown(&self, data: &[u8]) -> Result<String, Failure>;
}
