use polars::prelude::*;
use uuid::Uuid;

use crate::engines::content::TableContent;
use crate::types::validation_function_params::ValidationFunctionParams as F;
use crate::types::{
    BindingMode, RuleNullPolicy, RuleTarget, RulesetMissingValuePolicy, ScalarValue, Severity, ValidationRule,
    ValidationRulesetVersion, ValidationTargetType,
};

use super::catalogue;
use super::{validate_standard, OverallStatus, RuleExecutionResult, RuleOutcome, ValidationTarget};

fn target(columns: &[&str]) -> RuleTarget {
    RuleTarget {
        target_type: ValidationTargetType::Column,
        binding_mode: BindingMode::FixedColumnName,
        target_columns: columns.iter().map(|s| s.to_string()).collect(),
    }
}

fn version_with(rules: Vec<ValidationRule>) -> ValidationRulesetVersion {
    ValidationRulesetVersion::new(
        Uuid::new_v4(),
        1,
        1_000,
        "tester".to_string(),
        "initial".to_string(),
        catalogue::RULESET_SCHEMA_VERSION.to_string(),
        catalogue::CATALOGUE_VERSION.to_string(),
        "hash-1".to_string(),
        rules,
        vec![],
        vec![],
        vec![],
        vec![],
        RulesetMissingValuePolicy::default(),
    )
}

fn run_rules(df: DataFrame, rules: Vec<ValidationRule>) -> super::ValidationResults {
    let content = TableContent { table_id: Uuid::new_v4(), content_fingerprint: "test-fp".to_string(), frame: df.lazy() };
    let version = version_with(rules);
    let (results, _meta, __detail) = validate_standard(ValidationTarget::Table(content), Uuid::new_v4(), &version, "tester".to_string(), 1_000, "0.1.0", &|_| false);
    results
}

fn run_single(df: DataFrame, function: F, columns: &[&str], policy: RuleNullPolicy) -> RuleExecutionResult {
    let rule = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "test-rule".to_string(), function, true, Severity::Error, target(columns), policy, "failed".to_string());
    run_rules(df, vec![rule]).rule_results.into_iter().next().unwrap()
}

const FIM: RuleNullPolicy = RuleNullPolicy::FailIfMissing;

// ===================== Part B: one test per function =====================

#[test]
fn not_missing_happy_path() {
    let df = df!["x" => [Some(1i64), None, Some(3)]].unwrap();
    let r = run_single(df, F::NotMissing, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 2, 1));
}

#[test]
fn not_blank_happy_path() {
    let df = df!["x" => ["a", "", "b"]].unwrap();
    let r = run_single(df, F::NotBlank, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 2, 1));
}

#[test]
fn not_whitespace_only_happy_path() {
    let df = df!["x" => ["a", "   ", "b"]].unwrap();
    let r = run_single(df, F::NotWhitespaceOnly, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 2, 1));
}

#[test]
fn minimum_completeness_happy_path() {
    let df = df!["x" => [Some(1i64), None, Some(3), Some(4)]].unwrap();
    let r = run_single(df, F::MinimumCompleteness { min_ratio: 0.5 }, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (1, 1, 0), "3/4 = 75% >= 50%");
}

#[test]
fn one_of_required_happy_path() {
    let df = df!["a" => [Some(1i64), None, None], "b" => [None::<i64>, Some(2), None]].unwrap();
    let r = run_single(df, F::OneOfRequired { columns: vec!["a".to_string(), "b".to_string()] }, &["a", "b"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 2, 1));
}

#[test]
fn numeric_compatible_happy_path() {
    let df = df!["x" => ["12", "3.5", "abc"]].unwrap();
    let r = run_single(df, F::NumericCompatible, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 2, 1));
}

#[test]
fn date_compatible_happy_path() {
    let df = df!["x" => ["2024-01-01", "not-a-date"]].unwrap();
    let r = run_single(df, F::DateCompatible { format: Some("%Y-%m-%d".to_string()) }, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (2, 1, 1));
}

#[test]
fn boolean_compatible_happy_path() {
    let df = df!["x" => ["true", "false", "maybe"]].unwrap();
    let r = run_single(df, F::BooleanCompatible, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 2, 1));
}

#[test]
fn min_value_happy_path() {
    let df = df!["x" => [5.0, 10.0, 15.0]].unwrap();
    let r = run_single(df, F::MinValue { min: ScalarValue::Float(10.0), inclusive: true }, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 2, 1));
}

#[test]
fn max_value_happy_path() {
    let df = df!["x" => [5.0, 10.0, 15.0]].unwrap();
    let r = run_single(df, F::MaxValue { max: ScalarValue::Float(10.0), inclusive: true }, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 2, 1));
}

#[test]
fn between_values_numeric_happy_path() {
    let df = df!["x" => [5.0, 10.0, 15.0, 25.0]].unwrap();
    let r = run_single(df, F::BetweenValues { min: ScalarValue::Float(10.0), max: ScalarValue::Float(20.0), inclusive: true }, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (4, 2, 2));
}

#[test]
fn between_values_date_happy_path() {
    let df = df!["x" => ["2024-01-01", "2024-06-15", "2024-12-31"]].unwrap().lazy().select([col("x").str().to_date(StrptimeOptions::default())]).collect().unwrap();
    let r = run_single(
        df,
        F::BetweenValues { min: ScalarValue::Str("2024-06-01".to_string()), max: ScalarValue::Str("2024-12-01".to_string()), inclusive: true },
        &["x"],
        FIM,
    );
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 1, 2));
}

#[test]
fn allowed_values_happy_path() {
    let df = df!["x" => ["A", "B", "C"]].unwrap();
    let r = run_single(df, F::AllowedValues { values: vec![ScalarValue::Str("A".to_string()), ScalarValue::Str("B".to_string())] }, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 2, 1));
}

#[test]
fn disallowed_values_happy_path() {
    let df = df!["x" => ["A", "B", "C"]].unwrap();
    let r = run_single(df, F::DisallowedValues { values: vec![ScalarValue::Str("A".to_string())] }, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 2, 1));
}

#[test]
fn distinct_count_between_happy_path() {
    let df = df!["x" => ["A", "B", "A", "C"]].unwrap();
    let r = run_single(df, F::DistinctCountBetween { min: 2, max: 3 }, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (1, 1, 0), "3 distinct values, within [2,3]");
}

#[test]
fn regex_match_happy_path() {
    let df = df!["x" => ["abc123", "xyz"]].unwrap();
    let r = run_single(df, F::RegexMatch { pattern: r"\d+".to_string() }, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (2, 1, 1));
}

#[test]
fn regex_not_match_happy_path() {
    let df = df!["x" => ["abc123", "xyz"]].unwrap();
    let r = run_single(df, F::RegexNotMatch { pattern: r"\d+".to_string() }, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (2, 1, 1));
}

#[test]
fn character_pattern_match_happy_path() {
    let df = df!["x" => ["AB12", "ab12"]].unwrap();
    let r = run_single(df, F::CharacterPatternMatch { allowed_chars: "A-Z0-9".to_string() }, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (2, 1, 1));
}

#[test]
fn min_length_happy_path() {
    let df = df!["x" => ["ab", "abcd"]].unwrap();
    let r = run_single(df, F::MinLength { min: 3 }, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (2, 1, 1));
}

#[test]
fn max_length_happy_path() {
    let df = df!["x" => ["ab", "abcd"]].unwrap();
    let r = run_single(df, F::MaxLength { max: 3 }, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (2, 1, 1));
}

#[test]
fn length_between_happy_path() {
    let df = df!["x" => ["a", "abc", "abcdefgh"]].unwrap();
    let r = run_single(df, F::LengthBetween { min: 2, max: 5 }, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 1, 2));
}

#[test]
fn unique_column_happy_path() {
    let df = df!["x" => [1i64, 2, 2, 3]].unwrap();
    let r = run_single(df, F::UniqueColumn, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (4, 2, 2));
}

#[test]
fn unique_column_set_happy_path() {
    let df = df!["a" => [1i64, 1, 2], "b" => [1i64, 1, 2]].unwrap();
    let r = run_single(df, F::UniqueColumnSet { columns: vec!["a".to_string(), "b".to_string()] }, &["a", "b"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 1, 2));
}

#[test]
fn no_duplicate_rows_happy_path() {
    let df = df!["a" => [1i64, 1, 2], "b" => ["x", "x", "y"]].unwrap();
    let r = run_single(df, F::NoDuplicateRows, &[], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 1, 2));
}

#[test]
fn column_compare_happy_path() {
    let df = df!["a" => [10i64, 20, 5], "b" => [5i64, 25, 5]].unwrap();
    let r = run_single(df, F::ColumnCompare { other_column: "b".to_string(), op: crate::types::CompareOp::Gt }, &["a"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 1, 2));
}

#[test]
fn required_if_happy_path() {
    let df = df!["status" => ["closed", "open", "closed"], "reason" => [Some("done"), None::<&str>, None]].unwrap();
    let r = run_single(
        df,
        F::RequiredIf { other_column: "status".to_string(), other_equals: ScalarValue::Str("closed".to_string()) },
        &["reason"],
        FIM,
    );
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 2, 1));
}

#[test]
fn mutually_exclusive_happy_path() {
    let df = df!["a" => [Some(1i64), None, Some(1)], "b" => [None::<i64>, Some(2), Some(2)]].unwrap();
    let r = run_single(df, F::MutuallyExclusive { columns: vec!["a".to_string(), "b".to_string()] }, &["a", "b"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 2, 1));
}

#[test]
fn exists_in_reference_happy_path() {
    let df = df!["x" => ["A", "B", "Z"]].unwrap();
    let r = run_single(
        df,
        F::ExistsInReference { reference_values: vec![ScalarValue::Str("A".to_string()), ScalarValue::Str("B".to_string())] },
        &["x"],
        FIM,
    );
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 2, 1));
}

#[test]
fn not_exists_in_reference_happy_path() {
    let df = df!["x" => ["A", "B", "Z"]].unwrap();
    let r = run_single(
        df,
        F::NotExistsInReference { reference_values: vec![ScalarValue::Str("A".to_string())] },
        &["x"],
        FIM,
    );
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 2, 1));
}

#[test]
fn date_format_match_happy_path() {
    let df = df!["x" => ["2024-01-01", "01/01/2024"]].unwrap();
    let r = run_single(df, F::DateFormatMatch { format: "%Y-%m-%d".to_string() }, &["x"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (2, 1, 1));
}

#[test]
fn expression_true_happy_path() {
    let df = df!["amount" => [10.0, -5.0, 20.0]].unwrap();
    let r = run_single(df, F::ExpressionTrue { formula: "amount > 0".to_string() }, &["amount"], FIM);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 2, 1));
}

// ===================== Unsupported function_id =====================

#[test]
fn unsupported_function_id_is_not_executable_not_a_crash() {
    let df = df!["x" => [1i64, 2]].unwrap();
    let r = run_single(df, F::Unknown, &["x"], FIM);
    assert_eq!(r.outcome, RuleOutcome::NotExecutable);
}

// ===================== Null-policy behavior =====================

#[test]
fn fail_if_missing_counts_missing_rows_as_failed() {
    let df = df!["x" => [Some(50.0), None, Some(5.0)]].unwrap();
    let r = run_single(df, F::MinValue { min: ScalarValue::Float(10.0), inclusive: true }, &["x"], RuleNullPolicy::FailIfMissing);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (3, 1, 2), "the null row counts as failed under FailIfMissing");
}

#[test]
fn ignore_missing_excludes_missing_rows_entirely() {
    let df = df!["x" => [Some(50.0), None, Some(5.0)]].unwrap();
    let r = run_single(df, F::MinValue { min: ScalarValue::Float(10.0), inclusive: true }, &["x"], RuleNullPolicy::IgnoreMissing);
    assert_eq!((r.evaluated_count, r.passed_count, r.failed_count), (2, 1, 1), "the null row is excluded from evaluated_count under IgnoreMissing");
}

// ===================== Part C: engine-level behavior =====================

#[test]
fn outcome_never_takes_the_value_warning_anywhere() {
    // Amendment 10: `warning` is a `Severity`, never an `outcome` — the
    // `RuleOutcome` enum has no such variant at all (compile-time
    // guarantee), so this test exercises every outcome variant that
    // *does* exist and confirms none of their Debug renderings is
    // "warning" while a Warning-severity rule is present.
    let df = df!["x" => [Some(1i64), None]].unwrap();
    let rule = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "r".to_string(), F::NotMissing, true, Severity::Warning, target(&["x"]), FIM, "msg".to_string());
    let results = run_rules(df, vec![rule]);
    for r in &results.rule_results {
        let rendered = format!("{:?}", r.outcome).to_lowercase();
        assert_ne!(rendered, "warning");
    }
}

#[test]
fn warning_severity_rule_still_executes_and_reports_by_default() {
    // Amendment 8: no `severity_threshold` gating exists in this engine
    // at all — every enabled, executable rule runs regardless of
    // severity. This test exercises that default directly.
    let df = df!["x" => [Some(1i64), None]].unwrap();
    let rule = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "r".to_string(), F::NotMissing, true, Severity::Warning, target(&["x"]), FIM, "msg".to_string());
    let results = run_rules(df, vec![rule]);
    assert_eq!(results.rule_results.len(), 1);
    assert_ne!(results.rule_results[0].outcome, RuleOutcome::Skipped);
    assert_eq!(results.summary.warning_failures, 1);
}

#[test]
fn validation_never_mutates_the_target_frame() {
    let df = df!["x" => [Some(1i64), None, Some(3)]].unwrap();
    let content = TableContent { table_id: Uuid::new_v4(), content_fingerprint: "test-fp".to_string(), frame: df.clone().lazy() };
    let before = content.frame.clone().collect().unwrap();
    let rule = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "r".to_string(), F::NotMissing, true, Severity::Error, target(&["x"]), FIM, "msg".to_string());
    let version = version_with(vec![rule]);
    let _ = validate_standard(ValidationTarget::Table(content.clone()), Uuid::new_v4(), &version, "tester".to_string(), 1_000, "0.1.0", &|_| false);
    let after = content.frame.clone().collect().unwrap();
    assert_eq!(before, after);
}

#[test]
fn a_single_rule_execution_error_does_not_abort_the_run() {
    // A malformed regex is a real per-rule technical failure (the regex
    // engine rejects it) rather than a business validation failure —
    // this must resolve to `ExecutionError` for that one rule while the
    // rest of the ruleset still executes normally.
    let df = df!["x" => ["a", "b"]].unwrap();
    let bad_rule = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "bad".to_string(), F::RegexMatch { pattern: "[".to_string() }, true, Severity::Error, target(&["x"]), FIM, "msg".to_string());
    let good_rule = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "good".to_string(), F::NotMissing, true, Severity::Error, target(&["x"]), FIM, "msg".to_string());
    let results = run_rules(df, vec![bad_rule, good_rule]);
    assert_eq!(results.rule_results.len(), 2, "the run continued past the failing rule");
    assert_eq!(results.rule_results[0].outcome, RuleOutcome::ExecutionError);
    assert_eq!(results.rule_results[1].outcome, RuleOutcome::Passed);
    assert_eq!(results.summary.overall_status, OverallStatus::CompletedWithErrors);
}

#[test]
fn a_request_error_stops_execution_before_any_rule_runs() {
    let df = df!["x" => [1i64, 2]].unwrap();
    let content = TableContent { table_id: Uuid::new_v4(), content_fingerprint: "test-fp".to_string(), frame: df.lazy() };
    let rule = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "r".to_string(), F::NotMissing, true, Severity::Error, target(&["x"]), FIM, "msg".to_string());
    // Incompatible target kind (ruleset restricted to Dataset only, run
    // against a Table) is a request-level rejection (C.1's "Validate
    // Request" stage), not a per-rule concern.
    let version = ValidationRulesetVersion::new(
        Uuid::new_v4(),
        1,
        1_000,
        "tester".to_string(),
        "initial".to_string(),
        catalogue::RULESET_SCHEMA_VERSION.to_string(),
        catalogue::CATALOGUE_VERSION.to_string(),
        "hash".to_string(),
        vec![rule],
        vec![crate::types::CompatibleTargetObjectType::Dataset],
        vec![],
        vec![],
        vec![],
        RulesetMissingValuePolicy::default(),
    );
    let (results, _meta, __detail) = validate_standard(ValidationTarget::Table(content), Uuid::new_v4(), &version, "tester".to_string(), 1_000, "0.1.0", &|_| false);
    assert!(results.request_error.is_some());
    assert!(results.rule_results.is_empty(), "no rule ran");
    assert_eq!(results.summary.overall_status, OverallStatus::FailedExecution);
}

#[test]
fn cancellation_produces_a_cancelled_status_and_skips_remaining_rules() {
    let df = df!["x" => [1i64, 2]].unwrap();
    let content = TableContent { table_id: Uuid::new_v4(), content_fingerprint: "test-fp".to_string(), frame: df.lazy() };
    let rules: Vec<ValidationRule> = (0..3)
        .map(|i| ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), format!("r{i}"), F::NotMissing, true, Severity::Error, target(&["x"]), FIM, "msg".to_string()))
        .collect();
    let version = version_with(rules);
    // Cancel before rule index 1 (i.e. after the first rule has run).
    let (results, _meta, __detail) = validate_standard(ValidationTarget::Table(content), Uuid::new_v4(), &version, "tester".to_string(), 1_000, "0.1.0", &|idx| idx >= 1);
    assert_eq!(results.summary.overall_status, OverallStatus::Cancelled);
    assert_eq!(results.rule_results[0].outcome, RuleOutcome::Passed, "the rule that ran before cancellation is traceable");
    assert_eq!(results.rule_results[1].outcome, RuleOutcome::Skipped);
    assert_eq!(results.rule_results[2].outcome, RuleOutcome::Skipped);
}

#[test]
fn dataset_target_requires_project_id_and_routes_to_project_operation_log() {
    let df = df!["x" => [1i64, 2]].unwrap();
    let dataset_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let content = crate::engines::content::DatasetContent { dataset_id, frame: df.lazy() };
    let rule = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "r".to_string(), F::NotMissing, true, Severity::Error, target(&["x"]), FIM, "msg".to_string());
    let version = version_with(vec![rule]);
    let (_results, meta, __detail) =
        validate_standard(ValidationTarget::Dataset { content, project_id }, Uuid::new_v4(), &version, "tester".to_string(), 1_000, "0.1.0", &|_| false);
    assert_eq!(meta.project_id, Some(project_id));
    assert_eq!(super::log_route_for(&meta), super::LogRoute::ProjectOperationLog { project_id });
}

#[test]
fn table_target_has_no_project_id_and_routes_to_table_event_log() {
    let df = df!["x" => [1i64, 2]].unwrap();
    let content = TableContent { table_id: Uuid::new_v4(), content_fingerprint: "test-fp".to_string(), frame: df.lazy() };
    let rule = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "r".to_string(), F::NotMissing, true, Severity::Error, target(&["x"]), FIM, "msg".to_string());
    let version = version_with(vec![rule]);
    let (_results, meta, __detail) = validate_standard(ValidationTarget::Table(content), Uuid::new_v4(), &version, "tester".to_string(), 1_000, "0.1.0", &|_| false);
    assert_eq!(meta.project_id, None);
    assert_eq!(super::log_route_for(&meta), super::LogRoute::TableEventLog);
}

const BANNED_WORDS: &[&str] = &["bad", "poor", "suspicious", "problem", "risk", "recommended", "quality score"];

#[test]
fn results_never_contain_banned_evaluative_language() {
    let df = df!["x" => [Some(1i64), None]].unwrap();
    let rule = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "r".to_string(), F::NotMissing, true, Severity::Error, target(&["x"]), FIM, "value is missing".to_string());
    let results = run_rules(df, vec![rule]);
    let rendered = format!("{results:?}").to_lowercase();
    for word in BANNED_WORDS {
        assert!(!rendered.contains(word), "banned word '{word}' leaked into ValidationResults output");
    }
}
