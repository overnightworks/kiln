mod support;

use kiln_e1::{evaluate, Percent, Refusal, TpccTransaction, Verdict};
use support::*;

mod kiln_e1_12 {
    use super::*;

    #[test]
    fn thresholds_decide_the_verdict_for_both_shares() {
        let shares = [49.99, 50.0, 69.99, 70.0, 100.0];
        for tpcc in shares {
            for median in shares {
                let expected = if tpcc < 50.0 || median < 50.0 {
                    Verdict::Fail
                } else if tpcc >= 70.0 && median >= 70.0 {
                    Verdict::Pass
                } else {
                    Verdict::Inconclusive
                };
                let verdict = Verdict::from_shares(Percent(tpcc), Percent(median));
                assert_eq!(verdict, expected, "tpcc {tpcc}, median {median}");
            }
        }
    }

    #[test]
    fn a_fail_outranks_a_pass_on_the_other_share() {
        assert_eq!(
            Verdict::from_shares(Percent(100.0), Percent(49.99)),
            Verdict::Fail
        );
        assert_eq!(
            Verdict::from_shares(Percent(49.99), Percent(100.0)),
            Verdict::Fail
        );
    }

    #[test]
    fn evaluation_derives_the_verdict_from_units() {
        let manifest = manifest(&["a/b", "c/d", "e/f"]);
        let mut applications = repeated(7, gating("a/b"));
        applications.extend(repeated(3, unknown("a/b")));
        applications.extend(repeated(7, gating("c/d")));
        applications.extend(repeated(3, unknown("c/d")));
        applications.extend(repeated(1, gating("e/f")));
        let evaluation = evaluate(&manifest, &tpcc_units(&[]), &applications).unwrap();
        assert_eq!(evaluation.application_median, Percent(70.0));
        assert_eq!(evaluation.verdict, Verdict::Pass);
    }
}

mod kiln_e1_13 {
    use super::*;

    #[test]
    fn a_manifest_application_without_units_is_refused_and_named() {
        let manifest = manifest(&["a/b", "c/d"]);
        let refusal = evaluate(&manifest, &tpcc_units(&[]), &[gating("a/b")]).unwrap_err();
        assert_eq!(refusal, Refusal::ApplicationWithoutUnits(id("c/d")));
        assert!(refusal.to_string().contains("c/d"));
    }

    #[test]
    fn an_application_with_only_excluded_units_is_refused_too() {
        let manifest = manifest(&["a/b", "c/d"]);
        let units = [gating("a/b"), excluded("c/d")];
        let refusal = evaluate(&manifest, &tpcc_units(&[]), &units).unwrap_err();
        assert_eq!(refusal, Refusal::ApplicationWithoutUnits(id("c/d")));
    }

    #[test]
    fn an_empty_manifest_is_refused() {
        let refusal = evaluate(&manifest(&[]), &tpcc_units(&[]), &[]).unwrap_err();
        assert_eq!(refusal, Refusal::EmptyManifest);
    }
}

mod kiln_e1_14 {
    use super::*;

    #[test]
    fn a_unit_outside_the_manifest_is_refused_and_named() {
        let manifest = manifest(&["a/b"]);
        let units = [gating("a/b"), gating("x/y")];
        let refusal = evaluate(&manifest, &tpcc_units(&[]), &units).unwrap_err();
        assert_eq!(refusal, Refusal::ApplicationOutsideManifest(id("x/y")));
        assert!(refusal.to_string().contains("x/y"));
    }
}

mod kiln_e1_15 {
    use super::*;

    #[test]
    fn a_tpcc_unit_without_a_transaction_type_is_refused() {
        let mut tpcc = tpcc_units(&[]);
        tpcc.push(gating("refund"));
        let refusal = evaluate(&manifest(&["a/b"]), &tpcc, &[gating("a/b")]).unwrap_err();
        assert_eq!(refusal, Refusal::UnknownTpccTransaction(id("refund")));
    }

    #[test]
    fn a_tpcc_transaction_type_without_units_is_refused() {
        for transaction in TpccTransaction::ALL {
            let tpcc: Vec<_> = tpcc_units(&[])
                .into_iter()
                .filter(|unit| unit.unit().application.as_str() != transaction.name())
                .collect();
            let refusal = evaluate(&manifest(&["a/b"]), &tpcc, &[gating("a/b")]).unwrap_err();
            assert_eq!(refusal, Refusal::TpccTransactionWithoutUnits(transaction));
        }
    }

    #[test]
    fn a_tpcc_transaction_type_with_only_excluded_units_is_refused() {
        let mut tpcc = tpcc_units(&[]);
        tpcc.retain(|unit| unit.unit().application.as_str() != "delivery");
        tpcc.push(excluded("delivery"));
        let refusal = evaluate(&manifest(&["a/b"]), &tpcc, &[gating("a/b")]).unwrap_err();
        assert_eq!(
            refusal,
            Refusal::TpccTransactionWithoutUnits(TpccTransaction::Delivery)
        );
    }
}

fn id(name: &str) -> kiln_e1::ApplicationId {
    kiln_e1::ApplicationId::new(name)
}
