mod support;

use kiln_e1::{median, Manifest, Percent, ProvenCategory, Tally, TpccTransaction};
use support::*;

fn tally(units: &[kiln_e1::ClassifiedUnit]) -> Tally {
    let mut tally = Tally::default();
    units.iter().for_each(|unit| tally.count(unit));
    tally
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
        assert_eq!(tally.gating_share(), Some(Percent(50.0)));
        assert_eq!(tally.domain_bounded_share(), Some(Percent(25.0)));
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
        assert_eq!(tally(&units).gating_share(), Some(Percent(75.0)));
    }
}

mod kiln_e1_09 {
    use super::*;

    #[test]
    fn an_s4_exclusion_leaves_the_denominator_and_is_counted() {
        let units = [gating("a"), unknown("a"), excluded("a"), excluded("a")];
        let tally = tally(&units);
        assert_eq!(tally.gating_share(), Some(Percent(50.0)));
        assert_eq!(tally.denominator(), 2);
        assert_eq!(tally.excluded(), 2);
    }
}

mod kiln_e1_10 {
    use super::*;

    fn percents(values: &[f64]) -> Vec<Percent> {
        values.iter().map(|value| Percent(*value)).collect()
    }

    #[test]
    fn the_median_is_the_middle_or_the_mean_of_the_two_middle_shares() {
        let cases: [(&[f64], Option<f64>); 5] = [
            (&[], None),
            (&[40.0], Some(40.0)),
            (&[90.0, 10.0, 60.0], Some(60.0)),
            (&[100.0, 0.0, 80.0, 60.0], Some(70.0)),
            (&[50.0, 50.0], Some(50.0)),
        ];
        for (shares, expected) in cases {
            assert_eq!(
                median(&percents(shares)),
                expected.map(Percent),
                "{shares:?}"
            );
        }
    }
}

mod kiln_e1_11 {
    use super::*;
    use kiln_e1::{evaluate, Evaluation};

    fn weighted(unknown: &[TpccTransaction]) -> Evaluation {
        let manifest = manifest(&["a/b"]);
        evaluate(&manifest, &tpcc_units(unknown), &[gating("a/b")]).unwrap()
    }

    #[test]
    fn each_transaction_type_weighs_its_frozen_mix_share() {
        let cases = [
            (TpccTransaction::NewOrder, 55.0),
            (TpccTransaction::Payment, 57.0),
            (TpccTransaction::OrderStatus, 96.0),
            (TpccTransaction::Delivery, 96.0),
            (TpccTransaction::StockLevel, 96.0),
        ];
        for (unknown_type, expected) in cases {
            let share = weighted(&[unknown_type]).tpcc_weighted_share;
            assert!((share.0 - expected).abs() < 1e-9, "{unknown_type}: {share}");
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
        let share = weighted(&others).tpcc_weighted_share;
        assert!((share.0 - 43.0).abs() < 1e-9, "{share}");
    }

    #[test]
    fn a_transaction_type_weighs_its_own_unit_share() {
        let manifest = manifest(&["a/b"]);
        let mut units = tpcc_units(&[]);
        units.push(unknown("new-order"));
        let evaluation = evaluate(&manifest, &units, &[gating("a/b")]).unwrap();
        assert!((evaluation.tpcc_weighted_share.0 - 77.5).abs() < 1e-9);
    }
}
