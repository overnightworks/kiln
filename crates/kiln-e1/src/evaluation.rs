use std::collections::BTreeMap;
use std::fmt;

use crate::classified::ClassifiedUnit;
use crate::coverage::{median, Tally};
use crate::manifest::Manifest;
use crate::ratio::{Overflow, Ratio};
use crate::tpcc::TpccTransaction;
use crate::unit::{ApplicationId, Subject};
use crate::verdict::Verdict;

/// Why no verdict can be computed. The freeze runs the median over the frozen
/// set, so a gap is an extractor defect or a protocol amendment, not a number.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    EmptyManifest,
    ApplicationOutsideManifest(ApplicationId),
    ApplicationWithoutUnits(ApplicationId),
    TpccUnitNamesApplication(ApplicationId),
    ApplicationUnitNamesTpccTransaction(TpccTransaction),
    TpccTransactionWithoutUnits(TpccTransaction),
    Inexact(Overflow),
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
            Self::TpccUnitNamesApplication(id) => {
                write!(
                    formatter,
                    "a TPC-C unit names application `{id}` instead of a TPC-C transaction type"
                )
            }
            Self::ApplicationUnitNamesTpccTransaction(transaction) => {
                write!(
                    formatter,
                    "an application unit names TPC-C transaction type `{transaction}` instead of a manifest application"
                )
            }
            Self::TpccTransactionWithoutUnits(transaction) => {
                write!(
                    formatter,
                    "TPC-C transaction `{transaction}` has no unit in its denominator"
                )
            }
            Self::Inexact(overflow) => overflow.fmt(formatter),
        }
    }
}

impl std::error::Error for Refusal {}

#[derive(Debug)]
pub struct Evaluation {
    pub applications: Vec<(ApplicationId, Tally)>,
    pub tpcc_transactions: Vec<(TpccTransaction, Tally)>,
    pub tpcc_weighted_share: Ratio,
    pub application_median: Ratio,
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
) -> Result<(Vec<(TpccTransaction, Tally)>, Ratio), Refusal> {
    let mut tallies: BTreeMap<TpccTransaction, Tally> = BTreeMap::new();
    for unit in units {
        let transaction = match &unit.unit().subject {
            Subject::Tpcc(transaction) => *transaction,
            Subject::Application(id) => return Err(Refusal::TpccUnitNamesApplication(id.clone())),
        };
        tallies.entry(transaction).or_default().count(unit);
    }
    let mut weighted_share = Ratio::from_percent(0);
    let mut per_transaction = Vec::new();
    for transaction in TpccTransaction::ALL {
        let tally = tallies.remove(&transaction).unwrap_or_default();
        let share = tally
            .gating_share()
            .ok_or(Refusal::TpccTransactionWithoutUnits(transaction))?;
        let weighted = share
            .checked_mul(Ratio::from_percent(transaction.weight_percent()))
            .map_err(Refusal::Inexact)?;
        weighted_share = weighted_share
            .checked_add(weighted)
            .map_err(Refusal::Inexact)?;
        per_transaction.push((transaction, tally));
    }
    Ok((per_transaction, weighted_share))
}

fn application_median(
    manifest: &Manifest,
    units: &[ClassifiedUnit],
) -> Result<(Vec<(ApplicationId, Tally)>, Ratio), Refusal> {
    let mut tallies: BTreeMap<ApplicationId, Tally> = BTreeMap::new();
    for unit in units {
        let id = match &unit.unit().subject {
            Subject::Application(id) => id,
            Subject::Tpcc(transaction) => {
                return Err(Refusal::ApplicationUnitNamesTpccTransaction(*transaction))
            }
        };
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
    let middle = median(&shares)
        .map_err(Refusal::Inexact)?
        .ok_or(Refusal::EmptyManifest)?;
    Ok((per_application, middle))
}
