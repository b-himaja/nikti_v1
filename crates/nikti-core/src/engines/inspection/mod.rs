//! Inspection Engine (Inspection Report Design Spec v1.0, as summarized in
//! the governing task prompt — the full source spec was unavailable in
//! this repository at implementation time; see the final compliance
//! report for that caveat).
//!
//! INV-011: fixed and non-configurable — `inspect()` takes no
//! `definition`/ruleset parameter, and none may ever be added, even an
//! empty placeholder "for future use" (INV-028). `ImportSummary` is not an
//! exception to this: it is factual evidence about the import event that
//! produced the Table (rows read/rejected, encoding), not configuration
//! of *how* Inspection behaves — the same category of input as `content`
//! itself.
//!
//! INV-010: inspection only — never accepts user-defined rules, never
//! calls another engine, never writes to the Table it inspects.

pub mod metric_dictionary;
pub mod reserved;
pub mod sections;
mod threshold_registry;

#[cfg(test)]
mod tests;

use polars::prelude::*;
use serde::Serialize;

use crate::types::common::{TableId, Timestamp};

use super::content::TableContent;
pub use sections::{EmittedMetric, ImportSummary, MetricSink};
pub use threshold_registry::ThresholdRegistry;

#[derive(Debug, thiserror::Error)]
pub enum InspectionError {
    #[error("column '{0}' not found")]
    ColumnNotFound(String),
    #[error("polars error: {0}")]
    Polars(#[from] PolarsError),
}

/// Amendment 4: sampling disclosure. `None` means every section in this
/// `InspectionResults` was computed on full data. `Some` means the row-
/// materializing sections listed in `affected_sections` were computed on
/// a deterministic sample, never silently presented as `Full Data`.
#[derive(Debug, Clone, Serialize)]
pub struct SamplingDisclosure {
    pub sample_size: usize,
    pub sampling_method: String,
    pub seed: Option<u64>,
    pub full_row_count: u64,
    pub sampled_row_count: u64,
    pub affected_sections: Vec<String>,
}

/// INV-011/014: measurements, profiles, diagnostics — never
/// recommendations or business interpretation. Raw engine output; only
/// `ReportEngine::generate_inspection_report` wraps this into an
/// `InspectionReport` (INV-015).
#[derive(Debug, Clone, Serialize)]
pub struct InspectionResults {
    pub dataset_identity: sections::DatasetIdentity,
    pub import_health: sections::ImportHealth,
    pub dataset_shape: sections::DatasetShape,
    pub completeness: sections::Completeness,
    pub data_types: sections::DataTypes,
    pub values_and_cardinality: sections::ValuesAndCardinality,
    pub duplicates_and_matches: sections::DuplicatesAndMatches,
    pub number_profile: sections::NumberProfile,
    pub category_profile: sections::CategoryProfile,
    pub date_time_profile: sections::DateTimeProfile,
    pub text_profile: sections::TextProfile,
    pub format_patterns: sections::FormatPatterns,
    pub whitespace_and_hidden_characters: sections::WhitespaceAndHiddenCharacters,
    pub row_profile: sections::RowProfile,
    pub column_cards: Vec<sections::ColumnCard>,
    pub column_relationships: sections::ColumnRelationships,
    pub distribution_views: sections::DistributionViews,
    pub data_checks: sections::DataChecks,
    pub metadata_coverage: sections::MetadataCoverage,
    pub evidence_trail: sections::EvidenceTrail,
    pub sampling: Option<SamplingDisclosure>,
    pub emitted_metrics: Vec<EmittedMetric>,
}

/// INV-010: inspection only — never accepts user-defined rules, never
/// calls another engine, never writes to a Table.
pub trait InspectionEngine {
    fn inspect(&self, content: TableContent, import_summary: ImportSummary) -> Result<InspectionResults, InspectionError> {
        let _ = (content, import_summary);
        todo!()
    }
}

/// The native Rust/Polars implementation — Inspection has no Python-backed
/// path at all (INV-029's exception list applies per-operation, and no
/// Inspection operation is on it).
pub struct NativeInspectionEngine;

impl InspectionEngine for NativeInspectionEngine {
    fn inspect(&self, content: TableContent, import_summary: ImportSummary) -> Result<InspectionResults, InspectionError> {
        inspect_standard(content, import_summary, &ThresholdRegistry::v1(), "0.1.0", 0)
    }
}

/// Internal, independently testable entry point — takes the Threshold
/// Registry and inspection timestamp explicitly so both are verifiable
/// without going through `NativeInspectionEngine`'s fixed defaults (A.5's
/// "changing a registry value should change the grouping without a code
/// change" test needs to swap the registry in; a fixed `inspected_at`
/// keeps section tests deterministic).
pub fn inspect_standard(
    content: TableContent,
    import_summary: ImportSummary,
    thresholds: &ThresholdRegistry,
    engine_version: &str,
    inspected_at: Timestamp,
) -> Result<InspectionResults, InspectionError> {
    let table_id: TableId = content.table_id;
    let content_fingerprint = content.content_fingerprint.clone();
    let full_df = content.frame.clone().collect()?;
    let schema = full_df.schema();
    let row_count = full_df.height() as u64;

    let sampling_needed = row_count > thresholds.sample_trigger_row_count;
    let sample_exactness = if sampling_needed { metric_dictionary::Exactness::Sampled } else { metric_dictionary::Exactness::Exact };
    let sample_df = if sampling_needed {
        full_df.sample_n_literal(thresholds.sample_size.min(full_df.height()), false, false, Some(0))?
    } else {
        full_df.clone()
    };
    let sample_lf = sample_df.clone().lazy();
    let full_lf = full_df.clone().lazy();

    let mut sink = MetricSink { metrics: Vec::new() };

    let dataset_identity = sections::dataset_identity(table_id, &content_fingerprint, row_count, schema.len() as u64, &mut sink);
    let import_health = sections::import_health(&import_summary, &mut sink);
    let dataset_shape = sections::dataset_shape(&full_df, &mut sink);
    let completeness = sections::completeness(&full_df, thresholds, &mut sink)?;
    let data_types = sections::data_types(&full_df, &mut sink);
    let values_and_cardinality = sections::values_and_cardinality(&full_df, thresholds, &mut sink)?;
    let duplicates_and_matches = sections::duplicates_and_matches(&full_df, &mut sink)?;
    let number_profile = sections::number_profile(&full_lf, &schema, &mut sink)?;
    let category_profile = sections::category_profile(&sample_df, &schema, thresholds, sample_exactness, &mut sink)?;
    let date_time_profile = sections::date_time_profile(&full_lf, &schema, &mut sink)?;
    let text_profile = sections::text_profile(&sample_df, &schema, thresholds, sample_exactness, &mut sink)?;
    let format_patterns = sections::format_patterns(&sample_df, &schema, thresholds, sample_exactness, &mut sink)?;
    let whitespace_and_hidden_characters =
        sections::whitespace_and_hidden_characters(&sample_df, &schema, sample_exactness, &mut sink)?;
    let row_profile = sections::row_profile(&full_df, thresholds, &mut sink);
    let column_cards = sections::column_cards(&full_df, &data_types, &completeness, &values_and_cardinality, &mut sink);
    let column_relationships = sections::column_relationships(&sample_lf, &schema, sample_exactness, &mut sink)?;
    let distribution_views = sections::distribution_views(&sample_df, &schema, sample_exactness, &mut sink)?;
    let data_checks = sections::data_checks(&completeness, &duplicates_and_matches, &whitespace_and_hidden_characters, &row_profile, thresholds, &mut sink);
    let metadata_coverage = sections::metadata_coverage(&data_types, &mut sink);
    let evidence_trail = sections::evidence_trail(table_id, &content_fingerprint, inspected_at, engine_version, &mut sink);

    let sampling = if sampling_needed {
        Some(SamplingDisclosure {
            sample_size: sample_df.height(),
            sampling_method: "random, seed=0".to_string(),
            seed: Some(0),
            full_row_count: row_count,
            sampled_row_count: sample_df.height() as u64,
            affected_sections: vec![
                "Category Profile".to_string(),
                "Text Profile".to_string(),
                "Format Patterns".to_string(),
                "Whitespace & Hidden Characters".to_string(),
                "Column Relationships".to_string(),
                "Distribution Views".to_string(),
            ],
        })
    } else {
        None
    };

    Ok(InspectionResults {
        dataset_identity,
        import_health,
        dataset_shape,
        completeness,
        data_types,
        values_and_cardinality,
        duplicates_and_matches,
        number_profile,
        category_profile,
        date_time_profile,
        text_profile,
        format_patterns,
        whitespace_and_hidden_characters,
        row_profile,
        column_cards,
        column_relationships,
        distribution_views,
        data_checks,
        metadata_coverage,
        evidence_trail,
        sampling,
        emitted_metrics: sink.metrics,
    })
}
