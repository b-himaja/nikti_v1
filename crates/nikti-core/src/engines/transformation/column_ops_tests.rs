#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use polars::prelude::*;

    use crate::engines::transformation::column_ops as ops;
    use crate::engines::transformation::errors::TransformError;
    use crate::types::*;

    use ops::{
        apply_scaler, change_dtype, coalesce_columns, conditional_flag, copy_column, datetime_op, derive_column,
        differencing, discretise_column, drop_columns, fill_missing, flag_values, generate_row_number,
        generate_uuid, hash_column, lead_lag, null_standardise, recode_column, rename_columns, reorder_columns,
        string_op, text_standardise,
    };

    fn sample() -> LazyFrame {
        df![
            "id" => [1i64, 2, 3, 4, 5],
            "amount" => [10.0f64, 20.0, 30.0, 40.0, 50.0],
            "category" => ["a", "b", "a", "c", "b"],
            "flag" => [Some(1i64), None, Some(2), None, Some(1)],
        ]
        .unwrap()
        .lazy()
    }

    // ── derive_column ──────────────────────────────────────────────────

    #[test]
    fn derive_column_happy_path() {
        let out = derive_column(sample(), "double", "[amount] * 2", false).unwrap();
        let col = out.frame.column("double").unwrap().f64().unwrap();
        assert_eq!(col.get(0), Some(20.0));
    }

    #[test]
    fn derive_column_rejects_overwrite_without_flag() {
        let err = derive_column(sample(), "amount", "[amount] + 1", false).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    // ── recode_column ──────────────────────────────────────────────────

    #[test]
    fn recode_column_happy_path() {
        let out = recode_column(
            sample(),
            "category",
            &[("a".into(), "Alpha".into()), ("b".into(), "Beta".into())],
            None,
            Some("category_recoded"),
        )
        .unwrap();
        let col = out.frame.column("category_recoded").unwrap().str().unwrap();
        assert_eq!(col.get(0), Some("Alpha"));
        // Edge case: unmapped value falls back to original (cast to string), not null.
        assert_eq!(col.get(3), Some("c"));
    }

    #[test]
    fn recode_column_default_used_when_no_default_keeps_original() {
        let out = recode_column(sample(), "category", &[("a".into(), "Alpha".into())], Some("Other"), None).unwrap();
        let col = out.frame.column("category").unwrap().str().unwrap();
        assert_eq!(col.get(0), Some("Alpha"));
        assert_eq!(col.get(3), Some("Other"));
    }

    // ── discretise_column ─────────────────────────────────────────────

    #[test]
    fn discretise_column_fixed_bins_happy_path() {
        let out = discretise_column(
            sample(),
            "amount",
            None,
            &DiscretiseMethod::Fixed { bins: vec![25.0] },
            None,
        )
        .unwrap();
        assert!(out.frame.column("amount_bin").is_ok());
        assert_eq!(out.row_count(), 5);
    }

    #[test]
    fn discretise_column_equal_width_computes_breakpoints_from_data() {
        // amount ranges 10..50; equal_width with 2 bins -> single breakpoint at 30.
        let out =
            discretise_column(sample(), "amount", None, &DiscretiseMethod::EqualWidth { n_bins: 2 }, None).unwrap();
        assert_eq!(out.row_count(), 5);
    }

    // ── lead_lag ───────────────────────────────────────────────────────

    #[test]
    fn lead_lag_happy_path_lag() {
        let out = lead_lag(sample(), "amount", None, 1, None, None).unwrap();
        let col = out.frame.column("amount_lag1").unwrap().f64().unwrap();
        assert_eq!(col.get(0), None, "first row has no prior value");
        assert_eq!(col.get(1), Some(10.0));
    }

    #[test]
    fn lead_lag_negative_periods_is_lead_and_fill_value_applies() {
        let out = lead_lag(sample(), "amount", None, -1, None, Some(ScalarValue::Float(-1.0))).unwrap();
        let col = out.frame.column("amount_lead1").unwrap().f64().unwrap();
        assert_eq!(col.get(4), Some(-1.0), "last row's lead has no future value, filled");
    }

    // ── fill_missing ──────────────────────────────────────────────────

    #[test]
    fn fill_missing_constant_happy_path() {
        let out =
            fill_missing(sample(), &["flag".to_string()], &FillStrategy::Constant(ScalarValue::Int(0))).unwrap();
        let col = out.frame.column("flag").unwrap().i64().unwrap();
        assert_eq!(col.get(1), Some(0));
    }

    #[test]
    fn fill_missing_mean_edge_case_all_nulls_stays_null() {
        let df = df!["x" => [Option::<f64>::None, None, None]].unwrap().lazy();
        let out = fill_missing(df, &["x".to_string()], &FillStrategy::Mean).unwrap();
        let col = out.frame.column("x").unwrap().f64().unwrap();
        assert_eq!(col.get(0), None, "mean of an all-null column is itself null, not 0");
    }

    // ── apply_scaler ──────────────────────────────────────────────────

    #[test]
    fn apply_scaler_minmax_happy_path() {
        let out = apply_scaler(sample(), &["amount".to_string()], ScalerMethod::MinMax, None).unwrap();
        let col = out.frame.column("amount_scaled").unwrap().f64().unwrap();
        assert_eq!(col.get(0), Some(0.0));
        assert_eq!(col.get(4), Some(1.0));
    }

    #[test]
    fn apply_scaler_zscore_uses_sample_std_ddof1() {
        let out = apply_scaler(sample(), &["amount".to_string()], ScalerMethod::ZScore, None).unwrap();
        let col = out.frame.column("amount_scaled").unwrap().f64().unwrap();
        // mean=30, sample std (ddof=1) of [10,20,30,40,50] = ~15.811
        let v = col.get(0).unwrap();
        assert!((v - (-1.2649)).abs() < 0.001);
    }

    // ── rename_columns / reorder_columns ────────────────────────────

    #[test]
    fn rename_columns_happy_path() {
        let out = rename_columns(sample(), &[("amount".into(), "total".into())]).unwrap();
        assert!(out.frame.column("total").is_ok());
        assert!(out.frame.column("amount").is_err());
    }

    #[test]
    fn rename_columns_unknown_source_errors() {
        let err = rename_columns(sample(), &[("does_not_exist".into(), "x".into())]).unwrap_err();
        assert!(matches!(err, TransformError::Polars(_)));
    }

    #[test]
    fn reorder_columns_happy_path() {
        let out = reorder_columns(sample(), &["category".to_string(), "id".to_string()]).unwrap();
        assert_eq!(out.frame.get_column_names(), vec!["category", "id"]);
    }

    // ── string_op ─────────────────────────────────────────────────────

    #[test]
    fn string_op_upper_happy_path() {
        let out = string_op(sample(), "category", None, &StringOpKind::Upper).unwrap();
        let col = out.frame.column("category").unwrap().str().unwrap();
        assert_eq!(col.get(0), Some("A"));
    }

    #[test]
    fn string_op_replace_literal_vs_regex() {
        let df = df!["s" => ["a.b", "a.c"]].unwrap().lazy();
        let out = string_op(
            df,
            "s",
            Some("out"),
            &StringOpKind::Replace { pattern: ".".into(), replacement: "X".into(), regex: false },
        )
        .unwrap();
        let col = out.frame.column("out").unwrap().str().unwrap();
        // Literal replace: only the literal "." character is replaced, not "any char".
        assert_eq!(col.get(0), Some("aXb"));
    }

    // ── datetime_op ───────────────────────────────────────────────────

    #[test]
    fn datetime_op_extract_year_happy_path() {
        let df = df!["d" => [NaiveDate::from_ymd_opt(2024, 3, 15).unwrap()]].unwrap().lazy();
        let out = datetime_op(df, "d", None, &DatetimeOpKind::ExtractYear).unwrap();
        let col = out.frame.column("d_extract_year").unwrap().i32().unwrap();
        assert_eq!(col.get(0), Some(2024));
    }

    #[test]
    fn datetime_op_diff_always_returns_days_ignoring_unused_unit() {
        // Legacy bug we deliberately did not port: `unit` was accepted but
        // never read, so diff always returned days regardless. There is no
        // `unit` field at all in DatetimeOpKind::Diff — confirming the
        // fixed, always-days behavior structurally, not just by omission.
        let df = df![
            "a" => [NaiveDate::from_ymd_opt(2024, 3, 20).unwrap()],
            "b" => [NaiveDate::from_ymd_opt(2024, 3, 15).unwrap()],
        ]
        .unwrap()
        .lazy();
        let out = datetime_op(df, "a", None, &DatetimeOpKind::Diff { other_column: "b".to_string() }).unwrap();
        let col = out.frame.column("a_diff").unwrap().i64().unwrap();
        assert_eq!(col.get(0), Some(5));
    }

    // ── differencing ──────────────────────────────────────────────────

    #[test]
    fn differencing_happy_path() {
        let out = differencing(sample(), "amount", None, 1, false).unwrap();
        let col = out.frame.column("amount_diff1").unwrap().f64().unwrap();
        assert_eq!(col.get(0), None);
        assert_eq!(col.get(1), Some(10.0));
    }

    #[test]
    fn differencing_pct_change_edge_case_first_row_null() {
        let out = differencing(sample(), "amount", None, 1, true).unwrap();
        let col = out.frame.column("amount_diff1").unwrap().f64().unwrap();
        assert_eq!(col.get(0), None, "no prior value to compute pct change against");
        assert!((col.get(1).unwrap() - 1.0).abs() < 1e-9);
    }

    // ── conditional_flag ──────────────────────────────────────────────

    #[test]
    fn conditional_flag_first_match_wins() {
        let conditions = vec![
            ConditionCase {
                sub_conditions: vec![SubCondition {
                    column: "amount".into(),
                    op: CompareOp::Gt,
                    value: Some(ScalarValue::Float(5.0)),
                    negate: false,
                }],
                logic: CaseLogic::And,
                then: ScalarValue::Str("high".into()),
            },
            ConditionCase {
                sub_conditions: vec![SubCondition {
                    column: "amount".into(),
                    op: CompareOp::Gt,
                    value: Some(ScalarValue::Float(35.0)),
                    negate: false,
                }],
                logic: CaseLogic::And,
                then: ScalarValue::Str("very_high".into()),
            },
        ];
        let out = conditional_flag(sample(), "level", &conditions, Some(ScalarValue::Str("low".into()))).unwrap();
        let col = out.frame.column("level").unwrap().str().unwrap();
        // row 3 (amount=40) matches BOTH conditions; first condition wins.
        assert_eq!(col.get(3), Some("high"));
    }

    #[test]
    fn conditional_flag_requires_at_least_one_condition() {
        let err = conditional_flag(sample(), "level", &[], None).unwrap_err();
        assert!(err.to_string().contains("At least one condition"));
    }

    // ── flag_values ───────────────────────────────────────────────────

    #[test]
    fn flag_values_duplicates_happy_path() {
        let df = df!["x" => [1, 2, 2, 3]].unwrap().lazy();
        let out = flag_values(df, "x", None, &FlagMethod::Duplicates).unwrap();
        let col = out.frame.column("x_flag").unwrap().bool().unwrap();
        assert_eq!(col.get(1), Some(true));
        assert_eq!(col.get(0), Some(false));
    }

    #[test]
    fn flag_values_zscore_zero_variance_errors_not_divide_by_zero() {
        let df = df!["x" => [5.0, 5.0, 5.0]].unwrap().lazy();
        let err = flag_values(df, "x", None, &FlagMethod::OutlierZScore { threshold: 3.0 }).unwrap_err();
        assert!(err.to_string().contains("zero variance"));
    }

    // ── change_dtype ──────────────────────────────────────────────────

    #[test]
    fn change_dtype_happy_path() {
        let df = df!["x" => ["1", "2", "3"]].unwrap().lazy();
        let out = change_dtype(df, "x", TargetType::Int64, None).unwrap();
        assert_eq!(out.frame.column("x").unwrap().dtype(), &DataType::Int64);
    }

    #[test]
    fn change_dtype_strict_cast_rejects_unparseable_value() {
        let df = df!["x" => ["1", "hello", "3"]].unwrap().lazy();
        let err = change_dtype(df, "x", TargetType::Int64, None).unwrap_err();
        assert!(err.to_string().contains("Cannot convert"));
    }

    #[test]
    fn change_dtype_blocks_boolean_to_date() {
        let df = df!["x" => [true, false]].unwrap().lazy();
        let err = change_dtype(df, "x", TargetType::Date, None).unwrap_err();
        assert!(err.to_string().contains("no meaningful date mapping"));
    }

    // ── drop_columns / copy_column / coalesce_columns ─────────────────

    #[test]
    fn drop_columns_happy_path() {
        let out = drop_columns(sample(), &["flag".to_string()]).unwrap();
        assert!(out.frame.column("flag").is_err());
    }

    #[test]
    fn copy_column_happy_path() {
        let out = copy_column(sample(), "amount", "amount_copy").unwrap();
        assert_eq!(out.frame.column("amount").unwrap(), out.frame.column("amount_copy").unwrap());
    }

    #[test]
    fn coalesce_columns_happy_path() {
        let df = df!["a" => [Option::<i64>::None, Some(2)], "b" => [Some(9), Some(8)]].unwrap().lazy();
        let out = coalesce_columns(df, &["a".to_string(), "b".to_string()], "c").unwrap();
        let col = out.frame.column("c").unwrap().i64().unwrap();
        assert_eq!(col.get(0), Some(9));
        assert_eq!(col.get(1), Some(2));
    }

    #[test]
    fn coalesce_columns_requires_at_least_two() {
        let err = coalesce_columns(sample(), &["amount".to_string()], "c").unwrap_err();
        assert!(err.to_string().contains("at least 2"));
    }

    // ── generate_row_number ───────────────────────────────────────────

    #[test]
    fn generate_row_number_happy_path_default_start() {
        let out = generate_row_number(sample(), None, 1).unwrap();
        let col = out.frame.column("row_number").unwrap().i64().unwrap();
        assert_eq!(col.get(0), Some(1));
        assert_eq!(col.get(4), Some(5));
    }

    #[test]
    fn generate_row_number_custom_start() {
        let out = generate_row_number(sample(), None, 100).unwrap();
        let col = out.frame.column("row_number").unwrap().i64().unwrap();
        assert_eq!(col.get(0), Some(100));
    }

    // ── generate_uuid ─────────────────────────────────────────────────

    #[test]
    fn generate_uuid_happy_path_produces_unique_values() {
        let out = generate_uuid(sample(), None, None).unwrap();
        let col = out.frame.column("uuid").unwrap().str().unwrap();
        let values: std::collections::HashSet<_> = col.into_no_null_iter().collect();
        assert_eq!(values.len(), 5);
    }

    #[test]
    fn generate_uuid_seeded_is_deterministic_unlike_legacy() {
        // Nikti addition beyond the legacy behavior: with a seed, replaying
        // the same operation produces byte-identical output — required
        // for Recipe replay reproducibility (legacy uuid4() never had this).
        let out1 = generate_uuid(sample(), None, Some(42)).unwrap();
        let out2 = generate_uuid(sample(), None, Some(42)).unwrap();
        assert_eq!(
            out1.frame.column("uuid").unwrap().str().unwrap().into_no_null_iter().collect::<Vec<_>>(),
            out2.frame.column("uuid").unwrap().str().unwrap().into_no_null_iter().collect::<Vec<_>>(),
        );
    }

    // ── null_standardise ──────────────────────────────────────────────

    #[test]
    fn null_standardise_happy_path_case_insensitive() {
        let df = df!["x" => ["N/A", "value", "NULL", "unknown"]].unwrap().lazy();
        let out = null_standardise(df, &["x".to_string()], None).unwrap();
        let col = out.frame.column("x").unwrap().str().unwrap();
        assert_eq!(col.get(0), None);
        assert_eq!(col.get(1), Some("value"));
        assert_eq!(col.get(2), None);
    }

    #[test]
    fn null_standardise_custom_variants_override_defaults() {
        let df = df!["x" => ["N/A", "custom_null"]].unwrap().lazy();
        let out = null_standardise(df, &["x".to_string()], Some(vec!["custom_null".to_string()])).unwrap();
        let col = out.frame.column("x").unwrap().str().unwrap();
        assert_eq!(col.get(0), Some("N/A"), "default variants not applied once a custom list is given");
        assert_eq!(col.get(1), None);
    }

    // ── text_standardise ──────────────────────────────────────────────

    #[test]
    fn text_standardise_happy_path_case_insensitive_default() {
        let df = df!["country" => ["USA", "usa", "U.S.A.", "France"]].unwrap().lazy();
        let out = text_standardise(
            df,
            "country",
            None,
            &[("usa".into(), "United States".into())],
            false,
        )
        .unwrap();
        let col = out.frame.column("country").unwrap().str().unwrap();
        assert_eq!(col.get(0), Some("United States"));
        assert_eq!(col.get(1), Some("United States"));
        assert_eq!(col.get(3), Some("France"), "unmapped value keeps original");
    }

    #[test]
    fn text_standardise_case_sensitive_does_not_match_different_case() {
        let df = df!["country" => ["USA", "usa"]].unwrap().lazy();
        let out = text_standardise(df, "country", None, &[("USA".into(), "United States".into())], true).unwrap();
        let col = out.frame.column("country").unwrap().str().unwrap();
        assert_eq!(col.get(0), Some("United States"));
        assert_eq!(col.get(1), Some("usa"), "case-sensitive: lowercase 'usa' does not match 'USA'");
    }

    // ── hash_column ───────────────────────────────────────────────────

    #[test]
    fn hash_column_happy_path_sha256() {
        let df = df!["x" => ["hello"]].unwrap().lazy();
        let out = hash_column(df, "x", None, HashAlgorithm::Sha256).unwrap();
        let col = out.frame.column("x_hash").unwrap().str().unwrap();
        assert_eq!(col.get(0), Some("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"));
    }

    #[test]
    fn hash_column_null_stays_null_not_hash_of_string_none() {
        let df = df!["x" => [Option::<&str>::None, Some("hello")]].unwrap().lazy();
        let out = hash_column(df, "x", None, HashAlgorithm::Sha256).unwrap();
        let col = out.frame.column("x_hash").unwrap().str().unwrap();
        assert_eq!(col.get(0), None, "null input must hash to null, not the string 'None'/'null'");
    }
}
