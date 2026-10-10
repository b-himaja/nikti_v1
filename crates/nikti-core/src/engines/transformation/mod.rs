pub mod column_ops;
#[cfg(test)]
mod column_ops_tests;
pub mod errors;
pub mod formula;
#[cfg(test)]
mod formula_tests;
pub mod python_ops;
pub mod record_ops;
#[cfg(test)]
mod record_ops_tests;
pub mod table_ops;
#[cfg(test)]
mod table_ops_tests;

pub use errors::TransformError;
pub use crate::types::{ScalarValue, TransformationOperation};

use polars::prelude::*;

use super::content::DatasetContent;
use crate::types::table::{ColumnDef, TableSchema};

/// A deterministic outcome of applying one Transformation operation to one
/// or more Datasets. Every transformation is recorded in the Project
/// Operation Log (INV-013) by the command layer, not by this engine —
/// `TransformationResults` only describes the resulting content; whether
/// that becomes a `Dataset::apply_change` (in place) or a brand-new
/// Dataset is a command-layer decision.
#[derive(Debug)]
pub struct TransformationResults {
    pub frame: DataFrame,
    pub affected_columns: Vec<String>,
    /// Only ever populated by `FilterDataset { produce_complement: true }`.
    pub complement: Option<Box<TransformationResults>>,
}

impl TransformationResults {
    pub fn row_count(&self) -> u64 {
        self.frame.height() as u64
    }

    pub fn schema(&self) -> TableSchema {
        TableSchema {
            columns: self
                .frame
                .schema()
                .iter()
                .map(|(name, dtype)| ColumnDef {
                    name: name.to_string(),
                    data_type: format!("{dtype:?}"),
                })
                .collect(),
        }
    }
}

/// Collects a built-up lazy pipeline and wraps it as the operation's
/// result. Every op ends here — this is the one, explicit point (per op)
/// where "stay lazy" necessarily meets "the command layer needs a
/// concrete row count / schema to update Dataset metadata."
pub(super) fn finish(lf: LazyFrame, affected_columns: Vec<String>) -> Result<TransformationResults, TransformError> {
    Ok(TransformationResults {
        frame: lf.collect()?,
        affected_columns,
        complement: None,
    })
}

/// INV-010/013: Dataset transformations/Recipe execution only. The content
/// parameter is `DatasetContent`, not a Table handle — accepting a Table as
/// a transformation target must not typecheck, let alone run (INV-002,
/// Part 5 item 4).
pub trait TransformationEngine {
    fn transform(
        &self,
        content: DatasetContent,
        operation: TransformationOperation,
    ) -> Result<TransformationResults, TransformError> {
        let _ = (content, operation);
        todo!()
    }

    /// For operations needing two or more Dataset inputs (join, append/
    /// union, intersect, subtract, compare).
    fn transform_many(
        &self,
        contents: Vec<DatasetContent>,
        operation: TransformationOperation,
    ) -> Result<TransformationResults, TransformError> {
        let _ = (contents, operation);
        todo!()
    }
}

/// The Transformation operation catalogue's current version — same
/// convention as Validation's `function_catalogue_version`/Analysis's
/// `analysis_procedure_catalogue_version` (both plain hardcoded version
/// strings today, there having only ever been one version of either
/// catalogue). Used by the Audit Report task's `AuditContent::
/// transformation_catalogue_version` field.
pub const TRANSFORMATION_CATALOGUE_VERSION: &str = "1.0.0";

/// The native Rust/Polars implementation — the default backend for every
/// Transformation operation (INV-029: 100% native remains the default;
/// the exception list for this engine is currently empty, see
/// `docs/adr/0006-transformation-python-exception-list-empty.md`).
pub struct NativeTransformationEngine;

impl TransformationEngine for NativeTransformationEngine {
    fn transform(
        &self,
        content: DatasetContent,
        operation: TransformationOperation,
    ) -> Result<TransformationResults, TransformError> {
        use TransformationOperation as Op;

        let lf = content.frame;
        match operation {
            Op::DeriveColumn { output_col, formula, overwrite } => {
                column_ops::derive_column(lf, &output_col, &formula, overwrite)
            }
            Op::RecodeColumn { column, mapping, default, output_col } => {
                column_ops::recode_column(lf, &column, &mapping, default.as_deref(), output_col.as_deref())
            }
            Op::DiscretiseColumn { column, output_col, method, labels } => {
                column_ops::discretise_column(lf, &column, output_col.as_deref(), &method, labels)
            }
            Op::LeadLag { column, output_col, periods, group_by, fill_value } => {
                column_ops::lead_lag(lf, &column, output_col.as_deref(), periods, group_by.as_deref(), fill_value)
            }
            Op::FillMissing { columns, strategy } => column_ops::fill_missing(lf, &columns, &strategy),
            Op::ApplyScaler { columns, method, output_suffix } => {
                column_ops::apply_scaler(lf, &columns, method, output_suffix.as_deref())
            }
            Op::RenameColumns { mapping } => column_ops::rename_columns(lf, &mapping),
            Op::ReorderColumns { column_order } => column_ops::reorder_columns(lf, &column_order),
            Op::StringOp { column, output_col, op } => {
                column_ops::string_op(lf, &column, output_col.as_deref(), &op)
            }
            Op::DatetimeOp { column, output_col, op } => {
                column_ops::datetime_op(lf, &column, output_col.as_deref(), &op)
            }
            Op::Differencing { column, output_col, periods, pct_change } => {
                column_ops::differencing(lf, &column, output_col.as_deref(), periods, pct_change)
            }
            Op::ConditionalFlag { output_col, conditions, default } => {
                column_ops::conditional_flag(lf, &output_col, &conditions, default)
            }
            Op::FlagValues { column, output_col, method } => {
                column_ops::flag_values(lf, &column, output_col.as_deref(), &method)
            }
            Op::ChangeDtype { column, target_type, format } => {
                column_ops::change_dtype(lf, &column, target_type, format.as_deref())
            }
            Op::DropColumns { columns } => column_ops::drop_columns(lf, &columns),
            Op::CopyColumn { column, output_col } => column_ops::copy_column(lf, &column, &output_col),
            Op::CoalesceColumns { columns, output_col } => {
                column_ops::coalesce_columns(lf, &columns, &output_col)
            }
            Op::GenerateRowNumber { output_col, start } => {
                column_ops::generate_row_number(lf, output_col.as_deref(), start)
            }
            Op::GenerateUuid { output_col, seed } => column_ops::generate_uuid(lf, output_col.as_deref(), seed),
            Op::NullStandardise { columns, null_variants } => {
                column_ops::null_standardise(lf, &columns, null_variants)
            }
            Op::TextStandardise { column, output_col, mapping, case_sensitive } => {
                column_ops::text_standardise(lf, &column, output_col.as_deref(), &mapping, case_sensitive)
            }
            Op::HashColumn { column, output_col, algorithm } => {
                column_ops::hash_column(lf, &column, output_col.as_deref(), algorithm)
            }

            Op::UnpivotDataset { id_vars, value_vars, variable_name, value_name } => {
                table_ops::unpivot_dataset(lf, &id_vars, value_vars, &variable_name, &value_name)
            }
            Op::AggregateDataset { group_by, aggregations } => {
                table_ops::aggregate_dataset(lf, &group_by, &aggregations)
            }
            Op::MovingCalc { column, output_col, func, window, min_periods } => {
                table_ops::moving_calc(lf, &column, output_col.as_deref(), func, window, min_periods)
            }
            Op::RunningTotal { column, output_col, group_by } => {
                table_ops::running_total(lf, &column, output_col.as_deref(), group_by.as_deref())
            }

            Op::FilterDataset { condition, produce_complement } => {
                record_ops::filter_dataset(lf, &condition, produce_complement)
            }
            Op::SampleDataset { method, n, fraction, seed, group_by } => {
                record_ops::sample_dataset(lf, method, n, fraction, seed, group_by.as_deref())
            }
            Op::SortDataset { sort_by, nulls_last } => record_ops::sort_dataset(lf, &sort_by, nulls_last),
            Op::SliceRows { start, end } => record_ops::slice_rows(lf, start, end),
            Op::DeleteRows { indices } => record_ops::delete_rows(lf, &indices),
            Op::TagRows { tag_col, indices, tag } => {
                record_ops::tag_rows(lf, tag_col.as_deref(), &indices, &tag)
            }

            other @ (Op::JoinDatasets { .. }
            | Op::AppendDatasets { .. }
            | Op::IntersectDatasets
            | Op::SubtractDatasets
            | Op::CompareDatasets { .. }) => Err(TransformError::InvalidParams(format!(
                "{other:?} requires two or more Datasets — call transform_many, not transform"
            ))),
        }
    }

    fn transform_many(
        &self,
        contents: Vec<DatasetContent>,
        operation: TransformationOperation,
    ) -> Result<TransformationResults, TransformError> {
        use TransformationOperation as Op;

        let frames: Vec<LazyFrame> = contents.into_iter().map(|c| c.frame).collect();
        match operation {
            Op::JoinDatasets { keys, how, suffix } => table_ops::join_datasets(frames, &keys, how, suffix.as_deref()),
            Op::AppendDatasets { match_by, deduplicate, add_source_col } => {
                table_ops::append_datasets(frames, match_by, deduplicate, add_source_col)
            }
            Op::IntersectDatasets => table_ops::intersect_datasets(frames),
            Op::SubtractDatasets => table_ops::subtract_datasets(frames),
            Op::CompareDatasets { key_columns } => table_ops::compare_datasets(frames, &key_columns),
            other => Err(TransformError::InvalidParams(format!(
                "{other:?} is a single-Dataset operation — call transform, not transform_many"
            ))),
        }
    }
}
