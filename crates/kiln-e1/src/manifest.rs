use std::collections::BTreeSet;
use std::fmt;

use crate::unit::ApplicationId;

const APPLICATION_COLUMN: &str = "owner_repo";
const FIELD_SEPARATOR: char = ',';

#[derive(Debug, PartialEq, Eq)]
pub struct ManifestError {
    line: usize,
    message: String,
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "manifest line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ManifestError {}

/// The frozen application set (`inputs.md` §2), read from `corpus-manifest.csv`.
///
/// A plain split suffices because no frozen field is quoted or contains a
/// comma; a line that would need quoting is rejected instead of misread.
#[derive(Debug, PartialEq, Eq)]
pub struct Manifest {
    applications: BTreeSet<ApplicationId>,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let mut lines = text
            .lines()
            .enumerate()
            .map(|(index, line)| (index + 1, line));
        let (header_line, header) = lines.next().ok_or(ManifestError {
            line: 1,
            message: "the manifest is empty".to_owned(),
        })?;
        let columns: Vec<&str> = header.split(FIELD_SEPARATOR).collect();
        let application_index = columns
            .iter()
            .position(|column| *column == APPLICATION_COLUMN)
            .ok_or(ManifestError {
                line: header_line,
                message: format!("the header has no `{APPLICATION_COLUMN}` column"),
            })?;

        let mut applications = BTreeSet::new();
        for (line, row) in lines {
            let fail = |message: String| ManifestError { line, message };
            if row.contains('"') {
                return Err(fail("quoted fields are not supported".to_owned()));
            }
            let fields: Vec<&str> = row.split(FIELD_SEPARATOR).collect();
            if fields.len() != columns.len() {
                return Err(fail(format!(
                    "expected {} fields, found {}",
                    columns.len(),
                    fields.len()
                )));
            }
            let id = fields[application_index];
            if id.is_empty() {
                return Err(fail(format!("`{APPLICATION_COLUMN}` is empty")));
            }
            if !applications.insert(ApplicationId::new(id)) {
                return Err(fail(format!("duplicate application `{id}`")));
            }
        }
        Ok(Self { applications })
    }

    pub fn applications(&self) -> &BTreeSet<ApplicationId> {
        &self.applications
    }
}
