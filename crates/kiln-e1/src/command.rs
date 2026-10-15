use std::fmt::{self, Write as _};
use std::fs;
use std::path::{Path, PathBuf};

use crate::classified::ClassifiedUnit;
use crate::evaluation::{evaluate, Evaluation, Refusal};
use crate::jsonl::{parse_units, JsonlError};
use crate::manifest::{Manifest, ManifestError};

const USAGE: &str = "usage: kiln-e1 verdict --manifest <csv> --tpcc <jsonl> --applications <jsonl>";
const MEDIAN_LABEL: &str = "unweighted — no trace source exists";

#[derive(Debug)]
pub enum CommandError {
    Usage(String),
    Read {
        path: PathBuf,
        error: std::io::Error,
    },
    Manifest(ManifestError),
    Units(JsonlError),
    Refused(Refusal),
}

impl fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(problem) => write!(formatter, "{problem}\n{USAGE}"),
            Self::Read { path, error } => {
                write!(formatter, "cannot read {}: {error}", path.display())
            }
            Self::Manifest(error) => error.fmt(formatter),
            Self::Units(error) => error.fmt(formatter),
            Self::Refused(refusal) => write!(formatter, "refused: {refusal}"),
        }
    }
}

impl std::error::Error for CommandError {}

struct VerdictInputs {
    manifest: PathBuf,
    tpcc: PathBuf,
    applications: PathBuf,
}

/// Runs `kiln-e1 verdict` for the arguments after the program name and returns
/// the report to print.
pub fn run_verdict_command(arguments: &[String]) -> Result<String, CommandError> {
    let inputs = parse_arguments(arguments)?;
    let manifest = Manifest::parse(&read(&inputs.manifest)?).map_err(CommandError::Manifest)?;
    let tpcc = read_units(&inputs.tpcc)?;
    let applications = read_units(&inputs.applications)?;
    let evaluation = evaluate(&manifest, &tpcc, &applications).map_err(CommandError::Refused)?;
    Ok(render(&evaluation))
}

fn parse_arguments(arguments: &[String]) -> Result<VerdictInputs, CommandError> {
    let [command, flags @ ..] = arguments else {
        return Err(CommandError::Usage("missing command".to_owned()));
    };
    if command != "verdict" {
        return Err(CommandError::Usage(format!("unknown command `{command}`")));
    }
    let (mut manifest, mut tpcc, mut applications) = (None, None, None);
    for pair in flags.chunks(2) {
        let [flag, value] = pair else {
            return Err(CommandError::Usage(format!("`{}` needs a value", pair[0])));
        };
        let slot = match flag.as_str() {
            "--manifest" => &mut manifest,
            "--tpcc" => &mut tpcc,
            "--applications" => &mut applications,
            other => return Err(CommandError::Usage(format!("unknown flag `{other}`"))),
        };
        *slot = Some(PathBuf::from(value));
    }
    let require = |slot: Option<PathBuf>, flag: &str| {
        slot.ok_or_else(|| CommandError::Usage(format!("missing `{flag}`")))
    };
    Ok(VerdictInputs {
        manifest: require(manifest, "--manifest")?,
        tpcc: require(tpcc, "--tpcc")?,
        applications: require(applications, "--applications")?,
    })
}

fn read(path: &Path) -> Result<String, CommandError> {
    fs::read_to_string(path).map_err(|error| CommandError::Read {
        path: path.to_owned(),
        error,
    })
}

fn read_units(path: &Path) -> Result<Vec<ClassifiedUnit>, CommandError> {
    parse_units(&path.display().to_string(), &read(path)?).map_err(CommandError::Units)
}

fn render(evaluation: &Evaluation) -> String {
    let mut report = String::from("TPC-C (weighted by the frozen mix)\n");
    for (transaction, tally) in &evaluation.tpcc_transactions {
        writeln!(report, "  {transaction}: {}", describe(tally)).unwrap();
    }
    writeln!(
        report,
        "  weighted share: {}\n",
        evaluation.tpcc_weighted_share
    )
    .unwrap();
    report.push_str("Applications\n");
    for (id, tally) in &evaluation.applications {
        writeln!(report, "  {id}: {}", describe(tally)).unwrap();
    }
    writeln!(
        report,
        "  median share ({MEDIAN_LABEL}): {}\n",
        evaluation.application_median
    )
    .unwrap();
    writeln!(report, "Verdict: {}", evaluation.verdict).unwrap();
    report
}

fn describe(tally: &crate::coverage::Tally) -> String {
    let share = |percent: Option<crate::coverage::Percent>| {
        percent.map_or_else(|| "n/a".to_owned(), |percent| percent.to_string())
    };
    format!(
        "{} static or one-hop, domain-bounded {} (not gating), {} units, {} S4-excluded",
        share(tally.gating_share()),
        share(tally.domain_bounded_share()),
        tally.denominator(),
        tally.excluded()
    )
}
