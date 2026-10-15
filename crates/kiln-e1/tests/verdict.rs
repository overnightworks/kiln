use kiln_e1::{evaluate, ApplicationId, Evaluation, Refusal, TpccTransaction, Verdict};

#[path = "support/units.rs"]
mod units;
use units::*;

use Verdict::{Fail as F, Inconclusive as I, Pass as P};

/// Evaluates a TPC-C share of `tpcc` and a single application's share of
/// `application`, each as `(static units, units)`.
fn verdict_of(tpcc: (usize, usize), application: (usize, usize)) -> Verdict {
    evaluation_of(tpcc_units_with_shares([tpcc; 5]), application).verdict
}

fn evaluation_of(tpcc: Vec<kiln_e1::ClassifiedUnit>, application: (usize, usize)) -> Evaluation {
    let applications = units_with_share("a/b", application.0, application.1);
    evaluate(&manifest(&["a/b"]), &tpcc, &applications).unwrap()
}

mod kiln_e1_12 {
    use super::*;

    /// 49.99 %, 50 %, 69.99 %, 70 % and 100 %, as unit counts.
    const SHARES: [(usize, usize); 5] = [(4999, 10000), (1, 2), (6999, 10000), (7, 10), (1, 1)];

    /// Written from the ruled sentence: pass at least 70 % on both shares, fail
    /// below 50 % on either, otherwise inconclusive. Rows are the TPC-C share,
    /// columns the median, both in the order of `SHARES`.
    const EXPECTED: [[Verdict; 5]; 5] = [
        [F, F, F, F, F],
        [F, I, I, I, I],
        [F, I, I, I, I],
        [F, I, I, P, P],
        [F, I, I, P, P],
    ];

    #[test]
    fn thresholds_decide_the_verdict_for_both_shares() {
        for (tpcc, row) in SHARES.into_iter().zip(EXPECTED) {
            for (median, expected) in SHARES.into_iter().zip(row) {
                assert_eq!(
                    verdict_of(tpcc, median),
                    expected,
                    "tpcc {tpcc:?}, median {median:?}"
                );
            }
        }
    }

    #[test]
    fn a_fail_outranks_a_pass_on_the_other_share() {
        assert_eq!(verdict_of((1, 1), (4999, 10000)), F);
        assert_eq!(verdict_of((4999, 10000), (1, 1)), F);
    }

    #[test]
    fn a_tpcc_share_that_is_exactly_70_passes_whatever_its_transaction_shares() {
        let cases = [
            [(1, 1), (7, 15), (14, 15), (0, 1), (3, 10)],
            [(2, 3), (2, 3), (17, 18), (17, 18), (17, 18)],
        ];
        for shares in cases {
            let evaluation = evaluation_of(tpcc_units_with_shares(shares), (1, 1));
            assert_eq!(evaluation.verdict, P, "{shares:?}");
        }
    }

    #[test]
    fn a_tpcc_share_that_is_exactly_50_does_not_fail() {
        let shares = [(0, 1), (1, 1), (7, 12), (7, 12), (7, 12)];
        let evaluation = evaluation_of(tpcc_units_with_shares(shares), (1, 1));
        assert_eq!(evaluation.verdict, I);
    }

    #[test]
    fn a_median_of_an_even_count_that_is_exactly_70_or_50_is_not_below_it() {
        let cases = [
            (("a/b", 2, 3), ("c/d", 11, 15), P),
            (("a/b", 1, 3), ("c/d", 2, 3), I),
        ];
        for ((first, first_gating, first_total), (second, second_gating, second_total), expected) in
            cases
        {
            let mut applications = units_with_share(first, first_gating, first_total);
            applications.extend(units_with_share(second, second_gating, second_total));
            let evaluation =
                evaluate(&manifest(&[first, second]), &tpcc_units(&[]), &applications).unwrap();
            assert_eq!(evaluation.verdict, expected);
        }
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

    fn refusal_for(tpcc: Vec<kiln_e1::ClassifiedUnit>) -> Refusal {
        evaluate(&manifest(&["a/b"]), &tpcc, &[gating("a/b")]).unwrap_err()
    }

    #[test]
    fn a_tpcc_unit_naming_an_application_is_refused_and_named() {
        let mut tpcc = tpcc_units(&[]);
        tpcc.push(gating("refund"));
        let refusal = refusal_for(tpcc);
        assert_eq!(refusal, Refusal::TpccUnitNamesApplication(id("refund")));
        assert!(refusal.to_string().contains("refund"));
    }

    #[test]
    fn an_application_unit_naming_a_tpcc_transaction_type_is_refused_and_named() {
        let units = [gating("a/b"), gating(TpccTransaction::Payment)];
        let refusal = evaluate(&manifest(&["a/b"]), &tpcc_units(&[]), &units).unwrap_err();
        assert_eq!(
            refusal,
            Refusal::ApplicationUnitNamesTpccTransaction(TpccTransaction::Payment)
        );
        assert!(refusal.to_string().contains("payment"));
    }

    #[test]
    fn a_tpcc_transaction_type_without_units_is_refused() {
        for transaction in TpccTransaction::ALL {
            let tpcc = tpcc_units(&[])
                .into_iter()
                .filter(|unit| unit.unit().subject != transaction_subject(transaction))
                .collect();
            let refusal = refusal_for(tpcc);
            assert_eq!(refusal, Refusal::TpccTransactionWithoutUnits(transaction));
            assert!(refusal.to_string().contains(transaction.name()));
        }
    }

    #[test]
    fn a_tpcc_transaction_type_with_only_excluded_units_is_refused() {
        let mut tpcc = tpcc_units(&[]);
        tpcc.retain(|unit| unit.unit().subject != transaction_subject(TpccTransaction::Delivery));
        tpcc.push(excluded(TpccTransaction::Delivery));
        assert_eq!(
            refusal_for(tpcc),
            Refusal::TpccTransactionWithoutUnits(TpccTransaction::Delivery)
        );
    }
}

fn transaction_subject(transaction: TpccTransaction) -> kiln_e1::Subject {
    kiln_e1::Subject::Tpcc(transaction)
}

fn id(name: &str) -> ApplicationId {
    ApplicationId::new(name)
}
