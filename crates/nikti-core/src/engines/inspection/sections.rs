//! The 20 V1 Essential + V1 Signature sections (Inspection Report Design
//! Spec A.1). Every computed value here is exact/full-data unless the
//! caller passed a sampled `DataFrame` (see `mod.rs`'s sampling decision).
//! Terminology corrections (A.2) are load-bearing in the type/field names
//! themselves, not just prose: `columns_with_100pct_uniqueness` (never
//! "candidate key"), `pairwise_numeric_coefficients` (never "correlation
//! matrix"), `QuartileIqrSummary` (never "box plot"), `completeness_map`
//! (never a generic "heatmap"), `RowConditionIndex`/`CharacterConditionTable`
//! (never "issue index"/"issue table"), `detected_condition_count` (never
//! "issue count").

use std::collections::BTreeMap;

use polars::prelude::*;
use serde::Serialize;

use super::metric_dictionary::Exactness;
use super::threshold_registry::ThresholdRegistry;
use crate::types::common::{TableId, Timestamp};
use crate::types::TrustLabel;

use super::InspectionError;

/// One entry in the flat cross-cutting log used by the Metric Dictionary
/// coverage test and the banned-vocabulary scan (A.4, test list). Not
/// itself report content — a verification aid built alongside it.
#[derive(Debug, Clone, Serialize)]
pub struct EmittedMetric {
    pub key: &'static str,
    pub label: TrustLabel,
    pub rendered: String,
}

pub struct MetricSink {
    pub metrics: Vec<EmittedMetric>,
}

impl MetricSink {
    fn new() -> Self {
        Self { metrics: Vec::new() }
    }

    fn emit(&mut self, key: &'static str, label: TrustLabel, rendered: impl std::fmt::Debug) {
        debug_assert!(
            super::metric_dictionary::contains(key),
            "metric key '{key}' has no Metric Dictionary entry"
        );
        self.metrics.push(EmittedMetric { key, label, rendered: format!("{rendered:?}") });
    }
}

// ---------------------------------------------------------------------
// 1. Dataset Identity
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DatasetIdentity {
    pub table_id: TableId,
    pub row_count: u64,
    pub column_count: u64,
    pub content_fingerprint: String,
}

pub fn dataset_identity(
    table_id: TableId,
    content_fingerprint: &str,
    row_count: u64,
    column_count: u64,
    sink: &mut MetricSink,
) -> DatasetIdentity {
    sink.emit("identity.table_id", TrustLabel::Metadata, table_id);
    sink.emit("identity.row_count", TrustLabel::Measured, row_count);
    sink.emit("identity.column_count", TrustLabel::Measured, column_count);
    sink.emit("identity.content_fingerprint", TrustLabel::Metadata, content_fingerprint);
    DatasetIdentity { table_id, row_count, column_count, content_fingerprint: content_fingerprint.to_string() }
}

// ---------------------------------------------------------------------
// 2. Import Health
// ---------------------------------------------------------------------

/// Factual evidence about the import event that produced this Table —
/// not a ruleset or behavior configuration (INV-011 forbids Inspection
/// accepting one), just additional input evidence, analogous to `content`
/// itself. A default of 0/`None` means "not recorded", never a fabricated
/// "detected zero".
#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportSummary {
    pub rows_read: u64,
    pub rows_rejected: u64,
    pub parse_error_count: u64,
    pub encoding: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportHealth {
    pub rows_read: u64,
    pub rows_rejected: u64,
    pub parse_error_count: u64,
    pub encoding: Option<String>,
}

pub fn import_health(summary: &ImportSummary, sink: &mut MetricSink) -> ImportHealth {
    sink.emit("import_health.rows_read", TrustLabel::Metadata, summary.rows_read);
    sink.emit("import_health.rows_rejected", TrustLabel::Metadata, summary.rows_rejected);
    sink.emit("import_health.parse_error_count", TrustLabel::Metadata, summary.parse_error_count);
    sink.emit("import_health.encoding", TrustLabel::Metadata, &summary.encoding);
    ImportHealth {
        rows_read: summary.rows_read,
        rows_rejected: summary.rows_rejected,
        parse_error_count: summary.parse_error_count,
        encoding: summary.encoding.clone(),
    }
}

// ---------------------------------------------------------------------
// 3. Dataset Shape
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DatasetShape {
    pub row_count: u64,
    pub column_count: u64,
    pub estimated_memory_bytes: u64,
}

pub fn dataset_shape(df: &DataFrame, sink: &mut MetricSink) -> DatasetShape {
    let row_count = df.height() as u64;
    let column_count = df.width() as u64;
    let estimated_memory_bytes = df.estimated_size() as u64;
    sink.emit("shape.row_count", TrustLabel::Measured, row_count);
    sink.emit("shape.column_count", TrustLabel::Measured, column_count);
    sink.emit("shape.estimated_memory_bytes", TrustLabel::Derived, estimated_memory_bytes);
    DatasetShape { row_count, column_count, estimated_memory_bytes }
}

// ---------------------------------------------------------------------
// 4. Completeness
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ColumnCompleteness {
    pub column: String,
    pub missing_ratio: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Completeness {
    pub overall_missing_ratio: f64,
    /// Per-column missingness, i.e. the "Completeness Map" (A.2: never a
    /// generic correlation-style heatmap — this is the one and only
    /// heatmap-shaped concept Inspection computes).
    pub completeness_map: Vec<ColumnCompleteness>,
    pub high_missingness_columns: Vec<String>,
}

pub fn completeness(df: &DataFrame, thresholds: &ThresholdRegistry, sink: &mut MetricSink) -> Result<Completeness, InspectionError> {
    let row_count = df.height().max(1) as f64;
    let mut completeness_map = Vec::new();
    let mut high_missingness_columns = Vec::new();
    let mut total_nulls: u64 = 0;

    for name in df.get_column_names() {
        let s = df.column(name)?;
        let nulls = s.null_count() as u64;
        total_nulls += nulls;
        let ratio = nulls as f64 / row_count;
        sink.emit("completeness.column_missing_ratio", TrustLabel::Measured, (name, ratio));
        if ratio >= thresholds.high_missingness_min_ratio {
            high_missingness_columns.push(name.to_string());
        }
        completeness_map.push(ColumnCompleteness { column: name.to_string(), missing_ratio: ratio });
    }

    let total_cells = (df.height() * df.width()).max(1) as f64;
    let overall_missing_ratio = total_nulls as f64 / total_cells;
    sink.emit("completeness.overall_missing_ratio", TrustLabel::Measured, overall_missing_ratio);

    Ok(Completeness { overall_missing_ratio, completeness_map, high_missingness_columns })
}

// ---------------------------------------------------------------------
// 5. Data Types
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DataTypes {
    pub per_column: Vec<(String, String)>,
}

pub fn data_types(df: &DataFrame, sink: &mut MetricSink) -> DataTypes {
    let mut per_column = Vec::new();
    for name in df.get_column_names() {
        let dtype_str = format!("{:?}", df.column(name).unwrap().dtype());
        sink.emit("data_types.column_dtype", TrustLabel::Metadata, (name, &dtype_str));
        per_column.push((name.to_string(), dtype_str));
    }
    DataTypes { per_column }
}

// ---------------------------------------------------------------------
// 6. Values & Cardinality
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ColumnCardinality {
    pub column: String,
    pub distinct_count: u64,
    pub distinct_ratio: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValuesAndCardinality {
    pub per_column: Vec<ColumnCardinality>,
    /// A.2: never "candidate key" — columns whose values are all distinct.
    pub columns_with_100pct_uniqueness: Vec<String>,
    pub high_cardinality_columns: Vec<String>,
}

pub fn values_and_cardinality(
    df: &DataFrame,
    thresholds: &ThresholdRegistry,
    sink: &mut MetricSink,
) -> Result<ValuesAndCardinality, InspectionError> {
    let row_count = df.height().max(1) as f64;
    let mut per_column = Vec::new();
    let mut columns_with_100pct_uniqueness = Vec::new();
    let mut high_cardinality_columns = Vec::new();

    for name in df.get_column_names() {
        let s = df.column(name)?;
        let distinct_count = s.as_materialized_series().n_unique()? as u64;
        let ratio = distinct_count as f64 / row_count;
        sink.emit("cardinality.distinct_count", TrustLabel::Measured, (name, distinct_count));
        sink.emit("cardinality.distinct_ratio", TrustLabel::Derived, (name, ratio));
        if distinct_count as usize == df.height() && df.height() > 0 && s.null_count() == 0 {
            columns_with_100pct_uniqueness.push(name.to_string());
        }
        if ratio >= thresholds.high_cardinality_min_ratio {
            high_cardinality_columns.push(name.to_string());
        }
        per_column.push(ColumnCardinality { column: name.to_string(), distinct_count, distinct_ratio: ratio });
    }
    sink.emit(
        "cardinality.columns_with_100pct_uniqueness",
        TrustLabel::Derived,
        &columns_with_100pct_uniqueness,
    );

    Ok(ValuesAndCardinality { per_column, columns_with_100pct_uniqueness, high_cardinality_columns })
}

// ---------------------------------------------------------------------
// 7. Duplicates & Matches
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DuplicatesAndMatches {
    pub exact_duplicate_row_count: u64,
    pub duplicate_row_ratio: f64,
}

pub fn duplicates_and_matches(df: &DataFrame, sink: &mut MetricSink) -> Result<DuplicatesAndMatches, InspectionError> {
    let row_count = df.height();
    let exact_duplicate_row_count = if row_count == 0 {
        0
    } else {
        let mask = df.is_duplicated()?;
        mask.into_iter().filter(|v| v == &Some(true)).count() as u64
    };
    let ratio = if row_count == 0 { 0.0 } else { exact_duplicate_row_count as f64 / row_count as f64 };
    sink.emit("duplicates.exact_duplicate_row_count", TrustLabel::Measured, exact_duplicate_row_count);
    sink.emit("duplicates.duplicate_row_ratio", TrustLabel::Derived, ratio);
    Ok(DuplicatesAndMatches { exact_duplicate_row_count, duplicate_row_ratio: ratio })
}

// ---------------------------------------------------------------------
// 8. Number Profile
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct QuartileIqrSummary {
    pub q1: f64,
    pub q2_median: f64,
    pub q3: f64,
    pub iqr: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct NumericColumnProfile {
    pub column: String,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub std_dev: Option<f64>,
    pub quartiles: QuartileIqrSummary,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct NumberProfile {
    pub per_column: Vec<NumericColumnProfile>,
}

pub fn number_profile(lf: &LazyFrame, schema: &Schema, sink: &mut MetricSink) -> Result<NumberProfile, InspectionError> {
    let mut per_column = Vec::new();
    for (name, dtype) in schema.iter() {
        if !dtype.is_numeric() {
            continue;
        }
        let name = name.as_str();
        let stats = lf
            .clone()
            .select([
                col(name).cast(DataType::Float64).min().alias("__min"),
                col(name).cast(DataType::Float64).max().alias("__max"),
                col(name).cast(DataType::Float64).mean().alias("__mean"),
                col(name).cast(DataType::Float64).std(1).alias("__std"),
                col(name).cast(DataType::Float64).quantile(0.25.into(), QuantileMethod::Linear).alias("__q1"),
                col(name).cast(DataType::Float64).quantile(0.5.into(), QuantileMethod::Linear).alias("__q2"),
                col(name).cast(DataType::Float64).quantile(0.75.into(), QuantileMethod::Linear).alias("__q3"),
            ])
            .collect()?;
        let min = stats.column("__min")?.f64()?.get(0).unwrap_or(0.0);
        let max = stats.column("__max")?.f64()?.get(0).unwrap_or(0.0);
        let mean = stats.column("__mean")?.f64()?.get(0).unwrap_or(0.0);
        let std_dev = stats.column("__std")?.f64()?.get(0);
        let q1 = stats.column("__q1")?.f64()?.get(0).unwrap_or(0.0);
        let q2 = stats.column("__q2")?.f64()?.get(0).unwrap_or(0.0);
        let q3 = stats.column("__q3")?.f64()?.get(0).unwrap_or(0.0);
        let quartiles = QuartileIqrSummary { q1, q2_median: q2, q3, iqr: q3 - q1 };

        sink.emit("number_profile.min", TrustLabel::Measured, (name, min));
        sink.emit("number_profile.max", TrustLabel::Measured, (name, max));
        sink.emit("number_profile.mean", TrustLabel::Derived, (name, mean));
        sink.emit("number_profile.median", TrustLabel::Derived, (name, q2));
        sink.emit("number_profile.std_dev", TrustLabel::Derived, (name, std_dev));
        sink.emit("number_profile.quartile_iqr_summary", TrustLabel::Derived, (name, &quartiles.iqr));

        per_column.push(NumericColumnProfile { column: name.to_string(), min, max, mean, std_dev, quartiles });
    }
    Ok(NumberProfile { per_column })
}

// ---------------------------------------------------------------------
// 9. Category Profile
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct CategoryColumnProfile {
    pub column: String,
    pub top_values: Vec<(String, u64)>,
    pub rare_value_count: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CategoryProfile {
    pub per_column: Vec<CategoryColumnProfile>,
}

pub fn category_profile(
    df: &DataFrame,
    schema: &Schema,
    thresholds: &ThresholdRegistry,
    exactness: Exactness,
    sink: &mut MetricSink,
) -> Result<CategoryProfile, InspectionError> {
    let label = if exactness == Exactness::Sampled { TrustLabel::Sampled } else { TrustLabel::Measured };
    let mut per_column = Vec::new();
    for (name, dtype) in schema.iter() {
        if !matches!(dtype, DataType::String | DataType::Boolean) {
            continue;
        }
        let name = name.as_str();
        let s = df.column(name)?.cast(&DataType::String)?;
        let ca = s.str()?;
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        let mut total = 0u64;
        for v in ca.into_iter().flatten() {
            *counts.entry(v.to_string()).or_insert(0) += 1;
            total += 1;
        }
        let mut top_values: Vec<(String, u64)> = counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
        top_values.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        top_values.truncate(10);

        let rare_value_count = counts
            .values()
            .filter(|&&c| total > 0 && (c as f64 / total as f64) <= thresholds.rare_value_max_ratio)
            .count() as u64;

        sink.emit("category_profile.top_values", label, (name, &top_values));
        sink.emit("category_profile.rare_value_count", label, (name, rare_value_count));

        per_column.push(CategoryColumnProfile { column: name.to_string(), top_values, rare_value_count });
    }
    Ok(CategoryProfile { per_column })
}

// ---------------------------------------------------------------------
// 10. Date & Time Profile
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DateTimeColumnProfile {
    pub column: String,
    pub min: String,
    pub max: String,
    pub distinct_count: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DateTimeProfile {
    pub per_column: Vec<DateTimeColumnProfile>,
}

pub fn date_time_profile(lf: &LazyFrame, schema: &Schema, sink: &mut MetricSink) -> Result<DateTimeProfile, InspectionError> {
    let mut per_column = Vec::new();
    for (name, dtype) in schema.iter() {
        if !dtype.is_temporal() {
            continue;
        }
        let name = name.as_str();
        let stats = lf
            .clone()
            .select([
                col(name).cast(DataType::String).min().alias("__min"),
                col(name).cast(DataType::String).max().alias("__max"),
                col(name).n_unique().alias("__nunique"),
            ])
            .collect()?;
        let min = stats.column("__min")?.str()?.get(0).unwrap_or("").to_string();
        let max = stats.column("__max")?.str()?.get(0).unwrap_or("").to_string();
        let distinct_count = stats.column("__nunique")?.u32()?.get(0).unwrap_or(0) as u64;

        sink.emit("datetime_profile.min", TrustLabel::Measured, (name, &min));
        sink.emit("datetime_profile.max", TrustLabel::Measured, (name, &max));
        sink.emit("datetime_profile.distinct_count", TrustLabel::Measured, (name, distinct_count));

        per_column.push(DateTimeColumnProfile { column: name.to_string(), min, max, distinct_count });
    }
    Ok(DateTimeProfile { per_column })
}

// ---------------------------------------------------------------------
// 11. Text Profile
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct TextColumnProfile {
    pub column: String,
    pub min_length: u64,
    pub max_length: u64,
    pub mean_length: f64,
    pub long_text_count: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TextProfile {
    pub per_column: Vec<TextColumnProfile>,
}

pub fn text_profile(
    df: &DataFrame,
    schema: &Schema,
    thresholds: &ThresholdRegistry,
    exactness: Exactness,
    sink: &mut MetricSink,
) -> Result<TextProfile, InspectionError> {
    let label = if exactness == Exactness::Sampled { TrustLabel::Sampled } else { TrustLabel::Measured };
    let mut per_column = Vec::new();
    for (name, dtype) in schema.iter() {
        if !matches!(dtype, DataType::String) {
            continue;
        }
        let name = name.as_str();
        let ca = df.column(name)?.str()?;
        let lengths: Vec<u64> = ca.into_iter().flatten().map(|v| v.chars().count() as u64).collect();
        if lengths.is_empty() {
            continue;
        }
        let min_length = *lengths.iter().min().unwrap();
        let max_length = *lengths.iter().max().unwrap();
        let mean_length = lengths.iter().sum::<u64>() as f64 / lengths.len() as f64;
        let long_text_count = lengths.iter().filter(|&&l| l >= thresholds.long_text_min_chars).count() as u64;

        sink.emit("text_profile.min_length", label, (name, min_length));
        sink.emit("text_profile.max_length", label, (name, max_length));
        sink.emit("text_profile.mean_length", label, (name, mean_length));
        sink.emit("text_profile.long_text_count", label, (name, long_text_count));

        per_column.push(TextColumnProfile { column: name.to_string(), min_length, max_length, mean_length, long_text_count });
    }
    Ok(TextProfile { per_column })
}

// ---------------------------------------------------------------------
// 12. Format Patterns (structural shape only — never a semantic type
// verdict; see engines/inspection/reserved.rs for why semantic typing is
// out of V1 scope entirely).
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct FormatPatternProfile {
    pub column: String,
    pub distinct_format_count: u64,
    pub dominant_format_ratio: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct FormatPatterns {
    pub per_column: Vec<FormatPatternProfile>,
}

/// Reduces a value to a structural shape: digits -> '9', ASCII letters ->
/// 'A'/'a', everything else preserved. Purely structural — carries no
/// semantic meaning about what the column "is".
fn format_shape(v: &str) -> String {
    v.chars()
        .map(|c| if c.is_ascii_digit() { '9' } else if c.is_ascii_uppercase() { 'A' } else if c.is_ascii_lowercase() { 'a' } else { c })
        .collect()
}

pub fn format_patterns(
    df: &DataFrame,
    schema: &Schema,
    thresholds: &ThresholdRegistry,
    exactness: Exactness,
    sink: &mut MetricSink,
) -> Result<FormatPatterns, InspectionError> {
    // Amendment 4: sampling disclosure outranks the section's normal
    // `Pattern` label — a value computed on a sample is tagged `Sampled`,
    // never silently presented under its unsampled label.
    let label = if exactness == Exactness::Sampled { TrustLabel::Sampled } else { TrustLabel::Pattern };
    let mut per_column = Vec::new();
    for (name, dtype) in schema.iter() {
        if !matches!(dtype, DataType::String) {
            continue;
        }
        let name = name.as_str();
        let ca = df.column(name)?.str()?;
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        let mut total = 0u64;
        for v in ca.into_iter().flatten() {
            *counts.entry(format_shape(v)).or_insert(0) += 1;
            total += 1;
        }
        if total == 0 {
            continue;
        }
        let distinct_format_count = counts.len() as u64;
        let dominant = counts.values().max().copied().unwrap_or(0);
        let dominant_format_ratio = dominant as f64 / total as f64;

        sink.emit("format_patterns.distinct_format_count", label, (name, distinct_format_count));
        sink.emit("format_patterns.dominant_format_ratio", label, (name, dominant_format_ratio));
        let _ = thresholds.pattern_dominance_min_ratio;

        per_column.push(FormatPatternProfile { column: name.to_string(), distinct_format_count, dominant_format_ratio });
    }
    Ok(FormatPatterns { per_column })
}

// ---------------------------------------------------------------------
// 13. Whitespace & Hidden Characters (includes the Character Condition
// Table — A.2: never "Character Issue Table").
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct CharacterConditionEntry {
    pub column: String,
    pub leading_or_trailing_whitespace_count: u64,
    pub control_char_count: u64,
    pub non_ascii_count: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct WhitespaceAndHiddenCharacters {
    pub character_condition_table: Vec<CharacterConditionEntry>,
}

pub fn whitespace_and_hidden_characters(
    df: &DataFrame,
    schema: &Schema,
    exactness: Exactness,
    sink: &mut MetricSink,
) -> Result<WhitespaceAndHiddenCharacters, InspectionError> {
    // Amendment 4: sampling disclosure outranks the section's normal
    // `Detected` label when the input was sampled.
    let label = if exactness == Exactness::Sampled { TrustLabel::Sampled } else { TrustLabel::Detected };
    let mut character_condition_table = Vec::new();
    for (name, dtype) in schema.iter() {
        if !matches!(dtype, DataType::String) {
            continue;
        }
        let name = name.as_str();
        let ca = df.column(name)?.str()?;
        let mut whitespace = 0u64;
        let mut control = 0u64;
        let mut non_ascii = 0u64;
        for v in ca.into_iter().flatten() {
            if v != v.trim() {
                whitespace += 1;
            }
            if v.chars().any(|c| c.is_control()) {
                control += 1;
            }
            if v.chars().any(|c| !c.is_ascii()) {
                non_ascii += 1;
            }
        }
        sink.emit("whitespace.leading_trailing_count", label, (name, whitespace));
        sink.emit("whitespace.control_char_count", label, (name, control));
        sink.emit("character_condition_table.non_ascii_count", label, (name, non_ascii));

        character_condition_table.push(CharacterConditionEntry {
            column: name.to_string(),
            leading_or_trailing_whitespace_count: whitespace,
            control_char_count: control,
            non_ascii_count: non_ascii,
        });
    }
    Ok(WhitespaceAndHiddenCharacters { character_condition_table })
}

// ---------------------------------------------------------------------
// 14. Row Profile (A.2: never "Row Issue Index" — Row Condition Index)
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct RowCondition {
    pub row_index: u64,
    pub missing_field_count: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RowProfile {
    pub row_condition_index: Vec<RowCondition>,
    pub rows_flagged_high_missingness: u64,
}

pub fn row_profile(df: &DataFrame, thresholds: &ThresholdRegistry, sink: &mut MetricSink) -> RowProfile {
    let column_count = df.width().max(1) as f64;
    let mut row_condition_index = Vec::new();
    let mut flagged = 0u64;
    for row_idx in 0..df.height() {
        let mut missing = 0u64;
        for name in df.get_column_names() {
            if df.column(name).unwrap().get(row_idx).map(|v| v.is_null()).unwrap_or(true) {
                missing += 1;
            }
        }
        let ratio = missing as f64 / column_count;
        if ratio >= thresholds.row_high_missingness_min_ratio {
            flagged += 1;
            row_condition_index.push(RowCondition { row_index: row_idx as u64, missing_field_count: missing });
        }
    }
    sink.emit("row_profile.row_condition_index", TrustLabel::Derived, row_condition_index.len());
    sink.emit("row_profile.rows_flagged_high_missingness", TrustLabel::Derived, flagged);
    RowProfile { row_condition_index, rows_flagged_high_missingness: flagged }
}

// ---------------------------------------------------------------------
// 15. Column Cards (composite per-column summary)
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ColumnCard {
    pub column: String,
    pub dtype: String,
    pub missing_ratio: f64,
    pub distinct_count: u64,
    pub sample_values: Vec<String>,
}

pub fn column_cards(
    df: &DataFrame,
    data_types: &DataTypes,
    completeness: &Completeness,
    cardinality: &ValuesAndCardinality,
    sink: &mut MetricSink,
) -> Vec<ColumnCard> {
    let mut cards = Vec::new();
    for (i, name) in df.get_column_names().into_iter().enumerate() {
        let dtype = data_types.per_column.get(i).map(|(_, d)| d.clone()).unwrap_or_default();
        let missing_ratio = completeness.completeness_map.get(i).map(|c| c.missing_ratio).unwrap_or(0.0);
        let distinct_count = cardinality.per_column.get(i).map(|c| c.distinct_count).unwrap_or(0);
        let sample_values: Vec<String> = (0..df.height().min(3))
            .filter_map(|r| df.column(name).ok().and_then(|s| s.get(r).ok()).map(|v| format!("{v}")))
            .collect();
        sink.emit("column_cards.summary", TrustLabel::Derived, name);
        cards.push(ColumnCard { column: name.to_string(), dtype, missing_ratio, distinct_count, sample_values });
    }
    cards
}

// ---------------------------------------------------------------------
// 16. Column Relationships (A.2: never "correlation matrix" — pairwise
// numeric coefficients, no interpretive language attached)
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct PairwiseCoefficient {
    pub column_a: String,
    pub column_b: String,
    pub pearson_coefficient: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ColumnRelationships {
    pub pairwise_numeric_coefficients: Vec<PairwiseCoefficient>,
}

pub fn column_relationships(
    lf: &LazyFrame,
    schema: &Schema,
    exactness: Exactness,
    sink: &mut MetricSink,
) -> Result<ColumnRelationships, InspectionError> {
    let label = if exactness == Exactness::Sampled { TrustLabel::Sampled } else { TrustLabel::Derived };
    let numeric_cols: Vec<String> = schema.iter().filter(|(_, dt)| dt.is_numeric()).map(|(n, _)| n.to_string()).collect();
    let mut pairwise_numeric_coefficients = Vec::new();
    for i in 0..numeric_cols.len() {
        for j in (i + 1)..numeric_cols.len() {
            let a = &numeric_cols[i];
            let b = &numeric_cols[j];
            let out = lf.clone().select([pearson_corr(col(a), col(b), 1).alias("__corr")]).collect()?;
            let coeff = out.column("__corr")?.f64()?.get(0).unwrap_or(f64::NAN);
            sink.emit("column_relationships.pairwise_numeric_coefficient", label, (a, b, coeff));
            pairwise_numeric_coefficients.push(PairwiseCoefficient { column_a: a.clone(), column_b: b.clone(), pearson_coefficient: coeff });
        }
    }
    Ok(ColumnRelationships { pairwise_numeric_coefficients })
}

// ---------------------------------------------------------------------
// 17. Distribution Views (data underlying charts, not the charts)
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct HistogramBin {
    pub range_start: f64,
    pub range_end: f64,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnHistogram {
    pub column: String,
    pub bins: Vec<HistogramBin>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DistributionViews {
    pub per_column: Vec<ColumnHistogram>,
}

const HISTOGRAM_BIN_COUNT: usize = 10;

pub fn distribution_views(
    df: &DataFrame,
    schema: &Schema,
    exactness: Exactness,
    sink: &mut MetricSink,
) -> Result<DistributionViews, InspectionError> {
    let label = if exactness == Exactness::Sampled { TrustLabel::Sampled } else { TrustLabel::Derived };
    let mut per_column = Vec::new();
    for (name, dtype) in schema.iter() {
        if !dtype.is_numeric() {
            continue;
        }
        let name = name.as_str();
        let ca = df.column(name)?.cast(&DataType::Float64)?;
        let values: Vec<f64> = ca.f64()?.into_iter().flatten().collect();
        if values.is_empty() {
            continue;
        }
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let width = if (max - min).abs() < f64::EPSILON { 1.0 } else { (max - min) / HISTOGRAM_BIN_COUNT as f64 };

        let mut bins: Vec<HistogramBin> = (0..HISTOGRAM_BIN_COUNT)
            .map(|i| HistogramBin { range_start: min + i as f64 * width, range_end: min + (i + 1) as f64 * width, count: 0 })
            .collect();
        for &v in &values {
            let mut idx = ((v - min) / width).floor() as isize;
            if idx < 0 {
                idx = 0;
            }
            if idx as usize >= HISTOGRAM_BIN_COUNT {
                idx = HISTOGRAM_BIN_COUNT as isize - 1;
            }
            bins[idx as usize].count += 1;
        }
        sink.emit("distribution_views.histogram_bin", label, (name, bins.len()));
        per_column.push(ColumnHistogram { column: name.to_string(), bins });
    }
    Ok(DistributionViews { per_column })
}

// ---------------------------------------------------------------------
// 18. Data Checks (A.2: never "Issue Count" — Detected Condition Count.
// Fixed, non-configurable structural checks — not a Validation Ruleset,
// INV-012 still governs Validation exclusively for rule-driven checks.)
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DataCheckResult {
    pub check_name: String,
    pub detected_condition_count: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DataChecks {
    pub checks: Vec<DataCheckResult>,
    pub total_detected_condition_count: u64,
}

pub fn data_checks(
    completeness: &Completeness,
    duplicates: &DuplicatesAndMatches,
    whitespace: &WhitespaceAndHiddenCharacters,
    row_profile: &RowProfile,
    thresholds: &ThresholdRegistry,
    sink: &mut MetricSink,
) -> DataChecks {
    let high_missingness_condition_count = completeness
        .completeness_map
        .iter()
        .filter(|c| c.missing_ratio >= thresholds.high_missingness_min_ratio)
        .count() as u64;
    let whitespace_condition_count: u64 = whitespace
        .character_condition_table
        .iter()
        .map(|c| c.leading_or_trailing_whitespace_count)
        .sum();

    let checks = vec![
        DataCheckResult { check_name: "high_missingness_columns".to_string(), detected_condition_count: high_missingness_condition_count },
        DataCheckResult { check_name: "exact_duplicate_rows".to_string(), detected_condition_count: duplicates.exact_duplicate_row_count },
        DataCheckResult { check_name: "leading_or_trailing_whitespace_values".to_string(), detected_condition_count: whitespace_condition_count },
        DataCheckResult { check_name: "rows_flagged_high_missingness".to_string(), detected_condition_count: row_profile.rows_flagged_high_missingness },
    ];
    let total_detected_condition_count = checks.iter().map(|c| c.detected_condition_count).sum();
    for c in &checks {
        sink.emit("data_checks.detected_condition_count", TrustLabel::Derived, (&c.check_name, c.detected_condition_count));
    }
    DataChecks { checks, total_detected_condition_count }
}

// ---------------------------------------------------------------------
// 19. Metadata Coverage
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct MetadataCoverage {
    pub columns_with_type_metadata: u64,
    pub total_columns: u64,
}

pub fn metadata_coverage(data_types: &DataTypes, sink: &mut MetricSink) -> MetadataCoverage {
    let total_columns = data_types.per_column.len() as u64;
    let columns_with_type_metadata = data_types.per_column.iter().filter(|(_, d)| !d.is_empty()).count() as u64;
    sink.emit("metadata_coverage.columns_with_type_metadata", TrustLabel::Metadata, columns_with_type_metadata);
    MetadataCoverage { columns_with_type_metadata, total_columns }
}

// ---------------------------------------------------------------------
// 20. Evidence Trail
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceTrail {
    pub table_id: TableId,
    pub content_fingerprint: String,
    pub inspected_at: Timestamp,
    pub engine_version: String,
}

pub fn evidence_trail(
    table_id: TableId,
    content_fingerprint: &str,
    inspected_at: Timestamp,
    engine_version: &str,
    sink: &mut MetricSink,
) -> EvidenceTrail {
    sink.emit("evidence_trail.inspected_at", TrustLabel::Metadata, inspected_at);
    sink.emit("evidence_trail.engine_version", TrustLabel::Metadata, engine_version);
    EvidenceTrail {
        table_id,
        content_fingerprint: content_fingerprint.to_string(),
        inspected_at,
        engine_version: engine_version.to_string(),
    }
}
