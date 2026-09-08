use crate::error::All2mdError;

pub trait FormatParser {
    fn to_markdown(&self, data: &[u8]) -> Result<String, All2mdError>;
}
