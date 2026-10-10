//! Analysis Procedure Catalogue v1.0 (as summarized in the governing
//! task prompt — the full source document was unavailable in this
//! repository at implementation time; see the final compliance report).
//! Frozen at 19 procedures for V1. Part A (binding terminology):
//! **Analysis Procedure**, never "Analysis Rule" — `procedure_id`/
//! `procedure_type`/`AnalysisProcedure`, mirrored here as
//! `AnalysisProcedureParams` (the per-procedure-type typed parameter
//! shape, same pattern as `ValidationFunctionParams`).

use serde::{Deserialize, Serialize};

use super::transformation_operation::AggFunc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CorrelationMethod {
    Pearson,
    Spearman,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoundaryMethod {
    Iqr,
    ZScore,
    Percentile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DateBucket {
    Day,
    Week,
    Month,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnalysisProcedureParams {
    /// AP-001 (Amendment 5: mandatory, always first — see
    /// `AnalysisRulesetVersion::new`, which always inserts this
    /// variant and never allows it to be removed).
    ScopeAndVariableRoleSetup,
    /// AP-002. `formula` is compiled via the same native
    /// `FormulaParser` Transformation already uses — deterministic,
    /// never written back to the Dataset/Table (it is a measurement,
    /// not a Transformation step).
    DerivedMeasureDefinition { measure_name: String, formula: String },
    /// AP-003.
    ColumnPropertyExplorer { columns: Vec<String> },
    /// AP-004.
    GroupedSummary { group_by: Vec<String>, measure_column: String, agg: AggFunc },
    /// AP-005.
    SegmentSubsetComparison { segment_column: String, measure_column: String, agg: AggFunc },
    /// AP-006.
    CrossTabulation { row_column: String, column_column: String },
    /// AP-007.
    CategoryToNumericComparison { category_column: String, numeric_column: String },
    /// AP-008. "pairwise numeric coefficients" (C.2) — never
    /// "correlation matrix" as a label.
    NumericRelationshipAssociation { columns: Vec<String>, method: CorrelationMethod },
    /// AP-009.
    DistributionBoundaryExplorer { column: String, method: BoundaryMethod },
    /// AP-010.
    MissingnessStructureExplorer { columns: Vec<String> },
    /// AP-011.
    RepetitionDuplicateStructureExplorer { key_columns: Vec<String> },
    /// AP-012. `regex_list` is the catalogue's `regex list` parameter —
    /// `None` (or an empty list) means this ruleset version didn't
    /// configure any regex, in which case `regex_measure_table` is not
    /// executable for that run.
    PatternFormatStructureExplorer { column: String, regex_list: Option<Vec<String>> },
    /// AP-013. No forecasting. `duration_start_column`/`duration_end_column`
    /// are the catalogue's `duration start/end` parameter pair — a
    /// per-row difference between two named date/time columns (same
    /// "duration" definition AP-002 uses, e.g. `delay_days = payment_date
    /// - invoice_date`), never the gap between consecutive observations
    /// in `date_column`. `None` means this ruleset version didn't supply
    /// them, in which case `duration_summary_table` is not executable —
    /// there is no fallback computation.
    DateTimeTrendExplorer { date_column: String, bucket: DateBucket, duration_start_column: Option<String>, duration_end_column: Option<String> },
    /// AP-014.
    ParetoConcentrationExplorer { category_column: String, measure_column: String },
    /// AP-015. No sentiment/topic modeling.
    TextMeasurementExplorer { column: String, keywords: Vec<String> },
    /// AP-016. `segment_columns` is the catalogue's `segment columns`
    /// parameter — `None` (or an empty list) means this ruleset version
    /// didn't configure any segment column, in which case
    /// `target_by_segment_table` is not executable for that run.
    TargetLens { target_column: String, segment_columns: Option<Vec<String>> },
    /// AP-017. Never called "feature importance" anywhere in output.
    AssociationStrengthRanking { target_column: String, candidate_columns: Vec<String> },
    /// AP-018. Single-object only — never cross-table joinability.
    DependencyCandidateExplorer { columns: Vec<String> },
    /// AP-019 (Amendment 6: "retention review"/"row attention review" —
    /// never "drop review"). Aggregates evidence already produced by
    /// other procedures in the same run; carries no parameters of its
    /// own.
    FeatureAndRowAttentionScreen,
}

impl AnalysisProcedureParams {
    pub fn procedure_type(&self) -> &'static str {
        match self {
            Self::ScopeAndVariableRoleSetup => "AP-001",
            Self::DerivedMeasureDefinition { .. } => "AP-002",
            Self::ColumnPropertyExplorer { .. } => "AP-003",
            Self::GroupedSummary { .. } => "AP-004",
            Self::SegmentSubsetComparison { .. } => "AP-005",
            Self::CrossTabulation { .. } => "AP-006",
            Self::CategoryToNumericComparison { .. } => "AP-007",
            Self::NumericRelationshipAssociation { .. } => "AP-008",
            Self::DistributionBoundaryExplorer { .. } => "AP-009",
            Self::MissingnessStructureExplorer { .. } => "AP-010",
            Self::RepetitionDuplicateStructureExplorer { .. } => "AP-011",
            Self::PatternFormatStructureExplorer { .. } => "AP-012",
            Self::DateTimeTrendExplorer { .. } => "AP-013",
            Self::ParetoConcentrationExplorer { .. } => "AP-014",
            Self::TextMeasurementExplorer { .. } => "AP-015",
            Self::TargetLens { .. } => "AP-016",
            Self::AssociationStrengthRanking { .. } => "AP-017",
            Self::DependencyCandidateExplorer { .. } => "AP-018",
            Self::FeatureAndRowAttentionScreen => "AP-019",
        }
    }
}
