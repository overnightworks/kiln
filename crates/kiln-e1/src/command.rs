use std::fmt::{self, Write as _};
use std::fs;
use std::path::{Path, PathBuf};

use crate::classified::ClassifiedUnit;
use crate::evaluation::{evaluate, Evaluation, Refusal};
use crate::jsonl::{parse_units, render_units, JsonlError};
use crate::manifest::{Manifest, ManifestError};
use crate::tpcc::tpcc_units;

const USAGE: &str = "usage: kiln-e1 verdict --manifest <csv> --tpcc <jsonl> --applications <jsonl>\n       kiln-e1 tpcc --out <jsonl> --sensitivity-out <jsonl>";
const MEDIAN_LABEL: &str = "unweighted — no trace source exists";

#[derive(Debug)]
pub enum CommandError {
    Usage(String),
    Read {
        path: PathBuf,
        error: std::io::Error,
    },
    Write {
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
            Self::Write { path, error } => {
                write!(formatter, "cannot write {}: {error}", path.display())
            }
            Self::Manifest(error) => error.fmt(formatter),
            Self::Units(error) => error.fmt(formatter),
            Self::Refused(refusal) => write!(formatter, "refused: {refusal}"),
        }
    }
}

impl std::error::Error for CommandError {}

/// Runs `kiln-e1` for the arguments after the program name and returns the
/// report to print.
pub fn run_command(arguments: &[String]) -> Result<String, CommandError> {
    let [command, flags @ ..] = arguments else {
        return Err(CommandError::Usage("missing command".to_owned()));
    };
    match command.as_str() {
        "verdict" => run_verdict(flags),
        "tpcc" => run_tpcc(flags),
        other => Err(CommandError::Usage(format!("unknown command `{other}`"))),
    }
}

/// The values of a command's `--flag value` pairs, each flag given once.
struct Flags(Vec<(String, PathBuf)>);

impl Flags {
    fn parse(arguments: &[String], accepted: &[&str]) -> Result<Self, CommandError> {
        let mut given: Vec<(String, PathBuf)> = Vec::new();
        for pair in arguments.chunks(2) {
            let [flag, value] = pair else {
                return Err(CommandError::Usage(format!("`{}` needs a value", pair[0])));
            };
            if !accepted.contains(&flag.as_str()) {
                return Err(CommandError::Usage(format!("unknown flag `{flag}`")));
            }
            given.push((flag.clone(), PathBuf::from(value)));
        }
        Ok(Self(given))
    }

    fn require(&self, flag: &str) -> Result<&Path, CommandError> {
        self.0
            .iter()
            .rev()
            .find(|(given, _)| given == flag)
            .map(|(_, path)| path.as_path())
            .ok_or_else(|| CommandError::Usage(format!("missing `{flag}`")))
    }
}

fn run_verdict(arguments: &[String]) -> Result<String, CommandError> {
    let flags = Flags::parse(arguments, &["--manifest", "--tpcc", "--applications"])?;
    let manifest_path = flags.require("--manifest")?;
    let tpcc_path = flags.require("--tpcc")?;
    let applications_path = flags.require("--applications")?;
    let manifest = Manifest::parse(&read(manifest_path)?).map_err(CommandError::Manifest)?;
    let tpcc = read_units(tpcc_path)?;
    let applications = read_units(applications_path)?;
    let evaluation = evaluate(&manifest, &tpcc, &applications).map_err(CommandError::Refused)?;
    Ok(render(&evaluation))
}

/// Writes the lowered and classified TPC-C units; the report counts units and
/// states no share and no verdict.
fn run_tpcc(arguments: &[String]) -> Result<String, CommandError> {
    let flags = Flags::parse(arguments, &["--out", "--sensitivity-out"])?;
    let out = flags.require("--out")?;
    let sensitivity_out = flags.require("--sensitivity-out")?;
    let units = tpcc_units();
    write(out, &render_units(&units.gating))?;
    write(sensitivity_out, &render_units(&units.sensitivity))?;
    Ok(format!(
        "wrote {} units to {}\nwrote {} sensitivity units to {}\n",
        units.gating.len(),
        out.display(),
        units.sensitivity.len(),
        sensitivity_out.display()
    ))
}

fn read(path: &Path) -> Result<String, CommandError> {
    fs::read_to_string(path).map_err(|error| CommandError::Read {
        path: path.to_owned(),
        error,
    })
}

fn write(path: &Path, text: &str) -> Result<(), CommandError> {
    fs::write(path, text).map_err(|error| CommandError::Write {
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
    let share = |percent: Option<crate::ratio::Ratio>| {
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
