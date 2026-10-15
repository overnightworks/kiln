//! The harness binary's command-line arguments (protocol.md §13): parsed
//! here so a malformed or forbidden argument is rejected by tested code
//! before any process, file or measurement starts. Every `Err` carries
//! the message the binary prints.

use std::path::PathBuf;
use std::time::Duration;

use crate::machine;

/// The `pilot` subcommand's arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PilotArgs {
    pub output_dir: PathBuf,
    pub passthrough_binary: PathBuf,
    pub manifest: PathBuf,
    pub monitor_interval: Duration,
}

impl PilotArgs {
    /// Parses `--output-dir`, `--passthrough-binary`, `--manifest` and the
    /// optional `--monitor-interval-ms`. The warm-up and operation counts
    /// are frozen by protocol §13 and have no option: asking for either is
    /// refused by name.
    pub fn parse(args: &[String]) -> Result<PilotArgs, String> {
        let mut output_dir = None;
        let mut passthrough_binary = None;
        let mut manifest = None;
        let mut monitor_interval = machine::SAMPLE_INTERVAL;

        let mut iter = args.iter();
        while let Some(flag) = iter.next() {
            if matches!(flag.as_str(), "--warmup" | "--operations") {
                return Err(format!(
                    "{flag} is refused: the pilot's warm-up and operation counts are frozen \
                     by protocol §13 and cannot be set"
                ));
            }
            let value = iter.next().ok_or_else(|| format!("{flag} needs a value"))?;
            match flag.as_str() {
                "--output-dir" => output_dir = Some(PathBuf::from(value)),
                "--passthrough-binary" => passthrough_binary = Some(PathBuf::from(value)),
                "--manifest" => manifest = Some(PathBuf::from(value)),
                "--monitor-interval-ms" => monitor_interval = parse_interval_ms(value)?,
                other => return Err(format!("unknown pilot argument {other:?}")),
            }
        }

        match (output_dir, passthrough_binary, manifest) {
            (Some(output_dir), Some(passthrough_binary), Some(manifest)) => Ok(PilotArgs {
                output_dir,
                passthrough_binary,
                manifest,
                monitor_interval,
            }),
            _ => Err(
                "pilot needs --output-dir <dir>, --passthrough-binary <path> and --manifest <path>"
                    .to_string(),
            ),
        }
    }
}

/// A sampling interval in whole milliseconds; zero would make the monitor
/// spin and expect no samples at all.
pub fn parse_interval_ms(value: &str) -> Result<Duration, String> {
    match value.parse::<u64>() {
        Ok(milliseconds) if milliseconds > 0 => Ok(Duration::from_millis(milliseconds)),
        _ => Err(format!(
            "the monitor interval must be a positive number of milliseconds, got {value:?}"
        )),
    }
}

/// The `preflight` subcommand's arguments: nothing, or the manifest to
/// write and the passthrough binary to record in it.
pub fn parse_preflight_args(args: &[String]) -> Result<Option<(PathBuf, PathBuf)>, String> {
    match args {
        [] => Ok(None),
        [manifest, passthrough] => Ok(Some((PathBuf::from(manifest), PathBuf::from(passthrough)))),
        _ => Err(
            "usage: kiln-e4-harness preflight [<manifest-path> <passthrough-binary>]".to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| part.to_string()).collect()
    }

    const REQUIRED: [&str; 6] = [
        "--output-dir",
        "out",
        "--passthrough-binary",
        "pass",
        "--manifest",
        "manifest.txt",
    ];

    mod kiln_e4_21 {
        use super::*;

        #[test]
        fn the_pilot_refuses_to_change_its_frozen_operation_and_warmup_counts() {
            for option in ["--operations", "--warmup"] {
                let mut with_count = REQUIRED.to_vec();
                with_count.extend([option, "5"]);

                let message = PilotArgs::parse(&args(&with_count)).unwrap_err();

                assert!(
                    message.contains(option) && message.contains("frozen"),
                    "{message}"
                );
            }
        }

        #[test]
        fn the_count_options_are_refused_even_when_nothing_else_is_given() {
            assert!(
                PilotArgs::parse(&args(&["--operations", "5"]))
                    .unwrap_err()
                    .contains("frozen")
            );
        }

        #[test]
        fn the_pilot_takes_its_directory_binary_and_manifest_and_the_default_interval() {
            let parsed = PilotArgs::parse(&args(&REQUIRED)).expect("the required arguments");

            assert_eq!(
                parsed,
                PilotArgs {
                    output_dir: PathBuf::from("out"),
                    passthrough_binary: PathBuf::from("pass"),
                    manifest: PathBuf::from("manifest.txt"),
                    monitor_interval: machine::SAMPLE_INTERVAL,
                }
            );
        }

        #[test]
        fn the_pilot_refuses_a_missing_option_an_unknown_one_and_a_missing_value() {
            for bad in [
                args(&REQUIRED[..4]),
                args(&[&REQUIRED[..], &["--verbose", "yes"]].concat()),
                args(&[&REQUIRED[..], &["--manifest"]].concat()),
                args(&[]),
            ] {
                assert!(PilotArgs::parse(&bad).is_err(), "{bad:?}");
            }
        }

        #[test]
        fn the_monitor_interval_is_a_positive_number_of_milliseconds() {
            let mut with_interval = REQUIRED.to_vec();
            with_interval.extend(["--monitor-interval-ms", "50"]);
            assert_eq!(
                PilotArgs::parse(&args(&with_interval))
                    .unwrap()
                    .monitor_interval,
                Duration::from_millis(50)
            );

            for bad in ["0", "-5", "soon", ""] {
                assert!(parse_interval_ms(bad).is_err(), "{bad:?}");
            }
        }
    }

    mod kiln_e4_09 {
        use super::*;

        #[test]
        fn preflight_takes_both_a_manifest_path_and_a_passthrough_binary_or_neither() {
            assert_eq!(parse_preflight_args(&[]), Ok(None));
            assert_eq!(
                parse_preflight_args(&args(&["manifest.txt", "pass"])),
                Ok(Some((PathBuf::from("manifest.txt"), PathBuf::from("pass"))))
            );
            for bad in [args(&["manifest.txt"]), args(&["a", "b", "c"])] {
                assert!(parse_preflight_args(&bad).unwrap_err().contains("usage"));
            }
        }
    }
}
