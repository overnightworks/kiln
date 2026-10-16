//! The cells the pilot reports (protocol.md §5, §8, §11): every workload
//! at both sizes under both `synchronous` settings, and which of them the
//! 2 % overhead bound gates. The types are [`crate::workloads`]'s; this
//! module adds the spellings the pilot's child processes, reports and
//! logs share, so one place owns "point-update/medium/normal".

use crate::workloads::{Cell, Size, Synchronous, Workload};

impl Workload {
    pub const ALL: [Workload; 4] = [
        Workload::PointRead,
        Workload::PointUpdate,
        Workload::Insert,
        Workload::Transfer,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Workload::PointRead => "point-read",
            Workload::PointUpdate => "point-update",
            Workload::Insert => "insert",
            Workload::Transfer => "transfer",
        }
    }

    pub fn parse(value: &str) -> Option<Workload> {
        Workload::ALL
            .into_iter()
            .find(|workload| workload.label() == value)
    }

    /// Whether protocol §5's overhead bound gates this workload: only a
    /// timed transaction that calls `xWrite` or `xSync` is gated, and
    /// workload A's read-only transaction never does.
    pub fn is_gated(self) -> bool {
        self != Workload::PointRead
    }
}

impl Size {
    pub const ALL: [Size; 2] = [Size::Small, Size::Medium];

    pub fn label(self) -> &'static str {
        match self {
            Size::Small => "small",
            Size::Medium => "medium",
        }
    }
}

impl Synchronous {
    pub const ALL: [Synchronous; 2] = [Synchronous::Full, Synchronous::Normal];

    pub fn label(self) -> &'static str {
        match self {
            Synchronous::Full => "full",
            Synchronous::Normal => "normal",
        }
    }

    pub fn parse(value: &str) -> Option<Synchronous> {
        Synchronous::ALL
            .into_iter()
            .find(|synchronous| synchronous.label() == value)
    }
}

impl Cell {
    /// `workload/size/synchronous`, e.g. `insert/medium/normal`.
    pub fn label(&self) -> String {
        format!(
            "{}/{}/{}",
            self.workload.label(),
            self.size.label(),
            self.synchronous.label()
        )
    }
}

/// Every cell the pilot reports, in [`Cell::index`] order: 4 workloads ×
/// 2 sizes × 2 settings.
pub fn all_cells() -> Vec<Cell> {
    let mut cells = Vec::new();
    for workload in Workload::ALL {
        for size in Size::ALL {
            for synchronous in Synchronous::ALL {
                cells.push(Cell {
                    workload,
                    size,
                    synchronous,
                });
            }
        }
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    mod kiln_e4_11 {
        use super::*;

        #[test]
        fn the_pilot_has_sixteen_cells_in_index_order() {
            let cells = all_cells();
            assert_eq!(cells.len(), 16);
            let indices: Vec<u64> = cells.iter().map(Cell::index).collect();
            assert_eq!(indices, (0..16).collect::<Vec<u64>>());
        }

        #[test]
        fn exactly_the_write_workloads_at_both_sizes_and_settings_are_gated() {
            let gated: Vec<String> = all_cells()
                .iter()
                .filter(|cell| cell.workload.is_gated())
                .map(Cell::label)
                .collect();
            assert_eq!(gated.len(), 12);
            for workload in ["point-update", "insert", "transfer"] {
                for size in ["small", "medium"] {
                    for synchronous in ["full", "normal"] {
                        let expected = format!("{workload}/{size}/{synchronous}");
                        assert!(gated.contains(&expected), "{expected} must be gated");
                    }
                }
            }
        }

        #[test]
        fn no_point_read_cell_is_gated() {
            assert!(
                all_cells()
                    .iter()
                    .filter(|cell| cell.workload == Workload::PointRead)
                    .all(|cell| !cell.workload.is_gated())
            );
        }

        #[test]
        fn every_cell_label_is_distinct() {
            let mut labels: Vec<String> = all_cells().iter().map(Cell::label).collect();
            labels.sort();
            labels.dedup();
            assert_eq!(labels.len(), 16);
        }
    }

    mod kiln_e4_18 {
        use super::*;

        #[test]
        fn a_workload_is_selected_by_its_documented_spelling() {
            for (spelling, workload) in [
                ("point-read", Workload::PointRead),
                ("point-update", Workload::PointUpdate),
                ("insert", Workload::Insert),
                ("transfer", Workload::Transfer),
            ] {
                assert_eq!(Workload::parse(spelling), Some(workload));
                assert_eq!(workload.label(), spelling);
            }
            assert_eq!(Workload::parse("range-scan"), None);
        }

        #[test]
        fn a_synchronous_setting_is_selected_by_its_documented_spelling() {
            assert_eq!(Synchronous::parse("full"), Some(Synchronous::Full));
            assert_eq!(Synchronous::parse("normal"), Some(Synchronous::Normal));
            assert_eq!(Synchronous::parse("FULL"), None);
            assert_eq!(Synchronous::parse("off"), None);
        }

        #[test]
        fn a_cell_label_names_its_workload_size_and_setting() {
            let cell = Cell {
                workload: Workload::Insert,
                size: Size::Medium,
                synchronous: Synchronous::Normal,
            };
            assert_eq!(cell.label(), "insert/medium/normal");
        }
    }
}
