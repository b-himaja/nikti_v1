use chrono::NaiveDate;
use polars::prelude::*;
use uuid::Uuid;

use crate::engines::content::TableContent;
use crate::types::TrustLabel;

use super::sections::ImportSummary;
use super::threshold_registry::ThresholdRegistry;
use super::{inspect_standard, InspectionResults};

/// A realistic-looking but obviously-synthetic test fingerprint — never
/// the `"unfingerprinted:{table_id}"` shape the fabrication bug produced,
/// so a regression back to that fabrication would be caught by the
/// pass-through assertions below rather than accidentally matching this
/// fixture's own shape.
const TEST_FINGERPRINT: &str = "test-fp-3f9c2a";

fn content(df: DataFrame) -> TableContent {
    content_with_fingerprint(df, TEST_FINGERPRINT)
}

fn content_with_fingerprint(df: DataFrame, fingerprint: &str) -> TableContent {
    TableContent { table_id: Uuid::new_v4(), content_fingerprint: fingerprint.to_string(), frame: df.lazy() }
}

fn run(df: DataFrame) -> InspectionResults {
    inspect_standard(content(df), ImportSummary::default(), &ThresholdRegistry::v1(), "0.1.0", 1_000).unwrap()
}

fn run_with_thresholds(df: DataFrame, thresholds: &ThresholdRegistry) -> InspectionResults {
    inspect_standard(content(df), ImportSummary::default(), thresholds, "0.1.0", 1_000).unwrap()
}

fn base_fixture() -> DataFrame {
    df![
        "id" => [1i64, 2, 3, 4, 5],
        "amount" => [Some(10.0), Some(20.0), Some(30.0), Some(40.0), None],
        "category" => ["A", "A", "B", "B", "C"],
        "note" => [" hi ", "hello", "world", "test", "  spaced  "],
        "signup_date" => [
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 3).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 4).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 5).unwrap(),
        ],
    ]
    .unwrap()
}

// 1. Dataset Identity
#[test]
fn dataset_identity_reports_exact_row_and_column_counts() {
    let out = run(base_fixture());
    assert_eq!(out.dataset_identity.row_count, 5);
    assert_eq!(out.dataset_identity.column_count, 5);
    assert_eq!(out.dataset_identity.content_fingerprint, TEST_FINGERPRINT, "must be the real fingerprint carried on TableContent, never a fabricated fallback");
}

/// The specific regression this replaces: `inspect_standard` used to
/// fabricate `format!("unfingerprinted:{table_id}")` internally whenever
/// no real fingerprint was available. `TableContent::content_fingerprint`
/// is now a required field, so there is no code path left inside
/// `inspect_standard` that could still produce that shape — this test
/// proves two independently-fingerprinted inputs produce two genuinely
/// different `DatasetIdentity`/`EvidenceTrail` values, which a fabricated
/// per-table_id string could still have accidentally satisfied.
#[test]
fn content_fingerprint_is_never_fabricated_and_differs_per_input() {
    let a = run(base_fixture());
    let out_b = inspect_standard(content_with_fingerprint(base_fixture(), "a-completely-different-fingerprint"), ImportSummary::default(), &ThresholdRegistry::v1(), "0.1.0", 1_000).unwrap();

    assert_eq!(a.dataset_identity.content_fingerprint, TEST_FINGERPRINT);
    assert_eq!(out_b.dataset_identity.content_fingerprint, "a-completely-different-fingerprint");
    assert_ne!(a.dataset_identity.content_fingerprint, out_b.dataset_identity.content_fingerprint);
    assert!(!a.dataset_identity.content_fingerprint.starts_with("unfingerprinted:"));
    assert_eq!(a.evidence_trail.content_fingerprint, TEST_FINGERPRINT);
}

// 2. Import Health
#[test]
fn import_health_reports_the_supplied_import_summary_verbatim() {
    let summary = ImportSummary { rows_read: 5, rows_rejected: 1, parse_error_count: 2, encoding: Some("utf-8".to_string()) };
    let out = inspect_standard(content(base_fixture()), summary, &ThresholdRegistry::v1(), "0.1.0", 1_000).unwrap();
    assert_eq!(out.import_health.rows_read, 5);
    assert_eq!(out.import_health.rows_rejected, 1);
    assert_eq!(out.import_health.parse_error_count, 2);
    assert_eq!(out.import_health.encoding.as_deref(), Some("utf-8"));
}

// 3. Dataset Shape
#[test]
fn dataset_shape_reports_row_and_column_counts() {
    let out = run(base_fixture());
    assert_eq!(out.dataset_shape.row_count, 5);
    assert_eq!(out.dataset_shape.column_count, 5);
    assert!(out.dataset_shape.estimated_memory_bytes > 0);
}

// 4. Completeness
#[test]
fn completeness_reports_exact_missing_ratios() {
    let out = run(base_fixture());
    let amount = out.completeness.completeness_map.iter().find(|c| c.column == "amount").unwrap();
    assert!((amount.missing_ratio - 0.2).abs() < 1e-9, "1 null / 5 rows");
    // 1 null cell out of 5 rows * 5 columns = 25 cells.
    assert!((out.completeness.overall_missing_ratio - (1.0 / 25.0)).abs() < 1e-9);
}

// 5. Data Types
#[test]
fn data_types_reports_dtype_per_column() {
    let out = run(base_fixture());
    let dtype_of = |name: &str| out.data_types.per_column.iter().find(|(c, _)| c == name).unwrap().1.clone();
    assert_eq!(dtype_of("id"), "Int64");
    assert_eq!(dtype_of("category"), "String");
}

// 6. Values & Cardinality
#[test]
fn values_and_cardinality_flags_100pct_uniqueness_column() {
    let out = run(base_fixture());
    assert!(out.values_and_cardinality.columns_with_100pct_uniqueness.contains(&"id".to_string()));
    assert!(!out.values_and_cardinality.columns_with_100pct_uniqueness.contains(&"category".to_string()));
    let category = out.values_and_cardinality.per_column.iter().find(|c| c.column == "category").unwrap();
    assert_eq!(category.distinct_count, 3);
    assert!((category.distinct_ratio - 0.6).abs() < 1e-9);
}

// 7. Duplicates & Matches
#[test]
fn duplicates_and_matches_detects_exact_duplicate_rows() {
    let df = df!["a" => [1i64, 1, 2], "b" => ["x", "x", "y"]].unwrap();
    let out = run(df);
    assert_eq!(out.duplicates_and_matches.exact_duplicate_row_count, 2);
    assert!((out.duplicates_and_matches.duplicate_row_ratio - (2.0 / 3.0)).abs() < 1e-9);
}

// 8. Number Profile
#[test]
fn number_profile_reports_exact_min_max_mean_quartiles() {
    let out = run(base_fixture());
    let amount = out.number_profile.per_column.iter().find(|c| c.column == "amount").unwrap();
    assert_eq!(amount.min, 10.0);
    assert_eq!(amount.max, 40.0);
    assert_eq!(amount.mean, 25.0);
    assert!((amount.quartiles.q1 - 17.5).abs() < 1e-9);
    assert!((amount.quartiles.q2_median - 25.0).abs() < 1e-9);
    assert!((amount.quartiles.q3 - 32.5).abs() < 1e-9);
    assert!((amount.quartiles.iqr - 15.0).abs() < 1e-9);
}

// 9. Category Profile
#[test]
fn category_profile_reports_top_values_and_counts() {
    let out = run(base_fixture());
    let category = out.category_profile.per_column.iter().find(|c| c.column == "category").unwrap();
    assert_eq!(category.top_values.first(), Some(&("A".to_string(), 2)));
    assert!(category.top_values.contains(&("B".to_string(), 2)));
    assert!(category.top_values.contains(&("C".to_string(), 1)));
}

// 10. Date & Time Profile
#[test]
fn date_time_profile_reports_min_max_distinct() {
    let out = run(base_fixture());
    let signup = out.date_time_profile.per_column.iter().find(|c| c.column == "signup_date").unwrap();
    assert_eq!(signup.min, "2024-01-01");
    assert_eq!(signup.max, "2024-01-05");
    assert_eq!(signup.distinct_count, 5);
}

// 11. Text Profile
#[test]
fn text_profile_reports_length_stats() {
    let out = run(base_fixture());
    let note = out.text_profile.per_column.iter().find(|c| c.column == "note").unwrap();
    // lengths: " hi "=4, "hello"=5, "world"=5, "test"=4, "  spaced  "=10
    assert_eq!(note.min_length, 4);
    assert_eq!(note.max_length, 10);
    assert!((note.mean_length - (4.0 + 5.0 + 5.0 + 4.0 + 10.0) / 5.0).abs() < 1e-9);
}

// 12. Format Patterns
#[test]
fn format_patterns_reports_distinct_shapes_and_dominant_ratio() {
    let out = run(base_fixture());
    let note = out.format_patterns.per_column.iter().find(|c| c.column == "note").unwrap();
    // shapes: " aa ", "aaaaa", "aaaaa", "aaaa", "  aaaaaa  " -> 4 distinct, "aaaaa" dominant with count 2/5.
    assert_eq!(note.distinct_format_count, 4);
    assert!((note.dominant_format_ratio - 0.4).abs() < 1e-9);
}

// 13. Whitespace & Hidden Characters
#[test]
fn whitespace_and_hidden_characters_detects_leading_trailing_whitespace() {
    let out = run(base_fixture());
    let note = out.whitespace_and_hidden_characters.character_condition_table.iter().find(|c| c.column == "note").unwrap();
    assert_eq!(note.leading_or_trailing_whitespace_count, 2);
    assert_eq!(note.control_char_count, 0);
    assert_eq!(note.non_ascii_count, 0);
}

// 14. Row Profile
#[test]
fn row_profile_flags_high_missingness_rows() {
    let df = df![
        "a" => [Some(1i64), None, Some(3)],
        "b" => [Some(1i64), None, Some(3)],
        "c" => [Some(1i64), None, Some(3)],
    ]
    .unwrap();
    let out = run(df);
    // Row 1 (index 1) is entirely null: missing_ratio = 3/3 = 1.0 >= 0.5 default threshold.
    assert_eq!(out.row_profile.rows_flagged_high_missingness, 1);
    assert_eq!(out.row_profile.row_condition_index.len(), 1);
    assert_eq!(out.row_profile.row_condition_index[0].row_index, 1);
    assert_eq!(out.row_profile.row_condition_index[0].missing_field_count, 3);
}

// 15. Column Cards
#[test]
fn column_cards_combines_fields_from_other_sections() {
    let out = run(base_fixture());
    let id_card = out.column_cards.iter().find(|c| c.column == "id").unwrap();
    assert_eq!(id_card.dtype, "Int64");
    assert_eq!(id_card.distinct_count, 5);
    assert_eq!(id_card.missing_ratio, 0.0);
    assert_eq!(id_card.sample_values.len(), 3);
}

// 16. Column Relationships
#[test]
fn column_relationships_reports_pearson_coefficient() {
    let df = df!["x" => [1.0, 2.0, 3.0, 4.0, 5.0], "y" => [2.0, 4.0, 6.0, 8.0, 10.0]].unwrap();
    let out = run(df);
    assert_eq!(out.column_relationships.pairwise_numeric_coefficients.len(), 1);
    let pair = &out.column_relationships.pairwise_numeric_coefficients[0];
    assert!((pair.pearson_coefficient - 1.0).abs() < 1e-9, "y is a perfect positive linear function of x");
}

// 17. Distribution Views
#[test]
fn distribution_views_reports_histogram_bins_summing_to_row_count() {
    let df = df!["x" => [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]].unwrap();
    let out = run(df);
    let hist = out.distribution_views.per_column.iter().find(|c| c.column == "x").unwrap();
    assert_eq!(hist.bins.len(), 10);
    let total: u64 = hist.bins.iter().map(|b| b.count).sum();
    assert_eq!(total, 10, "every value falls into exactly one bin");
}

// 18. Data Checks
#[test]
fn data_checks_reports_detected_condition_counts() {
    let out = run(base_fixture());
    let whitespace_check = out.data_checks.checks.iter().find(|c| c.check_name == "leading_or_trailing_whitespace_values").unwrap();
    assert_eq!(whitespace_check.detected_condition_count, 2);
    assert!(out.data_checks.total_detected_condition_count >= 2);
}

// 19. Metadata Coverage
#[test]
fn metadata_coverage_reports_columns_with_type_metadata() {
    let out = run(base_fixture());
    assert_eq!(out.metadata_coverage.total_columns, 5);
    assert_eq!(out.metadata_coverage.columns_with_type_metadata, 5);
}

// 20. Evidence Trail
#[test]
fn evidence_trail_reports_table_id_and_engine_version() {
    let df = base_fixture();
    let c = content(df);
    let table_id = c.table_id;
    let out = inspect_standard(c, ImportSummary::default(), &ThresholdRegistry::v1(), "9.9.9", 42).unwrap();
    assert_eq!(out.evidence_trail.table_id, table_id);
    assert_eq!(out.evidence_trail.engine_version, "9.9.9");
    assert_eq!(out.evidence_trail.inspected_at, 42);
    assert_eq!(out.evidence_trail.content_fingerprint, TEST_FINGERPRINT, "must be the real fingerprint, never a fabricated fallback");
}

// --- Cross-cutting requirements ---

#[test]
fn inspection_never_mutates_the_source_frame() {
    let df = base_fixture();
    let c = content(df.clone());
    let before = c.frame.clone().collect().unwrap();
    let _ = inspect_standard(c.clone(), ImportSummary::default(), &ThresholdRegistry::v1(), "0.1.0", 1_000).unwrap();
    let after = c.frame.clone().collect().unwrap();
    assert_eq!(before, after, "Inspection must never alter the content it reads (INV-011/INV-002 downstream)");
}

const BANNED_WORDS: &[&str] = &["bad", "poor", "suspicious", "problem", "risk", "recommended", "quality score"];
const BANNED_CORRECTED_TERMS: &[&str] = &["candidate key", "correlation matrix", "box plot", "heatmap", "issue index", "issue table"];

#[test]
fn results_never_contain_banned_evaluative_or_superseded_vocabulary() {
    let out = run(base_fixture());
    let rendered = format!("{out:?}").to_lowercase();
    for word in BANNED_WORDS {
        assert!(!rendered.contains(word), "banned evaluative word '{word}' leaked into InspectionResults output");
    }
    for term in BANNED_CORRECTED_TERMS {
        assert!(!rendered.contains(term), "superseded term '{term}' leaked into InspectionResults output");
    }
}

#[test]
fn every_emitted_metric_has_a_metric_dictionary_entry() {
    let out = run(base_fixture());
    assert!(!out.emitted_metrics.is_empty());
    for metric in &out.emitted_metrics {
        assert!(
            super::metric_dictionary::contains(metric.key),
            "emitted metric key '{}' has no Metric Dictionary entry",
            metric.key
        );
    }
}

#[test]
fn threshold_registry_changes_grouping_without_a_code_change() {
    let df = df!["category" => ["A", "A", "B", "B", "C"]].unwrap();

    let permissive = ThresholdRegistry::v1();
    let out_permissive = run_with_thresholds(df.clone(), &permissive);
    let category_permissive = out_permissive.category_profile.per_column.iter().find(|c| c.column == "category").unwrap();
    assert_eq!(category_permissive.rare_value_count, 0, "default threshold (1%) treats a 20% share as not rare");

    let mut strict = ThresholdRegistry::v1();
    strict.version = 2;
    strict.rare_value_max_ratio = 0.5; // now anything at or below 50% share counts as rare
    let out_strict = run_with_thresholds(df, &strict);
    let category_strict = out_strict.category_profile.per_column.iter().find(|c| c.column == "category").unwrap();
    assert_eq!(
        category_strict.rare_value_count, 3,
        "same input, only the registry changed -> all 3 distinct values (<=50% share) now count as rare"
    );
}

#[test]
fn sampled_result_never_appears_tagged_full_data_and_carries_disclosure_fields() {
    let values: Vec<String> = (0..20).map(|i| format!("val{i}")).collect();
    let df = df!["category" => values].unwrap();

    let mut thresholds = ThresholdRegistry::v1();
    thresholds.sample_trigger_row_count = 5;
    thresholds.sample_size = 5;

    let out = run_with_thresholds(df, &thresholds);
    let disclosure = out.sampling.expect("row count exceeds the trigger threshold");
    assert_eq!(disclosure.full_row_count, 20);
    assert_eq!(disclosure.sampled_row_count, 5);
    assert_eq!(disclosure.sample_size, 5);
    assert_eq!(disclosure.seed, Some(0));
    assert!(!disclosure.affected_sections.is_empty());

    let affected_key_prefixes =
        ["category_profile.", "format_patterns.", "whitespace.", "character_condition_table.", "text_profile."];
    for metric in &out.emitted_metrics {
        if affected_key_prefixes.iter().any(|p| metric.key.starts_with(p)) {
            assert_eq!(
                metric.label,
                TrustLabel::Sampled,
                "metric '{}' is in a sampling-affected section and must be tagged Sampled, never Full Data or its unsampled label",
                metric.key
            );
        }
    }
}

#[test]
fn sampling_is_deterministic_across_identical_runs() {
    let values: Vec<String> = (0..50).map(|i| format!("val{i}")).collect();
    let mut thresholds = ThresholdRegistry::v1();
    thresholds.sample_trigger_row_count = 10;
    thresholds.sample_size = 10;

    let df1 = df!["category" => values.clone()].unwrap();
    let df2 = df!["category" => values].unwrap();
    let out1 = run_with_thresholds(df1, &thresholds);
    let out2 = run_with_thresholds(df2, &thresholds);
    assert_eq!(
        out1.category_profile.per_column[0].top_values,
        out2.category_profile.per_column[0].top_values,
        "seed=0 sampling must be reproducible across identical runs"
    );
}
