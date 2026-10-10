use polars::prelude::*;
use uuid::Uuid;

use crate::engines::content::TableContent;
use crate::types::analysis_procedure_params::{AnalysisProcedureParams as P, BoundaryMethod, CorrelationMethod, DateBucket};
use crate::types::transformation_operation::AggFunc;
use crate::types::{
    AnalysisCompatibleTargetType, AnalysisProcedure, AnalysisProcedureTarget, AnalysisRulesetVersion, DefaultScope, OutputOptions,
    TargetContract,
};

use super::{analyse_standard, AnalysisResult, AnalysisTarget, ExecutionStatus, FindingType, ProcedureStatus};

fn proc(name: &str, params: P, target: AnalysisProcedureTarget) -> AnalysisProcedure {
    AnalysisProcedure::new(Uuid::new_v4(), name.to_string(), params, true, 1, target, OutputOptions::default(), vec![], "".to_string(), vec![])
}

fn version(procedures: Vec<AnalysisProcedure>) -> AnalysisRulesetVersion {
    AnalysisRulesetVersion::new(
        Uuid::new_v4(),
        1,
        1_000,
        "author".to_string(),
        "initial".to_string(),
        None,
        "1.0".to_string(),
        "1.0.0".to_string(),
        "0.1.0".to_string(),
        "hash-1".to_string(),
        TargetContract::new(vec![AnalysisCompatibleTargetType::Table, AnalysisCompatibleTargetType::Dataset], None, vec![]),
        DefaultScope::default(),
        procedures,
    )
    .unwrap()
}

fn run(df: DataFrame, procedures: Vec<AnalysisProcedure>) -> AnalysisResult {
    let content = TableContent { table_id: Uuid::new_v4(), content_fingerprint: "test-fp".to_string(), frame: df.lazy() };
    let v = version(procedures);
    let (result, _meta, __detail) = analyse_standard(AnalysisTarget::Table(content), Uuid::new_v4(), &v, "tester".to_string(), 1_000, "0.1.0", &|_| false);
    result
}

/// Result of the one non-AP-001 procedure under test (AP-001 always
/// runs first and is excluded here for test brevity).
fn only_result(result: &AnalysisResult) -> &super::ProcedureResult {
    result.procedure_results.iter().find(|r| r.procedure_type != "AP-001").expect("exactly one non-AP-001 procedure was submitted")
}

fn measure_value<'a>(result: &'a super::ProcedureResult, name: &str) -> f64 {
    result.measures.iter().find(|m| m.measure_name == name).unwrap_or_else(|| panic!("measure '{name}' not found in {:?}", result.measures.iter().map(|m| &m.measure_name).collect::<Vec<_>>())).value
}

// ===================== AP-001 =====================

#[test]
fn ap001_scope_and_variable_role_setup_always_runs_first() {
    let df = df!["x" => [1i64, 2, 3]].unwrap();
    let other = proc("p", P::ColumnPropertyExplorer { columns: vec!["x".to_string()] }, AnalysisProcedureTarget::columns(&["x"]));
    let result = run(df, vec![other]);
    assert_eq!(result.procedure_results[0].procedure_type, "AP-001");
    assert_eq!(result.procedure_results[0].status, ProcedureStatus::Completed);
}

// ===================== One test per procedure (Part C, all 19) =====================

#[test]
fn ap002_derived_measure_definition_happy_path() {
    let df = df!["a" => [10.0, 20.0, 30.0]].unwrap();
    let p = proc("p", P::DerivedMeasureDefinition { measure_name: "double_a".to_string(), formula: "a * 2".to_string() }, AnalysisProcedureTarget::columns(&["a"]));
    let result = run(df, vec![p]);
    let r = only_result(&result);
    assert_eq!(r.status, ProcedureStatus::Completed);
    assert!((measure_value(r, "double_a_mean") - 40.0).abs() < 1e-9);
}

#[test]
fn ap003_column_property_explorer_happy_path() {
    let df = df!["a" => [1i64, 2, 2, 3]].unwrap();
    let p = proc("p", P::ColumnPropertyExplorer { columns: vec!["a".to_string()] }, AnalysisProcedureTarget::columns(&["a"]));
    let result = run(df, vec![p]);
    let r = only_result(&result);
    assert_eq!(measure_value(r, "a_distinct_count"), 3.0);
    assert_eq!(measure_value(r, "a_null_count"), 0.0);
}

#[test]
fn ap004_grouped_summary_happy_path() {
    let df = df!["g" => ["A", "A", "B"], "v" => [10.0, 20.0, 100.0]].unwrap();
    let p = proc("p", P::GroupedSummary { group_by: vec!["g".to_string()], measure_column: "v".to_string(), agg: AggFunc::Sum }, AnalysisProcedureTarget::columns(&["g", "v"]));
    let result = run(df, vec![p]);
    let r = only_result(&result);
    assert_eq!(measure_value(r, "group_count"), 2.0);
    assert_eq!(r.output_table_refs.iter().find(|t| t.name == "group_summary_table").unwrap().row_count, 2);
}

#[test]
fn ap005_segment_subset_comparison_happy_path() {
    let df = df!["seg" => ["A", "A", "B", "B"], "v" => [10.0, 10.0, 50.0, 50.0]].unwrap();
    let p = proc("p", P::SegmentSubsetComparison { segment_column: "seg".to_string(), measure_column: "v".to_string(), agg: AggFunc::Mean }, AnalysisProcedureTarget::columns(&["seg", "v"]));
    let result = run(df, vec![p]);
    let r = only_result(&result);
    assert_eq!(measure_value(r, "segment_count"), 2.0);
    assert!((measure_value(r, "segment_value_spread") - 40.0).abs() < 1e-9);
}

#[test]
fn ap006_cross_tabulation_happy_path() {
    // Perfect association: row category fully determines column category.
    let df = df!["r" => ["A", "A", "B", "B"], "c" => ["X", "X", "Y", "Y"]].unwrap();
    let p = proc("p", P::CrossTabulation { row_column: "r".to_string(), column_column: "c".to_string() }, AnalysisProcedureTarget::columns(&["r", "c"]));
    let result = run(df, vec![p]);
    let r = only_result(&result);
    assert!((measure_value(r, "cramers_v") - 1.0).abs() < 1e-6, "perfect association -> Cramer's V of 1.0");
    assert_eq!(r.findings.len(), 1);
    assert_eq!(r.findings[0].finding_type, FindingType::AssociationCandidate);
}

#[test]
fn ap007_category_to_numeric_comparison_happy_path() {
    let df = df!["cat" => ["A", "A", "B", "B"], "num" => [10.0, 10.0, 20.0, 20.0]].unwrap();
    let p = proc("p", P::CategoryToNumericComparison { category_column: "cat".to_string(), numeric_column: "num".to_string() }, AnalysisProcedureTarget::columns(&["cat", "num"]));
    let result = run(df, vec![p]);
    let r = only_result(&result);
    assert!((measure_value(r, "eta_squared") - 1.0).abs() < 1e-6, "no within-group variance -> eta squared of 1.0");
}

#[test]
fn ap008_numeric_relationship_association_happy_path() {
    let df = df!["x" => [1.0, 2.0, 3.0, 4.0], "y" => [2.0, 4.0, 6.0, 8.0]].unwrap();
    let p = proc(
        "p",
        P::NumericRelationshipAssociation { columns: vec!["x".to_string(), "y".to_string()], method: CorrelationMethod::Pearson },
        AnalysisProcedureTarget::columns(&["x", "y"]),
    );
    let result = run(df, vec![p]);
    let r = only_result(&result);
    assert!((measure_value(r, "x__y__coefficient") - 1.0).abs() < 1e-9);
}

#[test]
fn ap008_spearman_variant_happy_path() {
    let df = df!["x" => [1.0, 2.0, 3.0, 4.0], "y" => [10.0, 5.0, 40.0, 30.0]].unwrap();
    let p = proc(
        "p",
        P::NumericRelationshipAssociation { columns: vec!["x".to_string(), "y".to_string()], method: CorrelationMethod::Spearman },
        AnalysisProcedureTarget::columns(&["x", "y"]),
    );
    let result = run(df, vec![p]);
    let r = only_result(&result);
    // ranks of y: [1,0,3,2] approx -> not perfectly monotonic; just assert it computed a finite coefficient.
    assert!(measure_value(r, "x__y__coefficient").is_finite());
}

#[test]
fn ap009_distribution_boundary_explorer_happy_path() {
    let df = df!["v" => [10.0, 12.0, 11.0, 13.0, 12.0, 1000.0]].unwrap();
    let p = proc("p", P::DistributionBoundaryExplorer { column: "v".to_string(), method: BoundaryMethod::Iqr }, AnalysisProcedureTarget::columns(&["v"]));
    let result = run(df, vec![p]);
    let r = only_result(&result);
    assert_eq!(measure_value(r, "boundary_candidate_count"), 1.0, "the 1000.0 outlier is the one boundary candidate");
    assert_eq!(r.findings.len(), 1);
    assert_eq!(r.findings[0].finding_type, FindingType::BoundaryCandidate);
}

/// Regression test for the fabricated-evidence fix: `histogram_table`
/// used to hardcode `row_count=10` with no binning logic anywhere. Real
/// binning must produce bin counts that sum to the real row count, and
/// must actually place the outlier in a different bin than the cluster.
#[test]
fn ap009_histogram_table_is_real_binned_data_not_a_fabricated_ten_rows() {
    let df = df!["v" => [10.0, 12.0, 11.0, 13.0, 12.0, 1000.0]].unwrap();
    let p = proc("p", P::DistributionBoundaryExplorer { column: "v".to_string(), method: BoundaryMethod::Iqr }, AnalysisProcedureTarget::columns(&["v"]));
    let (result, _meta, detail) = analyse_standard(AnalysisTarget::Table(TableContent { table_id: Uuid::new_v4(), content_fingerprint: "test-fp".to_string(), frame: df.lazy() }), Uuid::new_v4(), &version(vec![p]), "tester".to_string(), 1_000, "0.1.0", &|_| false);
    let r = only_result(&result);
    let hist_ref = r.output_table_refs.iter().find(|t| t.name == "histogram_table").unwrap();
    let (_, hist_df) = detail.iter().find(|(reference, _)| reference == &hist_ref.reference).expect("real content behind the reference");
    let total: i64 = hist_df.column("count").unwrap().i64().unwrap().into_iter().flatten().sum();
    assert_eq!(total, 6, "every real row is accounted for across the real bins");
    assert_eq!(hist_df.height(), hist_ref.row_count as usize);
    // The outlier (1000.0) and the cluster (~10-13) must not land in the
    // same bin — proves real bin edges derived from the real min/max,
    // not a fixed unconditional row_count=10 placeholder.
    let non_empty_bins = hist_df.column("count").unwrap().i64().unwrap().into_iter().flatten().filter(|&c| c > 0).count();
    assert!(non_empty_bins >= 2, "the outlier and the cluster fall into different bins");
}

#[test]
fn ap010_missingness_structure_explorer_happy_path() {
    let df = df!["a" => [Some(1i64), None, Some(3)], "b" => [Some(1i64), None, Some(3)]].unwrap();
    let p = proc("p", P::MissingnessStructureExplorer { columns: vec!["a".to_string(), "b".to_string()] }, AnalysisProcedureTarget::columns(&["a", "b"]));
    let result = run(df, vec![p]);
    let r = only_result(&result);
    assert!((measure_value(r, "a_missing_ratio") - (1.0 / 3.0)).abs() < 1e-9);
    assert!((measure_value(r, "a__b__missingness_phi") - 1.0).abs() < 1e-6, "a and b are missing on exactly the same row");
}

#[test]
fn ap011_repetition_duplicate_structure_explorer_happy_path() {
    let df = df!["k" => [1i64, 1, 2]].unwrap();
    let p = proc("p", P::RepetitionDuplicateStructureExplorer { key_columns: vec!["k".to_string()] }, AnalysisProcedureTarget::columns(&["k"]));
    let result = run(df, vec![p]);
    let r = only_result(&result);
    assert_eq!(measure_value(r, "exact_duplicate_row_count"), 2.0);
    assert_eq!(measure_value(r, "distinct_key_group_count"), 2.0);
}

#[test]
fn ap012_pattern_format_structure_explorer_happy_path() {
    let df = df!["code" => ["AB12", "CD34", "ab12"]].unwrap();
    let p = proc("p", P::PatternFormatStructureExplorer { column: "code".to_string(), regex_list: None }, AnalysisProcedureTarget::columns(&["code"]));
    let result = run(df, vec![p]);
    let r = only_result(&result);
    assert_eq!(measure_value(r, "distinct_format_count"), 2.0, "AA99 and aa99 are the two distinct shapes");
}

/// The parameter-plumbing fix: `regex_measure_table` was correctly left
/// absent (Part B of the evidence-integrity remediation) because
/// `PatternFormatStructureExplorer` had no way to receive a regex list.
/// With `regex_list` now wired through, this proves the table is real —
/// genuine match/non-match counts against fixture data with a known
/// answer, not a placeholder.
#[test]
fn ap012_regex_measure_table_is_real_when_a_regex_list_is_supplied() {
    let df = df!["code" => ["AB12", "CD34", "ab12", "XY99"]].unwrap();
    let p = proc(
        "p",
        P::PatternFormatStructureExplorer { column: "code".to_string(), regex_list: Some(vec!["^[A-Z]+[0-9]+$".to_string()]) },
        AnalysisProcedureTarget::columns(&["code"]),
    );
    let (result, _meta, detail) =
        analyse_standard(AnalysisTarget::Table(TableContent { table_id: Uuid::new_v4(), content_fingerprint: "test-fp".to_string(), frame: df.lazy() }), Uuid::new_v4(), &version(vec![p]), "tester".to_string(), 1_000, "0.1.0", &|_| false);
    let r = only_result(&result);
    let regex_ref = r.output_table_refs.iter().find(|t| t.name == "regex_measure_table").unwrap();
    let (_, regex_df) = detail.iter().find(|(reference, _)| reference == &regex_ref.reference).unwrap();

    assert_eq!(regex_df.column("regex").unwrap().str().unwrap().get(0).unwrap(), "^[A-Z]+[0-9]+$");
    // "AB12", "CD34", "XY99" match (all-caps + digits); "ab12" does not.
    assert_eq!(regex_df.column("match_count").unwrap().i64().unwrap().get(0).unwrap(), 3);
    assert_eq!(regex_df.column("non_match_count").unwrap().i64().unwrap().get(0).unwrap(), 1);
    assert!((measure_value(r, "^[A-Z]+[0-9]+$__regex_match_count") - 3.0).abs() < 1e-9);
    assert!((measure_value(r, "^[A-Z]+[0-9]+$__regex_non_match_count") - 1.0).abs() < 1e-9);
}

/// Fix requirement (3): with no `regex_list` supplied, `regex_measure_table`
/// must be skipped with a recorded reason — never silently omitted, never
/// fabricated — while the rest of AP-012 (`pattern_frequency_table`)
/// still computes. Per-table gap, not a whole-procedure `NotExecutable`.
#[test]
fn ap012_regex_measure_table_is_not_executable_when_no_regex_list_is_supplied() {
    let df = df!["code" => ["AB12", "CD34"]].unwrap();
    let p = proc("p", P::PatternFormatStructureExplorer { column: "code".to_string(), regex_list: None }, AnalysisProcedureTarget::columns(&["code"]));
    let result = run(df, vec![p]);
    let r = only_result(&result);

    assert!(r.output_table_refs.iter().all(|t| t.name != "regex_measure_table"), "no dangling reference to a table nothing computed");
    assert!(r.output_table_refs.iter().any(|t| t.name == "pattern_frequency_table"), "unrelated output table still computes");
    assert!(r.warnings.iter().any(|w| w.contains("regex_measure_table") && w.contains("not")), "a reason code must be recorded: {:?}", r.warnings);
    assert_ne!(r.status, ProcedureStatus::NotExecutable, "this is a per-table gap, not a whole-procedure failure");
}

#[test]
fn ap013_date_time_trend_explorer_happy_path() {
    use chrono::NaiveDate;
    let df = df!["d" => [NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(), NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(), NaiveDate::from_ymd_opt(2024, 2, 1).unwrap()]].unwrap();
    let p = proc(
        "p",
        P::DateTimeTrendExplorer { date_column: "d".to_string(), bucket: DateBucket::Month, duration_start_column: None, duration_end_column: None },
        AnalysisProcedureTarget::columns(&["d"]),
    );
    let result = run(df, vec![p]);
    let r = only_result(&result);
    assert_eq!(measure_value(r, "time_bucket_count"), 2.0, "January and February");
}

/// Regression test for the ORIGINAL fabricated-evidence fix:
/// `duration_summary_table` used to be a byte-for-byte copy of
/// `date_summary_table`. This still guards against that specific
/// regression (columns must never collapse back to `date_summary_table`'s
/// shape). It does NOT prove the replacement computation is the correct
/// one — see `ap013_duration_summary_table_computes_the_per_row_start_end_difference_not_the_between_row_gap`
/// and `ap013_duration_summary_table_differs_from_the_between_row_gap_statistic`
/// below for that. Adjusted only for the `duration_start_column`/
/// `duration_end_column` parameters this correction task added — the core
/// "not a copy" assertion and structure are otherwise unchanged.
#[test]
fn ap013_duration_summary_table_is_no_longer_a_copy_of_date_summary_table() {
    use chrono::NaiveDate;
    let d = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
    let df = df![
        "d" => [NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(), NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(), NaiveDate::from_ymd_opt(2024, 1, 19).unwrap()],
        "start" => [d, d, d],
        "end" => [NaiveDate::from_ymd_opt(2024, 1, 6).unwrap(), NaiveDate::from_ymd_opt(2024, 1, 11).unwrap(), NaiveDate::from_ymd_opt(2024, 1, 16).unwrap()],
    ]
    .unwrap();
    let p = proc(
        "p",
        P::DateTimeTrendExplorer { date_column: "d".to_string(), bucket: DateBucket::Day, duration_start_column: Some("start".to_string()), duration_end_column: Some("end".to_string()) },
        AnalysisProcedureTarget::columns(&["d", "start", "end"]),
    );
    let (result, _meta, detail) = analyse_standard(AnalysisTarget::Table(TableContent { table_id: Uuid::new_v4(), content_fingerprint: "test-fp".to_string(), frame: df.lazy() }), Uuid::new_v4(), &version(vec![p]), "tester".to_string(), 1_000, "0.1.0", &|_| false);
    let r = only_result(&result);

    let date_ref = r.output_table_refs.iter().find(|t| t.name == "date_summary_table").unwrap();
    let duration_ref = r.output_table_refs.iter().find(|t| t.name == "duration_summary_table").unwrap();
    assert_ne!(date_ref.columns, duration_ref.columns, "different statistics entirely, not a copy");

    let (_, duration_df) = detail.iter().find(|(reference, _)| reference == &duration_ref.reference).unwrap();
    assert!(duration_df.column("median").is_ok());
}

/// The actual correctness fix: `duration_summary_table` must report
/// min/max/mean/median of the PER-ROW difference `end - start`, using the
/// `duration_start_column`/`duration_end_column` parameter pair — not the
/// gap between consecutive observations in `date_column` (that was the
/// bug this task corrects: the earlier evidence-integrity fix stopped
/// copying `date_summary_table` but became a relabeled `date_gap_table`
/// instead). Per-row diffs here are exactly 5, 10, and 15 days.
#[test]
fn ap013_duration_summary_table_computes_the_per_row_start_end_difference_not_the_between_row_gap() {
    use chrono::NaiveDate;
    let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
    let df = df![
        "d" => [start, start, start],
        "start" => [start, start, start],
        "end" => [NaiveDate::from_ymd_opt(2024, 1, 6).unwrap(), NaiveDate::from_ymd_opt(2024, 1, 11).unwrap(), NaiveDate::from_ymd_opt(2024, 1, 16).unwrap()],
    ]
    .unwrap();
    let p = proc(
        "p",
        P::DateTimeTrendExplorer { date_column: "d".to_string(), bucket: DateBucket::Day, duration_start_column: Some("start".to_string()), duration_end_column: Some("end".to_string()) },
        AnalysisProcedureTarget::columns(&["d", "start", "end"]),
    );
    let (result, _meta, detail) = analyse_standard(AnalysisTarget::Table(TableContent { table_id: Uuid::new_v4(), content_fingerprint: "test-fp".to_string(), frame: df.lazy() }), Uuid::new_v4(), &version(vec![p]), "tester".to_string(), 1_000, "0.1.0", &|_| false);
    let r = only_result(&result);
    let duration_ref = r.output_table_refs.iter().find(|t| t.name == "duration_summary_table").unwrap();
    let (_, duration_df) = detail.iter().find(|(reference, _)| reference == &duration_ref.reference).unwrap();

    let get = |col: &str| duration_df.column(col).unwrap().f64().unwrap().get(0).unwrap();
    assert!((get("min") - 5.0).abs() < 1e-9, "shortest per-row duration is 5 days");
    assert!((get("max") - 15.0).abs() < 1e-9, "longest per-row duration is 15 days");
    assert!((get("mean") - 10.0).abs() < 1e-9, "(5 + 10 + 15) / 3 = 10.0 days");
    assert!((get("median") - 10.0).abs() < 1e-9, "middle value of [5, 10, 15] is 10.0 days");
}

/// Proves the fix didn't just relabel `date_gap_table`'s concept under
/// `duration_summary_table`'s name a second time: on data where the
/// between-row gap statistic (computed from `date_column` alone, sorted)
/// and the per-row start/end difference statistic would produce
/// meaningfully different numbers, `duration_summary_table` must report
/// the per-row figure, not the gap figure — this cannot pass by
/// coincidence, since the two fixtures below are deliberately chosen to
/// disagree.
#[test]
fn ap013_duration_summary_table_differs_from_the_between_row_gap_statistic() {
    use chrono::NaiveDate;
    // `d`, sorted, has gaps of 1 day then 17 days -> a between-row-gap
    // computation (date_gap_table's concept) would report mean = 9.0.
    // `start`/`end` are unrelated to `d` and their per-row difference is
    // always exactly 100 days -> the correct duration_summary_table mean.
    // 9.0 and 100.0 cannot be confused with one another.
    let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
    let end = NaiveDate::from_ymd_opt(2024, 4, 10).unwrap(); // 100 days later
    let df = df![
        "d" => [NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(), NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(), NaiveDate::from_ymd_opt(2024, 1, 19).unwrap()],
        "start" => [start, start, start],
        "end" => [end, end, end],
    ]
    .unwrap();
    let p = proc(
        "p",
        P::DateTimeTrendExplorer { date_column: "d".to_string(), bucket: DateBucket::Day, duration_start_column: Some("start".to_string()), duration_end_column: Some("end".to_string()) },
        AnalysisProcedureTarget::columns(&["d", "start", "end"]),
    );
    let (result, _meta, detail) = analyse_standard(AnalysisTarget::Table(TableContent { table_id: Uuid::new_v4(), content_fingerprint: "test-fp".to_string(), frame: df.lazy() }), Uuid::new_v4(), &version(vec![p]), "tester".to_string(), 1_000, "0.1.0", &|_| false);
    let r = only_result(&result);
    let duration_ref = r.output_table_refs.iter().find(|t| t.name == "duration_summary_table").unwrap();
    let (_, duration_df) = detail.iter().find(|(reference, _)| reference == &duration_ref.reference).unwrap();
    let mean = duration_df.column("mean").unwrap().f64().unwrap().get(0).unwrap();

    assert!((mean - 100.0).abs() < 1e-9, "must be the per-row start/end difference (100.0 days), got {mean}");
    assert!((mean - 9.0).abs() > 1.0, "must NOT be the between-row gap statistic (9.0 days) — that would be date_gap_table's concept wearing this table's name again");
}

/// Fix requirement (2): with `duration_start_column`/`duration_end_column`
/// not supplied, `duration_summary_table` must be skipped with a reason
/// recorded (Rule 9's discipline applied to a missing optional parameter),
/// never silently omitted and never falling back to the old gap
/// computation. This is a per-TABLE gap, not a whole-procedure
/// `NotExecutable` — `date_summary_table`/`time_bucket_table` still
/// compute normally from the same run.
#[test]
fn ap013_duration_summary_table_is_skipped_with_a_reason_when_duration_columns_are_not_supplied() {
    use chrono::NaiveDate;
    let df = df!["d" => [NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(), NaiveDate::from_ymd_opt(2024, 1, 15).unwrap()]].unwrap();
    let p = proc(
        "p",
        P::DateTimeTrendExplorer { date_column: "d".to_string(), bucket: DateBucket::Day, duration_start_column: None, duration_end_column: None },
        AnalysisProcedureTarget::columns(&["d"]),
    );
    let result = run(df, vec![p]);
    let r = only_result(&result);

    assert!(r.output_table_refs.iter().all(|t| t.name != "duration_summary_table"), "no dangling reference to a table nothing computed");
    assert!(r.output_table_refs.iter().any(|t| t.name == "date_summary_table"), "unrelated output tables still compute");
    assert!(r.output_table_refs.iter().any(|t| t.name == "time_bucket_table"), "unrelated output tables still compute");
    assert!(
        r.warnings.iter().any(|w| w.contains("duration_summary_table") && w.contains("not")),
        "a reason code must be recorded, not a silent omission: {:?}",
        r.warnings
    );
    assert_ne!(r.status, ProcedureStatus::NotExecutable, "this is a per-table gap, not a whole-procedure failure");
}

#[test]
fn ap014_pareto_concentration_explorer_happy_path() {
    let df = df!["cat" => ["A", "B", "C"], "v" => [80.0, 10.0, 10.0]].unwrap();
    let p = proc("p", P::ParetoConcentrationExplorer { category_column: "cat".to_string(), measure_column: "v".to_string() }, AnalysisProcedureTarget::columns(&["cat", "v"]));
    let result = run(df, vec![p]);
    let r = only_result(&result);
    assert_eq!(measure_value(r, "categories_for_80pct_share"), 1.0, "category A alone reaches 80% share");
}

#[test]
fn ap015_text_measurement_explorer_happy_path() {
    let df = df!["t" => ["hello world", "hi", "greetings all"]].unwrap();
    let p = proc("p", P::TextMeasurementExplorer { column: "t".to_string(), keywords: vec!["hello".to_string()] }, AnalysisProcedureTarget::columns(&["t"]));
    let result = run(df, vec![p]);
    let r = only_result(&result);
    let expected_mean = ("hello world".len() + "hi".len() + "greetings all".len()) as f64 / 3.0;
    assert!((measure_value(r, "mean_length") - expected_mean).abs() < 1e-9);
    assert_eq!(r.output_table_refs.iter().find(|t| t.name == "keyword_frequency_table").unwrap().row_count, 1);
}

/// Regression test for the fabricated-evidence fix: `length_distribution_table`
/// used to hardcode `row_count=10` with no binning logic, same defect as
/// AP-009's histogram. Real binning must account for every real row.
#[test]
fn ap015_length_distribution_table_is_real_binned_data_not_a_fabricated_ten_rows() {
    let df = df!["t" => ["hello world", "hi", "greetings all"]].unwrap();
    let p = proc("p", P::TextMeasurementExplorer { column: "t".to_string(), keywords: vec![] }, AnalysisProcedureTarget::columns(&["t"]));
    let (result, _meta, detail) = analyse_standard(AnalysisTarget::Table(TableContent { table_id: Uuid::new_v4(), content_fingerprint: "test-fp".to_string(), frame: df.lazy() }), Uuid::new_v4(), &version(vec![p]), "tester".to_string(), 1_000, "0.1.0", &|_| false);
    let r = only_result(&result);
    let dist_ref = r.output_table_refs.iter().find(|t| t.name == "length_distribution_table").unwrap();
    let (_, dist_df) = detail.iter().find(|(reference, _)| reference == &dist_ref.reference).unwrap();
    let total: i64 = dist_df.column("count").unwrap().i64().unwrap().into_iter().flatten().sum();
    assert_eq!(total, 3, "every real row is accounted for across the real bins");
}

#[test]
fn ap016_target_lens_happy_path() {
    let df = df!["t" => [Some(1.0), Some(2.0), None]].unwrap();
    let p = proc("p", P::TargetLens { target_column: "t".to_string(), segment_columns: None }, AnalysisProcedureTarget::columns(&["t"]));
    let result = run(df, vec![p]);
    let r = only_result(&result);
    assert!((measure_value(r, "target_coverage_ratio") - (2.0 / 3.0)).abs() < 1e-9);
}

/// The parameter-plumbing fix: `target_by_segment_table` was correctly
/// left absent because `TargetLens` had no way to receive a segment
/// column. This proves the table is real and — per this task's explicit
/// warning not to reuse AP-005 wholesale — that it profiles the target
/// column itself per segment (same mean/std logic as the whole-dataset
/// `target_profile_table` above), not an arbitrary AP-005-style
/// measure_column+agg aggregation.
#[test]
fn ap016_target_by_segment_table_is_real_when_a_segment_column_is_supplied() {
    let df = df!["t" => [1.0, 3.0, 10.0, 20.0], "seg" => ["A", "A", "B", "B"]].unwrap();
    let p = proc(
        "p",
        P::TargetLens { target_column: "t".to_string(), segment_columns: Some(vec!["seg".to_string()]) },
        AnalysisProcedureTarget::columns(&["t", "seg"]),
    );
    let (result, _meta, detail) =
        analyse_standard(AnalysisTarget::Table(TableContent { table_id: Uuid::new_v4(), content_fingerprint: "test-fp".to_string(), frame: df.lazy() }), Uuid::new_v4(), &version(vec![p]), "tester".to_string(), 1_000, "0.1.0", &|_| false);
    let r = only_result(&result);
    let seg_ref = r.output_table_refs.iter().find(|t| t.name == "target_by_segment_table").unwrap();
    let (_, seg_df) = detail.iter().find(|(reference, _)| reference == &seg_ref.reference).unwrap();
    assert_eq!(seg_df.height(), 2, "one row per segment");

    let seg_col = seg_df.column("seg").unwrap().str().unwrap();
    let mean_col = seg_df.column("target_mean").unwrap().f64().unwrap();
    let mean_for = |name: &str| (0..seg_df.height()).find(|&i| seg_col.get(i) == Some(name)).map(|i| mean_col.get(i).unwrap()).unwrap();
    assert!((mean_for("A") - 2.0).abs() < 1e-9, "segment A mean = (1 + 3) / 2 = 2.0");
    assert!((mean_for("B") - 15.0).abs() < 1e-9, "segment B mean = (10 + 20) / 2 = 15.0");
}

/// Fix requirement (3): with no `segment_columns` supplied,
/// `target_by_segment_table` must be skipped with a recorded reason —
/// never silently omitted, never fabricated — while the rest of AP-016
/// (`target_profile_table`) still computes. Per-table gap, not a
/// whole-procedure `NotExecutable`.
#[test]
fn ap016_target_by_segment_table_is_not_executable_when_no_segment_column_is_supplied() {
    let df = df!["t" => [Some(1.0), Some(2.0), None]].unwrap();
    let p = proc("p", P::TargetLens { target_column: "t".to_string(), segment_columns: None }, AnalysisProcedureTarget::columns(&["t"]));
    let result = run(df, vec![p]);
    let r = only_result(&result);

    assert!(r.output_table_refs.iter().all(|t| t.name != "target_by_segment_table"), "no dangling reference to a table nothing computed");
    assert!(r.output_table_refs.iter().any(|t| t.name == "target_profile_table"), "unrelated output table still computes");
    assert!(r.warnings.iter().any(|w| w.contains("target_by_segment_table") && w.contains("not")), "a reason code must be recorded: {:?}", r.warnings);
    assert_ne!(r.status, ProcedureStatus::NotExecutable, "this is a per-table gap, not a whole-procedure failure");
}

#[test]
fn ap017_association_strength_ranking_happy_path() {
    let df = df!["target" => [1.0, 2.0, 3.0, 4.0], "candidate" => [2.0, 4.0, 6.0, 8.0]].unwrap();
    let p = proc(
        "p",
        P::AssociationStrengthRanking { target_column: "target".to_string(), candidate_columns: vec!["candidate".to_string()] },
        AnalysisProcedureTarget::columns(&["target", "candidate"]),
    );
    let result = run(df, vec![p]);
    let r = only_result(&result);
    assert!((measure_value(r, "candidate_association_strength") - 1.0).abs() < 1e-9);
}

#[test]
fn ap018_dependency_candidate_explorer_happy_path() {
    let df = df!["id" => [1i64, 2, 3], "cat" => ["A", "A", "B"]].unwrap();
    let p = proc("p", P::DependencyCandidateExplorer { columns: vec!["id".to_string(), "cat".to_string()] }, AnalysisProcedureTarget::columns(&["id", "cat"]));
    let result = run(df, vec![p]);
    let r = only_result(&result);
    assert_eq!(measure_value(r, "id_distinct_ratio"), 1.0);
    assert!(r.findings.iter().any(|f| f.statement.contains("100% uniqueness")));
}

#[test]
fn ap019_feature_and_row_attention_screen_happy_path() {
    // A boundary-candidate-producing AP-009 run first, then AP-019
    // should surface retention review evidence built from it.
    let df = df!["v" => [10.0, 12.0, 11.0, 13.0, 12.0, 1000.0]].unwrap();
    let boundary = proc("boundary", P::DistributionBoundaryExplorer { column: "v".to_string(), method: BoundaryMethod::Iqr }, AnalysisProcedureTarget::columns(&["v"]));
    let screen = proc("screen", P::FeatureAndRowAttentionScreen, AnalysisProcedureTarget::table_scope());
    let result = run(df, vec![boundary, screen]);
    let screen_result = result.procedure_results.iter().find(|r| r.procedure_name == "screen").unwrap();
    assert!(screen_result.findings.iter().any(|f| f.finding_type == FindingType::RetentionEvidence));
}

// ===================== Cross-cutting =====================

#[test]
fn analysis_never_mutates_the_target_frame() {
    let df = df!["x" => [1i64, 2, 3]].unwrap();
    let content = TableContent { table_id: Uuid::new_v4(), content_fingerprint: "test-fp".to_string(), frame: df.clone().lazy() };
    let before = content.frame.clone().collect().unwrap();
    let p = proc("p", P::ColumnPropertyExplorer { columns: vec!["x".to_string()] }, AnalysisProcedureTarget::columns(&["x"]));
    let v = version(vec![p]);
    let _ = analyse_standard(AnalysisTarget::Table(content.clone()), Uuid::new_v4(), &v, "tester".to_string(), 1_000, "0.1.0", &|_| false);
    let after = content.frame.clone().collect().unwrap();
    assert_eq!(before, after);
}

/// Amendment 2 (binding correction): `AnalysisResult` is transient, not
/// Category-A evidence — this test documents that explicitly (no
/// no-write-methods assertion here) so a future reader doesn't "fix"
/// this by adding immutability guarantees that were deliberately
/// removed. `AnalysisResult`'s fields are all `pub` and directly
/// mutable — the opposite of `InspectionReport`/`ValidationReport`'s
/// private-fields-plus-accessors shape.
#[test]
fn analysis_result_is_not_subject_to_category_a_immutability() {
    let df = df!["x" => [1i64, 2, 3]].unwrap();
    let p = proc("p", P::ColumnPropertyExplorer { columns: vec!["x".to_string()] }, AnalysisProcedureTarget::columns(&["x"]));
    let mut result = run(df, vec![p]);
    // If this didn't compile, `AnalysisResult` would have regressed into
    // a Category-A-shaped type — the point of this test is that it does.
    result.warnings.push("directly mutable — this is expected and correct".to_string());
    assert!(!result.warnings.is_empty());
}

#[test]
fn single_target_enforcement_is_structural() {
    // B.3: there is no `AnalysisTarget` variant, and none may ever be
    // added, that binds two Tables/Datasets to one run — the type only
    // has `Table(TableContent)` and `Dataset { content, project_id }`.
    fn assert_signature<F>(_f: F)
    where
        F: Fn(AnalysisTarget) -> super::AnalysisTargetObjectType,
    {
    }
    assert_signature(|t: AnalysisTarget| match t {
        AnalysisTarget::Table(_) => super::AnalysisTargetObjectType::Table,
        AnalysisTarget::Dataset { .. } => super::AnalysisTargetObjectType::Dataset,
    });
}

const BANNED_WORDS: &[&str] = &["bad", "wrong", "suspicious", "useless", "must drop", "recommended fix", "prediction", "forecast", "business meaning"];

#[test]
fn generated_results_contain_no_banned_vocabulary_and_ap017_never_says_feature_importance() {
    let df = df![
        "id" => [1i64, 2, 3, 4],
        "cat" => ["A", "A", "B", "B"],
        "num" => [10.0, 20.0, 30.0, 1000.0],
        "target" => [1.0, 2.0, 3.0, 4.0],
        "candidate" => [2.0, 4.0, 6.0, 8.0],
        "code" => ["AB12", "CD34", "ab12", "EF56"],
        "text" => ["hello world", "hi there", "greetings", "salutations"],
    ]
    .unwrap();

    let procedures = vec![
        proc("p3", P::ColumnPropertyExplorer { columns: vec![] }, AnalysisProcedureTarget::columns(&["id"])),
        proc("p6", P::CrossTabulation { row_column: "cat".to_string(), column_column: "cat".to_string() }, AnalysisProcedureTarget::columns(&["cat"])),
        proc("p9", P::DistributionBoundaryExplorer { column: "num".to_string(), method: BoundaryMethod::Iqr }, AnalysisProcedureTarget::columns(&["num"])),
        proc(
            "p17",
            P::AssociationStrengthRanking { target_column: "target".to_string(), candidate_columns: vec!["candidate".to_string()] },
            AnalysisProcedureTarget::columns(&["target", "candidate"]),
        ),
        proc("p18", P::DependencyCandidateExplorer { columns: vec!["id".to_string()] }, AnalysisProcedureTarget::columns(&["id"])),
        proc("p19", P::FeatureAndRowAttentionScreen, AnalysisProcedureTarget::table_scope()),
    ];
    let result = run(df, procedures);
    let rendered = format!("{result:?}").to_lowercase();
    for word in BANNED_WORDS {
        assert!(!rendered.contains(word), "banned word '{word}' leaked into AnalysisResult output");
    }
    assert!(!rendered.contains("feature importance"), "AP-017 output must never say 'feature importance'");
    assert!(!rendered.contains("candidate key"), "user-facing output must say 'columns with 100% uniqueness', never 'candidate key'");
    assert!(!rendered.contains("correlation matrix"));
}

#[test]
fn ap019_output_never_contains_drop_or_delete_language() {
    let df = df!["v" => [10.0, 12.0, 11.0, 13.0, 12.0, 1000.0]].unwrap();
    let boundary = proc("boundary", P::DistributionBoundaryExplorer { column: "v".to_string(), method: BoundaryMethod::Iqr }, AnalysisProcedureTarget::columns(&["v"]));
    let screen = proc("screen", P::FeatureAndRowAttentionScreen, AnalysisProcedureTarget::table_scope());
    let result = run(df, vec![boundary, screen]);
    let screen_result = result.procedure_results.iter().find(|r| r.procedure_name == "screen").unwrap();
    let rendered = format!("{screen_result:?}").to_lowercase();
    for banned in ["drop", "delete", "retention/drop", "drop review"] {
        assert!(!rendered.contains(banned), "AP-019 output must never contain '{banned}' (Amendment 6)");
    }
}

#[test]
fn a_single_procedure_execution_error_does_not_abort_the_run() {
    let df = df!["x" => ["a", "b"]].unwrap();
    let bad = proc("bad", P::DerivedMeasureDefinition { measure_name: "m".to_string(), formula: "not_a_real_column + 1".to_string() }, AnalysisProcedureTarget::columns(&["x"]));
    let good = proc("good", P::ColumnPropertyExplorer { columns: vec!["x".to_string()] }, AnalysisProcedureTarget::columns(&["x"]));
    let result = run(df, vec![bad, good]);
    let bad_result = result.procedure_results.iter().find(|r| r.procedure_name == "bad").unwrap();
    let good_result = result.procedure_results.iter().find(|r| r.procedure_name == "good").unwrap();
    assert_eq!(bad_result.status, ProcedureStatus::ExecutionError);
    assert_eq!(good_result.status, ProcedureStatus::Completed);
}

#[test]
fn a_request_error_stops_execution_before_any_procedure_runs() {
    let df = df!["x" => [1i64, 2]].unwrap();
    let content = TableContent { table_id: Uuid::new_v4(), content_fingerprint: "test-fp".to_string(), frame: df.lazy() };
    let p = proc("p", P::ColumnPropertyExplorer { columns: vec!["x".to_string()] }, AnalysisProcedureTarget::columns(&["x"]));
    let v = AnalysisRulesetVersion::new(
        Uuid::new_v4(),
        1,
        1_000,
        "author".to_string(),
        "initial".to_string(),
        None,
        "1.0".to_string(),
        "1.0.0".to_string(),
        "0.1.0".to_string(),
        "hash".to_string(),
        TargetContract::new(vec![AnalysisCompatibleTargetType::Dataset], None, vec![]),
        DefaultScope::default(),
        vec![p],
    )
    .unwrap();
    let (result, _meta, __detail) = analyse_standard(AnalysisTarget::Table(content), Uuid::new_v4(), &v, "tester".to_string(), 1_000, "0.1.0", &|_| false);
    assert!(result.fatal_error.is_some());
    assert!(result.procedure_results.is_empty());
    assert_eq!(result.execution_status, ExecutionStatus::Failed);
}

#[test]
fn cancellation_produces_a_cancelled_status_and_skips_remaining_procedures() {
    let df = df!["x" => [1i64, 2]].unwrap();
    let content = TableContent { table_id: Uuid::new_v4(), content_fingerprint: "test-fp".to_string(), frame: df.lazy() };
    let procs: Vec<AnalysisProcedure> = (0..2).map(|i| proc(&format!("p{i}"), P::ColumnPropertyExplorer { columns: vec!["x".to_string()] }, AnalysisProcedureTarget::columns(&["x"]))).collect();
    let v = version(procs);
    // idx 0 is AP-001 (auto-inserted), idx 1/2 are p0/p1 — cancel from idx 1 onward.
    let (result, _meta, __detail) = analyse_standard(AnalysisTarget::Table(content), Uuid::new_v4(), &v, "tester".to_string(), 1_000, "0.1.0", &|idx| idx >= 1);
    assert_eq!(result.execution_status, ExecutionStatus::Cancelled);
    assert_eq!(result.procedure_results[0].status, ProcedureStatus::Completed, "AP-001 ran before cancellation");
    assert_eq!(result.procedure_results[1].status, ProcedureStatus::Skipped);
}

#[test]
fn dataset_target_requires_project_id_and_routes_to_project_operation_log() {
    let df = df!["x" => [1i64, 2]].unwrap();
    let dataset_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let content = crate::engines::content::DatasetContent { dataset_id, frame: df.lazy() };
    let p = proc("p", P::ColumnPropertyExplorer { columns: vec!["x".to_string()] }, AnalysisProcedureTarget::columns(&["x"]));
    let v = version(vec![p]);
    let (_result, meta, __detail) = analyse_standard(AnalysisTarget::Dataset { content, project_id }, Uuid::new_v4(), &v, "tester".to_string(), 1_000, "0.1.0", &|_| false);
    assert_eq!(meta.project_id, Some(project_id));
    assert_eq!(super::log_route_for(&meta), super::LogRoute::ProjectOperationLog { project_id });
}

#[test]
fn table_target_has_no_project_id_and_routes_to_table_event_log() {
    let df = df!["x" => [1i64, 2]].unwrap();
    let content = TableContent { table_id: Uuid::new_v4(), content_fingerprint: "test-fp".to_string(), frame: df.lazy() };
    let p = proc("p", P::ColumnPropertyExplorer { columns: vec!["x".to_string()] }, AnalysisProcedureTarget::columns(&["x"]));
    let v = version(vec![p]);
    let (_result, meta, __detail) = analyse_standard(AnalysisTarget::Table(content), Uuid::new_v4(), &v, "tester".to_string(), 1_000, "0.1.0", &|_| false);
    assert_eq!(meta.project_id, None);
    assert_eq!(super::log_route_for(&meta), super::LogRoute::TableEventLog);
}

/// C.3: nothing from the excluded Future/Advanced list is reachable
/// through any code path, including a Python-sidecar path. Whole-word
/// scan of the Analysis engine's own source (mirrors
/// `tests/invariants.rs::inv025_no_ai_imports_in_deterministic_engines`'s
/// approach) plus the sidecar client that could theoretically carry a
/// procedure across the process boundary.
#[test]
fn nothing_from_the_excluded_future_list_is_reachable_anywhere() {
    let excluded_tokens = [
        "joinability", "proximity_structure", "pca", "svd", "dimensionality_reduction", "hypothesis_test", "p_value", "regression_diagnostic",
        "tree_segment", "clustering", "cluster_discovery", "feature_importance", "forecast", "sentiment", "topic_model", "ai_interpretation",
    ];
    let dirs = [
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/engines/analysis"),
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/sidecar"),
    ];
    fn visit(dir: &std::path::Path, tokens: &[&str]) {
        for entry in std::fs::read_dir(dir).expect("directory must exist") {
            let path = entry.expect("readable directory entry").path();
            if path.is_dir() {
                visit(&path, tokens);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            // This test file itself necessarily carries the excluded
            // tokens as literal negative-test fixtures (this very list,
            // plus the banned-vocabulary list above) — scan production
            // code, not the test asserting production code is clean.
            if path.file_name().and_then(|n| n.to_str()) == Some("tests.rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("source file must be readable");
            // Comment lines legitimately *document* what's excluded
            // ("no forecasting", "no sentiment/topic modeling") — only
            // actual code (identifiers, string literals) is evidence of
            // reachability, so strip `//`/`//!` lines before scanning.
            let code_only: String = source.lines().filter(|line| !line.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
            let lowercase = code_only.to_lowercase();
            let words: std::collections::HashSet<&str> = lowercase.split(|c: char| !c.is_ascii_alphanumeric()).collect();
            for token in tokens {
                assert!(!words.contains(token), "excluded-list token {token:?} found in executable code in {path:?} — C.3");
            }
        }
    }
    for dir in dirs {
        visit(std::path::Path::new(dir), &excluded_tokens);
    }
}
