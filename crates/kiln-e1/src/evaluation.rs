use std::collections::BTreeMap;
use std::fmt;

use crate::classified::ClassifiedUnit;
use crate::coverage::{median, Percent, Tally};
use crate::manifest::Manifest;
use crate::tpcc::TpccTransaction;
use crate::unit::ApplicationId;
use crate::verdict::Verdict;

/// Why no verdict can be computed. The freeze runs the median over the frozen
/// set, so a gap is an extractor defect or a protocol amendment, not a number.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    EmptyManifest,
    ApplicationOutsideManifest(ApplicationId),
    ApplicationWithoutUnits(ApplicationId),
    UnknownTpccTransaction(ApplicationId),
    TpccTransactionWithoutUnits(TpccTransaction),
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyManifest => formatter.write_str("the manifest lists no application"),
            Self::ApplicationOutsideManifest(id) => {
                write!(
                    formatter,
                    "classified units name `{id}`, which is not in the manifest"
                )
            }
            Self::ApplicationWithoutUnits(id) => {
                write!(
                    formatter,
                    "manifest application `{id}` has no unit in its denominator"
                )
            }
            Self::UnknownTpccTransaction(id) => {
                write!(
                    formatter,
                    "TPC-C units name `{id}`, which is no TPC-C transaction type"
                )
            }
            Self::TpccTransactionWithoutUnits(transaction) => {
                write!(
                    formatter,
                    "TPC-C transaction `{transaction}` has no unit in its denominator"
                )
            }
        }
    }
}

impl std::error::Error for Refusal {}

#[derive(Debug)]
pub struct Evaluation {
    pub applications: Vec<(ApplicationId, Tally)>,
    pub tpcc_transactions: Vec<(TpccTransaction, Tally)>,
    pub tpcc_weighted_share: Percent,
    pub application_median: Percent,
    pub verdict: Verdict,
}

pub fn evaluate(
    manifest: &Manifest,
    tpcc_units: &[ClassifiedUnit],
    application_units: &[ClassifiedUnit],
) -> Result<Evaluation, Refusal> {
    let (tpcc_transactions, tpcc_weighted_share) = tpcc_weighted(tpcc_units)?;
    let (applications, application_median) = application_median(manifest, application_units)?;
    Ok(Evaluation {
        applications,
        tpcc_transactions,
        tpcc_weighted_share,
        application_median,
        verdict: Verdict::from_shares(tpcc_weighted_share, application_median),
    })
}

fn tpcc_weighted(
    units: &[ClassifiedUnit],
) -> Result<(Vec<(TpccTransaction, Tally)>, Percent), Refusal> {
    let mut tallies: BTreeMap<TpccTransaction, Tally> = BTreeMap::new();
    for unit in units {
        let name = &unit.unit().application;
        let transaction = TpccTransaction::from_name(name.as_str())
            .ok_or_else(|| Refusal::UnknownTpccTransaction(name.clone()))?;
        tallies.entry(transaction).or_default().count(unit);
    }
    let mut weighted_sum = 0.0;
    let mut per_transaction = Vec::new();
    for transaction in TpccTransaction::ALL {
        let tally = tallies.remove(&transaction).unwrap_or_default();
        let share = tally
            .gating_share()
            .ok_or(Refusal::TpccTransactionWithoutUnits(transaction))?;
        weighted_sum += transaction.weight_percent() * share.0;
        per_transaction.push((transaction, tally));
    }
    Ok((per_transaction, Percent(weighted_sum / 100.0)))
}

fn application_median(
    manifest: &Manifest,
    units: &[ClassifiedUnit],
) -> Result<(Vec<(ApplicationId, Tally)>, Percent), Refusal> {
    let mut tallies: BTreeMap<ApplicationId, Tally> = BTreeMap::new();
    for unit in units {
        let id = &unit.unit().application;
        if !manifest.applications().contains(id) {
            return Err(Refusal::ApplicationOutsideManifest(id.clone()));
        }
        tallies.entry(id.clone()).or_default().count(unit);
    }
    let mut shares = Vec::new();
    let mut per_application = Vec::new();
    for id in manifest.applications() {
        let tally = tallies.remove(id).unwrap_or_default();
        let share = tally
            .gating_share()
            .ok_or_else(|| Refusal::ApplicationWithoutUnits(id.clone()))?;
        shares.push(share);
        per_application.push((id.clone(), tally));
    }
    let middle = median(&shares).ok_or(Refusal::EmptyManifest)?;
    Ok((per_application, middle))
}
