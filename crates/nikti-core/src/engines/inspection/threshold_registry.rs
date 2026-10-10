//! Amendment 3 (Inspection Report Design Spec): any threshold-based
//! grouping reads its threshold from this single versioned registry, never
//! from a literal embedded in a computation. Changing a threshold is a
//! tracked, versioned change (bump `version`), not a silent constant edit.
//!
//! This registry is a fixed system default, not a caller-supplied
//! `definition` — INV-011 forbids Inspection from accepting a
//! ruleset/config parameter. Compute functions in `sections.rs` take
//! `&ThresholdRegistry` explicitly so the "reads from the registry, not a
//! literal" property is independently testable without changing
//! `InspectionEngine::inspect`'s public signature.

#[derive(Debug, Clone, PartialEq)]
pub struct ThresholdRegistry {
    /// Registry schema version. Bump whenever a threshold value below
    /// changes, so every report's provenance can note which version of the
    /// thresholds produced its groupings.
    pub version: u32,

    /// A value's share of a column's non-null values below which it is
    /// grouped as "rare" in Category Profile.
    pub rare_value_max_ratio: f64,
    /// A column's missing-value ratio at or above which it is grouped as
    /// "high missingness" in Completeness.
    pub high_missingness_min_ratio: f64,
    /// A column's distinct-value ratio at or above which it is grouped as
    /// "high cardinality" in Values & Cardinality.
    pub high_cardinality_min_ratio: f64,
    /// Character length at or above which a text value is grouped as
    /// "long text" in Text Profile.
    pub long_text_min_chars: u64,
    /// A row's missing-field ratio at or above which it is flagged in the
    /// Row Condition Index (Row Profile).
    pub row_high_missingness_min_ratio: f64,
    /// A column's distinct-format-shape ratio at or above which it is
    /// grouped as "format variation" in Format Patterns.
    pub format_variation_min_distinct_ratio: f64,
    /// The dominant format shape's share of non-null values at or above
    /// which Format Patterns reports that shape as dominant.
    pub pattern_dominance_min_ratio: f64,
    /// Row count above which row-materializing sections switch from full
    /// data to a deterministic sample (Amendment 4 / A.6 sampling
    /// disclosure).
    pub sample_trigger_row_count: u64,
    /// Sample size drawn once `sample_trigger_row_count` is exceeded.
    pub sample_size: usize,
}

impl ThresholdRegistry {
    /// The v1 default registry. Not a ruleset — a fixed system default that
    /// applies uniformly to every Inspection run.
    pub fn v1() -> Self {
        Self {
            version: 1,
            rare_value_max_ratio: 0.01,
            high_missingness_min_ratio: 0.20,
            high_cardinality_min_ratio: 0.90,
            long_text_min_chars: 256,
            row_high_missingness_min_ratio: 0.50,
            format_variation_min_distinct_ratio: 0.05,
            pattern_dominance_min_ratio: 0.80,
            sample_trigger_row_count: 500_000,
            sample_size: 50_000,
        }
    }
}

impl Default for ThresholdRegistry {
    fn default() -> Self {
        Self::v1()
    }
}
