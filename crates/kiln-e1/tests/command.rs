use std::path::PathBuf;
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn verdict(manifest: &str, tpcc: &str, applications: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kiln-e1"))
        .arg("verdict")
        .args(["--manifest".as_ref(), fixture(manifest).as_os_str()])
        .args(["--tpcc".as_ref(), fixture(tpcc).as_os_str()])
        .args(["--applications".as_ref(), fixture(applications).as_os_str()])
        .output()
        .unwrap()
}

mod kiln_e1_16 {
    use super::*;

    #[test]
    fn prints_every_share_the_labelled_median_and_the_verdict() {
        let output = verdict("manifest.csv", "tpcc.jsonl", "applications.jsonl");
        let stdout = String::from_utf8(output.stdout).unwrap();

        assert!(output.status.success(), "{stdout}");
        assert!(stdout.contains("alpha/one: 80.0000 %"), "{stdout}");
        assert!(stdout.contains("beta/two: 75.0000 %"), "{stdout}");
        assert!(stdout.contains("gamma/three: 50.0000 %"), "{stdout}");
        assert!(stdout.contains("1 S4-excluded"), "{stdout}");
        assert!(stdout.contains("weighted share: 100.0000 %"), "{stdout}");
        assert!(
            stdout.contains("median share (unweighted — no trace source exists): 75.0000 %"),
            "{stdout}"
        );
        assert!(stdout.contains("Verdict: PASS"), "{stdout}");
    }

    #[test]
    fn a_refusal_prints_its_reason_to_stderr_and_exits_non_zero() {
        let output = verdict("manifest.csv", "tpcc.jsonl", "missing-application.jsonl");
        assert!(!output.status.success());
        assert!(String::from_utf8(output.stderr)
            .unwrap()
            .contains("missing-application.jsonl"));
        assert!(output.stdout.is_empty());

        let output = verdict(
            "manifest-with-extra.csv",
            "tpcc.jsonl",
            "applications.jsonl",
        );
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(!output.status.success());
        assert!(stderr.contains("delta/four"), "{stderr}");
    }

    #[test]
    fn invalid_arguments_print_the_usage_and_exit_non_zero() {
        let output = Command::new(env!("CARGO_BIN_EXE_kiln-e1"))
            .arg("verdict")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8(output.stderr).unwrap().contains("usage:"));
    }
}
