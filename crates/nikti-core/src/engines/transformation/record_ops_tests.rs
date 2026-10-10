#[cfg(test)]
mod tests {
    use polars::prelude::*;

    use crate::engines::transformation::errors::TransformError;
    use crate::engines::transformation::record_ops as ops;
    use crate::types::*;

    fn sample() -> LazyFrame {
        df!["id" => [1i64, 2, 3, 4, 5], "amount" => [10.0f64, 20.0, 30.0, 40.0, 50.0]].unwrap().lazy()
    }

    // ── filter_dataset ────────────────────────────────────────────────

    #[test]
    fn filter_dataset_happy_path() {
        let out = ops::filter_dataset(sample(), "[amount] > 25", false).unwrap();
        assert_eq!(out.row_count(), 3);
        assert!(out.complement.is_none());
    }

    #[test]
    fn filter_dataset_complement_partitions_rows() {
        let out = ops::filter_dataset(sample(), "[amount] > 25", true).unwrap();
        let complement = out.complement.as_ref().unwrap();
        assert_eq!(out.row_count() + complement.row_count(), 5);
    }

    #[test]
    fn filter_dataset_null_condition_excluded_from_both_sides() {
        // Edge case identified during the port: a row where the condition
        // itself evaluates to null is dropped by BOTH `filter(expr)` and
        // `filter(~expr)` (three-valued logic) — it appears in neither
        // output. Matches legacy Polars behavior exactly (not a porting
        // bug), but must be verified, not assumed.
        let df = df!["a" => [Some(1i64), None, Some(3)]].unwrap().lazy();
        let out = ops::filter_dataset(df, "[a] > 1", true).unwrap();
        let complement = out.complement.as_ref().unwrap();
        assert_eq!(
            out.row_count() + complement.row_count(),
            2,
            "the null-`a` row (comparison is null, not true or false) is in neither output"
        );
    }

    // ── sample_dataset ────────────────────────────────────────────────

    #[test]
    fn sample_dataset_random_n_happy_path() {
        let out = ops::sample_dataset(sample(), SampleMethod::Random, Some(3), None, 42, None).unwrap();
        assert_eq!(out.row_count(), 3);
    }

    #[test]
    fn sample_dataset_random_same_seed_is_deterministic() {
        let out1 = ops::sample_dataset(sample(), SampleMethod::Random, Some(3), None, 7, None).unwrap();
        let out2 = ops::sample_dataset(sample(), SampleMethod::Random, Some(3), None, 7, None).unwrap();
        let ids1: Vec<Option<i64>> = out1.frame.column("id").unwrap().i64().unwrap().into_iter().collect();
        let ids2: Vec<Option<i64>> = out2.frame.column("id").unwrap().i64().unwrap().into_iter().collect();
        assert_eq!(ids1, ids2, "same seed must reproduce the same sample — required for Recipe replay");
    }

    #[test]
    fn sample_dataset_random_requires_n_or_fraction() {
        let err = ops::sample_dataset(sample(), SampleMethod::Random, None, None, 42, None).unwrap_err();
        assert!(matches!(err, TransformError::InvalidParams(_)));
    }

    #[test]
    fn sample_dataset_systematic_happy_path() {
        let out = ops::sample_dataset(sample(), SampleMethod::Systematic, Some(2), None, 0, None).unwrap();
        assert_eq!(out.row_count(), 2);
    }

    #[test]
    fn sample_dataset_stratified_preserves_group_presence() {
        let df = df!["grp" => ["a", "a", "a", "b", "b"], "v" => [1, 2, 3, 4, 5]].unwrap().lazy();
        let out = ops::sample_dataset(df, SampleMethod::Stratified, None, Some(0.5), 1, Some("grp")).unwrap();
        // Each group contributes at least 1 row (legacy: `max(1, ...)`).
        let groups: std::collections::HashSet<String> =
            out.frame.column("grp").unwrap().str().unwrap().into_no_null_iter().map(String::from).collect();
        assert!(groups.contains("a"));
        assert!(groups.contains("b"));
    }

    // ── sort_dataset ──────────────────────────────────────────────────

    #[test]
    fn sort_dataset_happy_path_descending() {
        let out = ops::sort_dataset(
            sample(),
            &[SortKey { column: "amount".to_string(), descending: true }],
            true,
        )
        .unwrap();
        let col = out.frame.column("amount").unwrap().f64().unwrap();
        assert_eq!(col.get(0), Some(50.0));
    }

    #[test]
    fn sort_dataset_nulls_last_edge_case() {
        let df = df!["x" => [Some(2i64), None, Some(1)]].unwrap().lazy();
        let out = ops::sort_dataset(df, &[SortKey { column: "x".to_string(), descending: false }], true).unwrap();
        let col = out.frame.column("x").unwrap().i64().unwrap();
        assert_eq!(col.get(2), None, "null sorts last, not first, per nulls_last=true");
    }

    #[test]
    fn sort_dataset_requires_at_least_one_key() {
        let err = ops::sort_dataset(sample(), &[], true).unwrap_err();
        assert!(matches!(err, TransformError::InvalidParams(_)));
    }

    // ── slice_rows ────────────────────────────────────────────────────

    #[test]
    fn slice_rows_happy_path() {
        let out = ops::slice_rows(sample(), 1, 3).unwrap();
        assert_eq!(out.row_count(), 2);
        let ids = out.frame.column("id").unwrap().i64().unwrap();
        assert_eq!(ids.get(0), Some(2));
    }

    #[test]
    fn slice_rows_end_before_start_is_rejected_not_silently_reinterpreted() {
        // The legacy prototype had no guard here and would have fed a
        // negative length to Polars' `.slice()`, which has its own
        // special (and non-obvious) meaning for negative lengths. We
        // reject this explicitly instead of replicating that behavior.
        let err = ops::slice_rows(sample(), 3, 1).unwrap_err();
        assert!(matches!(err, TransformError::InvalidParams(_)));
    }

    // ── delete_rows ───────────────────────────────────────────────────

    #[test]
    fn delete_rows_happy_path() {
        let out = ops::delete_rows(sample(), &[1, 3]).unwrap();
        assert_eq!(out.row_count(), 3);
        let ids = out.frame.column("id").unwrap().i64().unwrap();
        assert_eq!(ids.into_no_null_iter().collect::<Vec<_>>(), vec![1, 3, 5]);
    }

    #[test]
    fn delete_rows_out_of_range_index_is_a_no_op_not_an_error() {
        let out = ops::delete_rows(sample(), &[999]).unwrap();
        assert_eq!(out.row_count(), 5);
    }

    // ── tag_rows ──────────────────────────────────────────────────────

    #[test]
    fn tag_rows_happy_path_creates_new_column() {
        let out = ops::tag_rows(sample(), None, &[0, 2], "Review").unwrap();
        let col = out.frame.column("_tag").unwrap().str().unwrap();
        assert_eq!(col.get(0), Some("Review"));
        assert_eq!(col.get(1), None);
    }

    #[test]
    fn tag_rows_preserves_existing_tags_on_untouched_rows() {
        let df = df!["id" => [1, 2, 3], "_tag" => [Some("Old"), None, Some("Keep")]].unwrap().lazy();
        let out = ops::tag_rows(df, None, &[1], "New").unwrap();
        let col = out.frame.column("_tag").unwrap().str().unwrap();
        assert_eq!(col.get(0), Some("Old"), "untouched row keeps its existing tag");
        assert_eq!(col.get(1), Some("New"));
        assert_eq!(col.get(2), Some("Keep"));
    }
}
