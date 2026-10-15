use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use kiln_e1::{
    evaluate, parse_units, render_units, tpcc_units as lowered_tpcc_units, Classification,
    ClassifiedUnit, Evidence, ProvenCategory, Reason, TpccTransaction,
};

#[path = "support/units.rs"]
mod units;
use units::*;

mod kiln_e1_35 {
    use super::*;

    fn unknown_with_evidence(subject: &str) -> ClassifiedUnit {
        let unit = unknown(subject).unit().clone();
        let classification = Classification::Unknown {
            reason: Reason::new("range over an undeclared index").unwrap(),
            evidence: Some(Evidence::new(vec!["f/body/effects[1]".to_owned()]).unwrap()),
        };
        ClassifiedUnit::new(unit, classification)
    }

    fn one_of_each_classification() -> Vec<ClassifiedUnit> {
        let mut written = vec![
            proven("a/b", ProvenCategory::Static),
            proven("a/b", ProvenCategory::OneHop),
            proven(TpccTransaction::Payment, ProvenCategory::DomainBounded),
            gating("c/d"),
            unknown("c/d"),
            unknown_with_evidence("c/d"),
            excluded("e/f"),
        ];
        written.extend(units_with_share("g/h", 2, 3));
        written.extend(tpcc_units(&[TpccTransaction::Payment]));
        written.extend(tpcc_units_with_shares([(1, 2); 5]));
        written
    }

    #[test]
    fn every_kind_of_unit_reads_back_as_the_unit_that_was_written() {
        let written = one_of_each_classification();

        let read = parse_units("written.jsonl", &render_units(&written)).unwrap();

        assert_eq!(read, written);
    }

    #[test]
    fn the_same_units_always_give_the_same_bytes_one_line_each() {
        let written = one_of_each_classification();

        let text = render_units(&written);

        assert_eq!(text, render_units(&written));
        assert_eq!(text.lines().count(), written.len());
        assert!(text.ends_with('\n'));
    }
}

fn scratch_file(name: &str) -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("tpcc-{name}"));
    fs::create_dir_all(&directory).unwrap();
    directory.join("units.jsonl")
}

fn tpcc(arguments: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kiln-e1"))
        .arg("tpcc")
        .args(arguments)
        .output()
        .unwrap()
}

fn write_units(name: &str) -> (Output, PathBuf, PathBuf) {
    let out = scratch_file(&format!("{name}-gating"));
    let sensitivity = scratch_file(&format!("{name}-sensitivity"));
    let output = tpcc(&[
        "--out".as_ref(),
        out.as_os_str(),
        "--sensitivity-out".as_ref(),
        sensitivity.as_os_str(),
    ]);
    (output, out, sensitivity)
}

mod kiln_e1_36 {
    use super::*;

    #[test]
    fn writes_one_unit_per_transaction_type_in_mix_order_and_the_sensitivity_units_apart() {
        let (output, out, sensitivity) = write_units("layout");

        assert!(output.status.success(), "{output:?}");
        let gating = parse_units("out", &fs::read_to_string(out).unwrap()).unwrap();
        let subjects: Vec<_> = gating
            .iter()
            .map(|unit| unit.unit().subject.clone())
            .collect();
        assert_eq!(subjects, TpccTransaction::ALL.map(kiln_e1::Subject::Tpcc));
        let sensitivity =
            parse_units("sensitivity", &fs::read_to_string(sensitivity).unwrap()).unwrap();
        assert_eq!(sensitivity.len(), 2);
        assert_eq!(gating, lowered_tpcc_units().gating);
    }

    #[test]
    fn the_gating_file_passes_the_verdict_reader_and_its_evidence_gate() {
        let (_, out, _) = write_units("gate");

        let tpcc = parse_units("out", &fs::read_to_string(out).unwrap()).unwrap();

        evaluate(&manifest(&["a/b"]), &tpcc, &units_with_share("a/b", 1, 1)).unwrap();
    }

    #[test]
    fn writes_the_same_bytes_on_every_run_and_prints_no_share_and_no_verdict() {
        let (first, out, _) = write_units("repeat");
        let first_bytes = fs::read(&out).unwrap();
        let (second, out, _) = write_units("repeat");

        assert_eq!(first_bytes, fs::read(out).unwrap());
        let stdout = String::from_utf8(first.stdout).unwrap();
        assert_eq!(stdout, String::from_utf8(second.stdout).unwrap());
        assert!(
            !stdout.contains('%') && !stdout.contains("Verdict"),
            "{stdout}"
        );
    }

    #[test]
    fn a_missing_flag_or_an_unwritable_path_prints_the_reason_and_exits_non_zero() {
        let out = scratch_file("missing");
        let unwritable = out.parent().unwrap().join("no-such-directory/units.jsonl");
        let missing_flag = tpcc(&["--out".as_ref(), out.as_os_str()]);
        let unknown_flag = tpcc(&["--elsewhere".as_ref(), out.as_os_str()]);
        let unwritable_path = tpcc(&[
            "--out".as_ref(),
            unwritable.as_os_str(),
            "--sensitivity-out".as_ref(),
            out.as_os_str(),
        ]);

        for (output, reason) in [
            (missing_flag, "missing `--sensitivity-out`"),
            (unknown_flag, "unknown flag `--elsewhere`"),
            (unwritable_path, "cannot write"),
        ] {
            let stderr = String::from_utf8(output.stderr).unwrap();
            assert!(!output.status.success());
            assert!(stderr.contains(reason), "{stderr}");
            assert!(output.stdout.is_empty());
        }
    }

    #[test]
    fn identical_or_aliased_destinations_are_rejected_before_anything_is_written() {
        let out = scratch_file("collision");
        let aliased = out.parent().unwrap().join("../tpcc-collision/units.jsonl");
        fs::remove_file(&out).ok();

        for sensitivity in [&out, &aliased] {
            let output = tpcc(&[
                "--out".as_ref(),
                out.as_os_str(),
                "--sensitivity-out".as_ref(),
                sensitivity.as_os_str(),
            ]);

            let stderr = String::from_utf8(output.stderr).unwrap();
            assert!(!output.status.success());
            assert!(stderr.contains("name the same file"), "{stderr}");
            assert!(!out.exists());
        }
    }

    fn contents_survive_a_rejected_alias(alias: impl FnOnce(&PathBuf, &PathBuf)) {
        let name = format!("alias-{}", std::thread::current().name().unwrap());
        let out = scratch_file(&format!("{name}-gating"));
        let sensitivity = scratch_file(&format!("{name}-sensitivity"));
        fs::remove_file(&sensitivity).ok();
        fs::remove_file(&out).ok();
        alias(&out, &sensitivity);

        let output = tpcc(&[
            "--out".as_ref(),
            out.as_os_str(),
            "--sensitivity-out".as_ref(),
            sensitivity.as_os_str(),
        ]);

        assert!(!output.status.success());
        assert!(!out.exists() || fs::read_to_string(&out).unwrap() == "kept\n");
        assert!(fs::read_to_string(&sensitivity).map_or(true, |text| text == "kept\n"));
    }

    #[test]
    fn a_dangling_symlink_to_the_other_destination_is_rejected_and_nothing_is_written() {
        contents_survive_a_rejected_alias(|out, sensitivity| {
            std::os::unix::fs::symlink(out, sensitivity).unwrap();
        });
    }

    #[test]
    fn a_hard_link_to_the_other_destination_is_rejected_and_keeps_its_contents() {
        contents_survive_a_rejected_alias(|out, sensitivity| {
            fs::write(out, "kept\n").unwrap();
            fs::hard_link(out, sensitivity).unwrap();
        });
    }
}
