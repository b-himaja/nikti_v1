#[cfg(test)]
mod tests {
    use polars::prelude::*;

    use crate::engines::transformation::errors::TransformError;
    use crate::engines::transformation::table_ops as ops;
    use crate::types::*;

    fn left() -> LazyFrame {
        df!["id" => [1i64, 2, 3], "name" => ["a", "b", "c"]].unwrap().lazy()
    }

    fn right() -> LazyFrame {
        df!["id" => [2i64, 3, 4], "score" => [20.0, 30.0, 40.0]].unwrap().lazy()
    }

    // ── join_datasets ─────────────────────────────────────────────────

    #[test]
    fn join_datasets_inner_happy_path() {
        let out = ops::join_datasets(
            vec![left(), right()],
            &JoinKeys::On(vec!["id".to_string()]),
            JoinHow::Inner,
            None,
        )
        .unwrap();
        assert_eq!(out.row_count(), 2);
    }

    #[test]
    fn join_datasets_left_keeps_unmatched_rows_with_nulls() {
        let out =
            ops::join_datasets(vec![left(), right()], &JoinKeys::On(vec!["id".to_string()]), JoinHow::Left, None)
                .unwrap();
        assert_eq!(out.row_count(), 3, "left join keeps all left rows, id=1 unmatched");
        let score = out.frame.column("score").unwrap().f64().unwrap();
        assert_eq!(score.get(0), None, "unmatched left row gets null, not an error");
    }

    #[test]
    fn join_datasets_requires_exactly_two() {
        let err =
            ops::join_datasets(vec![left()], &JoinKeys::On(vec!["id".to_string()]), JoinHow::Inner, None).unwrap_err();
        assert!(matches!(err, TransformError::InvalidParams(_)));
    }

    // ── append_datasets ───────────────────────────────────────────────

    #[test]
    fn append_datasets_by_name_happy_path() {
        let a = df!["x" => [1, 2]].unwrap().lazy();
        let b = df!["x" => [3, 4]].unwrap().lazy();
        let out = ops::append_datasets(vec![a, b], MatchBy::Name, false, false).unwrap();
        assert_eq!(out.row_count(), 4);
    }

    #[test]
    fn append_datasets_by_name_diagonal_fills_missing_columns_with_null() {
        // Edge case: frames with different columns — "diagonal" alignment
        // fills the gaps with null rather than erroring.
        let a = df!["x" => [1, 2]].unwrap().lazy();
        let b = df!["y" => [3, 4]].unwrap().lazy();
        let out = ops::append_datasets(vec![a, b], MatchBy::Name, false, false).unwrap();
        assert_eq!(out.row_count(), 4);
        assert!(out.frame.column("x").is_ok());
        assert!(out.frame.column("y").is_ok());
        let x = out.frame.column("x").unwrap().i32().unwrap();
        assert_eq!(x.get(2), None);
    }

    #[test]
    fn append_datasets_deduplicate_removes_exact_duplicate_rows() {
        let a = df!["x" => [1, 2]].unwrap().lazy();
        let b = df!["x" => [2, 3]].unwrap().lazy();
        let out = ops::append_datasets(vec![a, b], MatchBy::Name, true, false).unwrap();
        assert_eq!(out.row_count(), 3, "1,2,2,3 deduplicated -> 1,2,3");
    }

    // ── unpivot_dataset ───────────────────────────────────────────────

    #[test]
    fn unpivot_dataset_happy_path() {
        let df = df!["id" => [1, 2], "jan" => [10, 20], "feb" => [11, 21]].unwrap().lazy();
        let out = ops::unpivot_dataset(df, &["id".to_string()], None, "month", "value").unwrap();
        assert_eq!(out.row_count(), 4, "2 ids x 2 value columns");
    }

    // ── aggregate_dataset ─────────────────────────────────────────────

    #[test]
    fn aggregate_dataset_group_by_happy_path() {
        let df = df!["grp" => ["a", "a", "b"], "val" => [10.0, 20.0, 30.0]].unwrap().lazy();
        let specs = vec![AggSpec { column: "val".to_string(), func: AggFunc::Sum, alias: Some("total".to_string()) }];
        let out = ops::aggregate_dataset(df, &["grp".to_string()], &specs).unwrap();
        assert_eq!(out.row_count(), 2);
        let total = out.frame.column("total").unwrap().f64().unwrap();
        assert_eq!(total.get(0), Some(30.0));
    }

    #[test]
    fn aggregate_dataset_no_group_by_collapses_to_one_row() {
        let df = df!["val" => [10.0, 20.0, 30.0]].unwrap().lazy();
        let specs = vec![AggSpec { column: "val".to_string(), func: AggFunc::Mean, alias: None }];
        let out = ops::aggregate_dataset(df, &[], &specs).unwrap();
        assert_eq!(out.row_count(), 1);
    }

    // ── intersect_datasets / subtract_datasets ───────────────────────

    #[test]
    fn intersect_datasets_happy_path() {
        let a = df!["id" => [1, 2, 3]].unwrap().lazy();
        let b = df!["id" => [2, 3, 4]].unwrap().lazy();
        let out = ops::intersect_datasets(vec![a, b]).unwrap();
        assert_eq!(out.row_count(), 2);
    }

    #[test]
    fn subtract_datasets_happy_path() {
        let a = df!["id" => [1, 2, 3]].unwrap().lazy();
        let b = df!["id" => [2, 3, 4]].unwrap().lazy();
        let out = ops::subtract_datasets(vec![a, b]).unwrap();
        assert_eq!(out.row_count(), 1, "only id=1 is in a but not b");
    }

    #[test]
    fn intersect_datasets_null_key_treated_as_equal_join_nulls_true() {
        let a = df!["id" => [Option::<i64>::None, Some(1)]].unwrap().lazy();
        let b = df!["id" => [Option::<i64>::None, Some(2)]].unwrap().lazy();
        let out = ops::intersect_datasets(vec![a, b]).unwrap();
        assert_eq!(out.row_count(), 1, "null == null via join_nulls=true, matching legacy");
    }

    // ── moving_calc / running_total ────────────────────────────────────

    #[test]
    fn moving_calc_happy_path() {
        let df = df!["x" => [1.0, 2.0, 3.0, 4.0]].unwrap().lazy();
        let out = ops::moving_calc(df, "x", None, RollingFunc::Sum, 2, None).unwrap();
        let col = out.frame.column("x_rolling_sum2").unwrap().f64().unwrap();
        assert_eq!(col.get(0), None, "window not yet full");
        assert_eq!(col.get(1), Some(3.0));
    }

    #[test]
    fn running_total_with_group_by() {
        let df = df!["grp" => ["a", "a", "b"], "x" => [1i64, 2, 10]].unwrap().lazy();
        let out = ops::running_total(df, "x", None, Some("grp")).unwrap();
        let col = out.frame.column("x_cumsum").unwrap().i64().unwrap();
        assert_eq!(col.get(1), Some(3), "cumsum within group a");
        assert_eq!(col.get(2), Some(10), "group b starts its own cumsum");
    }

    // ── compare_datasets ──────────────────────────────────────────────

    #[test]
    fn compare_datasets_happy_path_classifies_rows() {
        let a = df!["id" => [1, 2, 3], "v" => [10, 20, 30]].unwrap().lazy();
        let b = df!["id" => [2, 3, 4], "v" => [20, 99, 40]].unwrap().lazy();
        let out = ops::compare_datasets(vec![a, b], &["id".to_string()]).unwrap();
        let change = out.frame.column("_change").unwrap().str().unwrap();
        let mut counts = std::collections::HashMap::new();
        for v in change.into_no_null_iter() {
            *counts.entry(v).or_insert(0) += 1;
        }
        assert_eq!(counts.get("added").copied().unwrap_or(0), 1); // id=4
        assert_eq!(counts.get("removed").copied().unwrap_or(0), 1); // id=1
        assert_eq!(counts.get("modified").copied().unwrap_or(0), 1); // id=3, v differs
        assert_eq!(counts.get("unchanged").copied().unwrap_or(0), 1); // id=2
    }

    #[test]
    fn compare_datasets_identical_tables_are_all_unchanged() {
        let a = df!["id" => [1, 2], "v" => [10, 20]].unwrap().lazy();
        let b = df!["id" => [1, 2], "v" => [10, 20]].unwrap().lazy();
        let out = ops::compare_datasets(vec![a, b], &["id".to_string()]).unwrap();
        let change = out.frame.column("_change").unwrap().str().unwrap();
        assert!(change.into_no_null_iter().all(|v| v == "unchanged"));
    }
}
