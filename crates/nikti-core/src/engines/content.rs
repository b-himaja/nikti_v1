use polars::prelude::LazyFrame;

use crate::types::common::{DatasetId, TableId};

/// The Arrow/Polars content handle Inspection/Validation/Analysis read.
///
/// `content_fingerprint` is the Table's real, already-computed
/// `data_fingerprint` (see `types::table::TableDataRef`) — required, not
/// `Option`, deliberately: there is no code path in this codebase where a
/// `Table` exists without one, so a caller that can't supply the real
/// value has a precondition failure to fix, not a placeholder to fall
/// back on (see `engines::inspection::inspect_standard`, which used to
/// fabricate `format!("unfingerprinted:{table_id}")` here before this
/// field existed).
#[derive(Clone)]
pub struct TableContent {
    pub table_id: TableId,
    pub content_fingerprint: String,
    pub frame: LazyFrame,
}

/// INV-013: the Transformation Engine's content parameter is a Dataset
/// handle, never a Table handle — there is deliberately no `From<TableId>`
/// or similar conversion into this type, and none may ever be added.
///
/// Carries a real `LazyFrame` (not just an id) so Transformation
/// operations are genuinely executable and testable now, per the
/// legacy-code port — real repository-backed loading of that frame from
/// content-addressed Parquet is still a later task.
#[derive(Clone)]
pub struct DatasetContent {
    pub dataset_id: DatasetId,
    pub frame: LazyFrame,
}
