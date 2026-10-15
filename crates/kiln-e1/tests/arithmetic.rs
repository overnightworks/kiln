use std::num::NonZeroU128;

use kiln_e1::{
    evaluate, median, ClassifiedUnit, Manifest, Overflow, ProvenCategory, Ratio, Tally,
    TpccTransaction,
};

#[path = "support/units.rs"]
mod units;
use units::*;

fn tally(units: &[ClassifiedUnit]) -> Tally {
    let mut tally = Tally::default();
    units.iter().for_each(|unit| tally.count(unit));
    tally
}

/// The exact fraction `numerator / denominator`.
fn ratio(numerator: u128, denominator: u128) -> Ratio {
    Ratio::new(numerator, NonZeroU128::new(denominator).unwrap())
}

mod kiln_e1_06 {
    use super::*;

    #[test]
    fn the_frozen_manifest_holds_110_applications() {
        let text =
            include_str!("../../../experiments/e1-classification-coverage/corpus-manifest.csv");
        let manifest = Manifest::parse(text).unwrap();
        assert_eq!(manifest.applications().len(), 110);
        assert!(manifest
            .applications()
            .iter()
            .any(|id| id.as_str() == "allegro/ralph"));
    }

    #[test]
    fn malformed_manifests_are_rejected_naming_the_line() {
        let cases = [
            ("orm,owner_repo\nDjango,a/b\nDjango\n", "line 3"),
            ("orm,owner_repo\nDjango,a/b\nDjango,\"c,d\"\n", "line 3"),
            ("orm,owner_repo\nDjango,a/b\nDjango,a/b\n", "line 3"),
            ("orm,owner_repo\nDjango,\n", "line 2"),
            ("orm,repository\nDjango,a/b\n", "line 1"),
            ("", "line 1"),
        ];
        for (text, expected) in cases {
            let error = Manifest::parse(text).unwrap_err().to_string();
            assert!(error.contains(expected), "{text:?}: {error}");
        }
    }

    #[test]
    fn the_application_column_is_found_by_name() {
        let manifest = Manifest::parse("owner_repo,orm\na/b,Django\n").unwrap();
        assert_eq!(
            manifest.applications().iter().next().unwrap().as_str(),
            "a/b"
        );
    }
}

mod kiln_e1_07 {
    use super::*;

    #[test]
    fn only_static_and_one_hop_units_gate() {
        let units = [
            gating("a"),
            proven("a", ProvenCategory::OneHop),
            proven("a", ProvenCategory::DomainBounded),
            unknown("a"),
        ];
        let tally = tally(&units);
        assert_eq!(tally.gating_share(), Some(ratio(1, 2)));
        assert_eq!(tally.domain_bounded_share(), Some(ratio(1, 4)));
        assert_eq!(tally.denominator(), 4);
    }

    #[test]
    fn an_empty_denominator_has_no_share() {
        assert_eq!(Tally::default().gating_share(), None);
        assert_eq!(Tally::default().domain_bounded_share(), None);
    }
}

mod kiln_e1_08 {
    use super::*;

    #[test]
    fn a_refused_unit_stays_in_the_denominator() {
        let units = [gating("a"), gating("a"), gating("a"), unknown("a")];
        assert_eq!(tally(&units).gating_share(), Some(ratio(3, 4)));
    }
}

mod kiln_e1_09 {
    use super::*;

    #[test]
    fn an_s4_exclusion_leaves_the_denominator_and_is_counted() {
        let units = [gating("a"), unknown("a"), excluded("a"), excluded("a")];
        let tally = tally(&units);
        assert_eq!(tally.gating_share(), Some(ratio(1, 2)));
        assert_eq!(tally.denominator(), 2);
        assert_eq!(tally.excluded(), 2);
    }
}

mod kiln_e1_10 {
    use super::*;

    #[test]
    fn the_median_is_the_middle_or_the_mean_of_the_two_middle_shares() {
        let cases: [(Vec<Ratio>, Option<Ratio>); 5] = [
            (vec![], None),
            (vec![ratio(2, 5)], Some(ratio(2, 5))),
            (
                vec![ratio(9, 10), ratio(1, 10), ratio(3, 5)],
                Some(ratio(3, 5)),
            ),
            (
                vec![ratio(1, 1), ratio(0, 1), ratio(4, 5), ratio(3, 5)],
                Some(ratio(7, 10)),
            ),
            (vec![ratio(1, 2), ratio(1, 2)], Some(ratio(1, 2))),
        ];
        for (shares, expected) in cases {
            assert_eq!(median(&shares), Ok(expected), "{shares:?}");
        }
    }

    #[test]
    fn an_even_count_landing_exactly_on_a_threshold_keeps_it() {
        let cases = [
            (ratio(2, 3), ratio(11, 15), ratio(7, 10)),
            (ratio(1, 3), ratio(2, 3), ratio(1, 2)),
        ];
        for (low, high, expected) in cases {
            assert_eq!(median(&[high, low]), Ok(Some(expected)));
        }
    }

    #[test]
    fn the_evaluation_takes_the_median_over_the_manifest_applications() {
        let manifest = manifest(&["a/b", "c/d"]);
        let mut applications = units_with_share("a/b", 2, 3);
        applications.extend(units_with_share("c/d", 11, 15));
        let evaluation = evaluate(&manifest, &tpcc_units(&[]), &applications).unwrap();
        assert_eq!(evaluation.application_median, ratio(7, 10));
    }
}

mod kiln_e1_11 {
    use super::*;

    fn weighted_share(tpcc: &[ClassifiedUnit]) -> Ratio {
        let manifest = manifest(&["a/b"]);
        evaluate(&manifest, tpcc, &[gating("a/b")])
            .unwrap()
            .tpcc_weighted_share
    }

    #[test]
    fn each_transaction_type_weighs_its_frozen_mix_share() {
        let cases = [
            (TpccTransaction::NewOrder, ratio(55, 100)),
            (TpccTransaction::Payment, ratio(57, 100)),
            (TpccTransaction::OrderStatus, ratio(96, 100)),
            (TpccTransaction::Delivery, ratio(96, 100)),
            (TpccTransaction::StockLevel, ratio(96, 100)),
        ];
        for (unknown_type, expected) in cases {
            let share = weighted_share(&tpcc_units(&[unknown_type]));
            assert_eq!(share, expected, "{unknown_type}");
        }
    }

    #[test]
    fn only_payment_static_gives_its_weight() {
        let others = [
            TpccTransaction::NewOrder,
            TpccTransaction::OrderStatus,
            TpccTransaction::Delivery,
            TpccTransaction::StockLevel,
        ];
        assert_eq!(weighted_share(&tpcc_units(&others)), ratio(43, 100));
    }

    #[test]
    fn a_transaction_type_weighs_its_own_unit_share() {
        let mut units = tpcc_units(&[]);
        units.push(unknown(TpccTransaction::NewOrder));
        assert_eq!(weighted_share(&units), ratio(775, 1000));
    }

    #[test]
    fn unequal_unit_counts_per_transaction_weigh_exactly() {
        // 45 * 1 + 43 * 7/15 + 4 * 14/15 + 4 * 0 + 4 * 3/10 = 70, to the last digit.
        let units = tpcc_units_with_shares([(1, 1), (7, 15), (14, 15), (0, 1), (3, 10)]);
        assert_eq!(weighted_share(&units), ratio(70, 100));
    }
}

mod kiln_e1_17 {
    use super::*;

    #[test]
    fn a_share_that_does_not_fit_exactly_is_refused_not_rounded() {
        let large = ratio(1, u128::MAX - 1);
        let other = ratio(1, u128::MAX - 2);
        assert_eq!(large.checked_add(other), Err(Overflow));
        assert_eq!(large.checked_mul(other), Err(Overflow));
        assert_eq!(median(&[large, other]), Err(Overflow));
    }

    #[test]
    fn shares_compare_exactly_even_where_cross_multiplying_would_overflow() {
        let smaller = ratio(u128::MAX - 2, u128::MAX - 1);
        let larger = ratio(u128::MAX - 1, u128::MAX);
        assert!(smaller < larger);
        assert!(larger < ratio(1, 1));
    }

    #[test]
    fn a_share_displays_as_percent() {
        assert_eq!(ratio(3, 4).to_string(), "75.0000 %");
        assert_eq!(ratio(1, 3).to_string(), "33.3333 %");
    }
}
