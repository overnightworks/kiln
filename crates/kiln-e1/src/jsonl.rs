use std::fmt;

use crate::classified::ClassifiedUnit;

#[derive(Debug)]
pub struct JsonlError {
    source_name: String,
    line: usize,
    message: String,
}

impl fmt::Display for JsonlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}: {}",
            self.source_name, self.line, self.message
        )
    }
}

impl std::error::Error for JsonlError {}

/// Reads one classified unit per line; a blank line is malformed like any other.
pub fn parse_units(source_name: &str, text: &str) -> Result<Vec<ClassifiedUnit>, JsonlError> {
    text.lines()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str(line).map_err(|error| JsonlError {
                source_name: source_name.to_owned(),
                line: index + 1,
                message: error.to_string(),
            })
        })
        .collect()
}
