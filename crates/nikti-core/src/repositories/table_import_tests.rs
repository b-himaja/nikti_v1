#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use uuid::Uuid;

    use crate::repositories::table_import::{data_fingerprint, read_csv, schema_fingerprint, ParquetContentStore};

    /// A fresh, isolated scratch directory per test — cleaned up on drop
    /// so repeated runs never see stale content-store files.
    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("nikti-import-test-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_csv(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn reading_a_real_csv_produces_real_row_and_column_counts_and_a_malformed_row_count() {
        let dir = TempDir::new();
        // Row 3 is ragged (missing a field) — a genuine malformed row.
        let path = write_csv(&dir, "orders.csv", "id,name,amount\n1,alice,10.5\n2,bob,20.0\n3,carol\n4,dave,40.0\n");

        let outcome = read_csv(&path).unwrap();

        assert_eq!(outcome.column_count, 3);
        assert_eq!(outcome.malformed_row_count, 1, "exactly row 3 is ragged");
        // Polars, told to ignore parse errors, still loads a row for the
        // ragged line (padding the missing field with null) — the point
        // of this test is that the malformed count is genuine and
        // separate, not that the row vanishes.
        assert!(outcome.row_count >= 3);
        assert!(!outcome.schema_fingerprint.is_empty());
        assert!(!outcome.data_fingerprint.is_empty());
    }

    #[test]
    fn schema_fingerprint_is_identical_for_identical_structure_with_different_data() {
        let dir = TempDir::new();
        let a = write_csv(&dir, "a.csv", "id,name\n1,alice\n2,bob\n");
        let b = write_csv(&dir, "b.csv", "id,name\n9,zack\n8,yara\n7,xena\n");

        let outcome_a = read_csv(&a).unwrap();
        let outcome_b = read_csv(&b).unwrap();

        assert_eq!(outcome_a.schema_fingerprint, outcome_b.schema_fingerprint, "same structure -> same schema fingerprint");
        assert_ne!(outcome_a.data_fingerprint, outcome_b.data_fingerprint, "different cell content -> different data fingerprint");
    }

    #[test]
    fn schema_fingerprint_differs_when_column_order_differs_even_with_identical_data_values() {
        let dir = TempDir::new();
        let a = write_csv(&dir, "a.csv", "id,name\n1,alice\n2,bob\n");
        let b = write_csv(&dir, "b.csv", "name,id\nalice,1\nbob,2\n");

        let outcome_a = read_csv(&a).unwrap();
        let outcome_b = read_csv(&b).unwrap();

        assert_ne!(outcome_a.schema_fingerprint, outcome_b.schema_fingerprint, "column order is part of the schema, never silently normalized away");
    }

    #[test]
    fn data_fingerprint_is_stable_across_repeated_reads_of_unchanged_input() {
        let dir = TempDir::new();
        let path = write_csv(&dir, "a.csv", "id,name\n1,alice\n2,bob\n");

        let first = read_csv(&path).unwrap();
        let second = read_csv(&path).unwrap();

        assert_eq!(first.data_fingerprint, second.data_fingerprint);
        assert_eq!(first.schema_fingerprint, second.schema_fingerprint);
    }

    #[test]
    fn content_store_deduplicates_identical_content_regardless_of_source_filename() {
        let dir = TempDir::new();
        let store = ParquetContentStore::new(dir.path().join("content")).unwrap();

        let a = write_csv(&dir, "orders_january.csv", "id,name\n1,alice\n2,bob\n");
        let b = write_csv(&dir, "orders_copy.csv", "id,name\n1,alice\n2,bob\n");

        let outcome_a = read_csv(&a).unwrap();
        let outcome_b = read_csv(&b).unwrap();
        assert_eq!(outcome_a.data_fingerprint, outcome_b.data_fingerprint, "identical content must fingerprint identically regardless of filename");

        let mut frame_a = outcome_a.frame;
        let ref_a = store.write_if_absent(&outcome_a.data_fingerprint, &mut frame_a).unwrap();

        let parquet_dir = dir.path().join("content");
        let file_count_after_first_write = fs::read_dir(&parquet_dir).unwrap().count();

        let mut frame_b = outcome_b.frame;
        let ref_b = store.write_if_absent(&outcome_b.data_fingerprint, &mut frame_b).unwrap();

        let file_count_after_second_write = fs::read_dir(&parquet_dir).unwrap().count();

        assert_eq!(ref_a, ref_b, "both point at the same content-addressed file");
        assert_eq!(file_count_after_first_write, file_count_after_second_write, "no second Parquet file was written for content already stored");
        assert_eq!(file_count_after_first_write, 1);
    }

    #[test]
    fn schema_fingerprint_helper_matches_read_csvs_own_computation() {
        let dir = TempDir::new();
        let path = write_csv(&dir, "a.csv", "id,name\n1,alice\n");
        let outcome = read_csv(&path).unwrap();
        assert_eq!(schema_fingerprint(&outcome.frame.schema()), outcome.schema_fingerprint);
        assert_eq!(data_fingerprint(&outcome.frame).unwrap(), outcome.data_fingerprint);
    }
}
