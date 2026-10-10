//! The real import pipeline, exercised end to end against genuine files
//! on disk — not synthetic fixture `Table`/`TableContent` values, which
//! is all every prior task (Table Repository DI/consistency-check,
//! Inspection Engine) ever had to test against. See this task's Output
//! for which of those earlier "passing" tests turn out to have only ever
//! proven the state machine/engine logic itself, never the real
//! CSV-to-evidence chain.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use uuid::Uuid;

    use crate::commands::table_workspace::load_into_project;
    use crate::commands::{self, ImportSource};
    use crate::repositories::report_repository::testing::InMemoryReportRepository;
    use crate::repositories::table_repository::testing::InMemoryTableRepository;
    use crate::repositories::{ParquetContentStore, TableRepository};
    use crate::types::{Project, ReportState, TableState};

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("nikti-import-flow-test-{}", Uuid::new_v4()));
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

    fn write_csv(dir: &TempDir, name: &str, content: &str) -> String {
        let path = dir.path().join(name);
        fs::write(&path, content).unwrap();
        path.display().to_string()
    }

    struct Harness {
        _dir: TempDir,
        table_repo: InMemoryTableRepository,
        report_repo: InMemoryReportRepository,
        content_store: ParquetContentStore,
    }

    impl Harness {
        fn new() -> Self {
            let dir = TempDir::new();
            let content_store = ParquetContentStore::new(dir.path().join("content")).unwrap();
            Self { _dir: dir, table_repo: InMemoryTableRepository::new(), report_repo: InMemoryReportRepository::new(), content_store }
        }
    }

    #[tokio::test]
    async fn importing_a_real_fixture_csv_produces_real_non_fabricated_values() {
        let harness = Harness::new();
        let dir = TempDir::new();
        // Row with id=3 is ragged (missing the amount field) — a genuine
        // malformed row, not simulated.
        let uri = write_csv(&dir, "orders.csv", "id,name,amount\n1,alice,10.5\n2,bob,20.0\n3,carol\n4,dave,40.0\n");

        let table = commands::import_table(&harness.table_repo, &harness.report_repo, &harness.content_store, ImportSource { uri: uri.clone() }, 1_000, "tester".to_string())
            .await
            .unwrap();

        // Cross-checked directly against the same file read independently
        // (Part A/B) — not values this test invented.
        let outcome = crate::repositories::table_import::read_csv(std::path::Path::new(&uri)).unwrap();
        assert_eq!(table.row_count(), outcome.row_count);
        assert_eq!(table.schema().columns.len(), outcome.column_count as usize);
        assert_eq!(table.table_data_ref().data_fingerprint, outcome.data_fingerprint);
        assert_eq!(table.schema_ref().schema_fingerprint, outcome.schema_fingerprint);
        assert_eq!(outcome.malformed_row_count, 1, "row 3 is genuinely malformed — this is what feeds ImportSummary.rows_rejected inside import_table");
        assert_eq!(table.table_state(), TableState::Available);
    }

    #[tokio::test]
    async fn reimporting_the_identical_file_twice_hits_idempotency_not_duplication() {
        let harness = Harness::new();
        let dir = TempDir::new();
        let uri = write_csv(&dir, "orders.csv", "id,name\n1,alice\n2,bob\n");

        let first = commands::import_table(&harness.table_repo, &harness.report_repo, &harness.content_store, ImportSource { uri: uri.clone() }, 1_000, "tester".to_string()).await.unwrap();
        let second = commands::import_table(&harness.table_repo, &harness.report_repo, &harness.content_store, ImportSource { uri: uri.clone() }, 2_000, "tester".to_string()).await.unwrap();

        assert_eq!(first.id(), second.id(), "the same table_id must be returned, not a new one");
        assert_eq!(harness.table_repo.list().await.unwrap().len(), 1, "no duplicate Table record");

        let parquet_files = fs::read_dir(harness._dir.path().join("content")).unwrap().count();
        assert_eq!(parquet_files, 1, "no duplicate Parquet file");
    }

    #[tokio::test]
    async fn identical_schema_different_data_share_a_schema_fingerprint_but_not_a_data_fingerprint() {
        let harness = Harness::new();
        let dir = TempDir::new();
        let uri_a = write_csv(&dir, "a.csv", "id,name\n1,alice\n2,bob\n");
        let uri_b = write_csv(&dir, "b.csv", "id,name\n9,zack\n8,yara\n7,xena\n");

        let table_a = commands::import_table(&harness.table_repo, &harness.report_repo, &harness.content_store, ImportSource { uri: uri_a }, 1_000, "tester".to_string()).await.unwrap();
        let table_b = commands::import_table(&harness.table_repo, &harness.report_repo, &harness.content_store, ImportSource { uri: uri_b }, 1_000, "tester".to_string()).await.unwrap();

        assert_eq!(table_a.schema_ref().schema_fingerprint, table_b.schema_ref().schema_fingerprint);
        assert_ne!(table_a.table_data_ref().data_fingerprint, table_b.table_data_ref().data_fingerprint);
    }

    #[tokio::test]
    async fn identical_data_different_column_order_is_a_genuine_schema_difference() {
        let harness = Harness::new();
        let dir = TempDir::new();
        let uri_a = write_csv(&dir, "a.csv", "id,name\n1,alice\n2,bob\n");
        let uri_b = write_csv(&dir, "b.csv", "name,id\nalice,1\nbob,2\n");

        let table_a = commands::import_table(&harness.table_repo, &harness.report_repo, &harness.content_store, ImportSource { uri: uri_a }, 1_000, "tester".to_string()).await.unwrap();
        let table_b = commands::import_table(&harness.table_repo, &harness.report_repo, &harness.content_store, ImportSource { uri: uri_b }, 1_000, "tester".to_string()).await.unwrap();

        assert_ne!(table_a.schema_ref().schema_fingerprint, table_b.schema_ref().schema_fingerprint, "column order must never be silently normalized away");
    }

    #[tokio::test]
    async fn the_content_store_deduplicates_across_different_source_filenames() {
        let harness = Harness::new();
        let dir = TempDir::new();
        let uri_a = write_csv(&dir, "orders_january.csv", "id,name\n1,alice\n2,bob\n");
        let uri_b = write_csv(&dir, "orders_copy.csv", "id,name\n1,alice\n2,bob\n");

        // Different filenames -> different idempotency_key -> two Table
        // records — but Part C's dedup is about the Parquet file, not the
        // Table record.
        let table_a = commands::import_table(&harness.table_repo, &harness.report_repo, &harness.content_store, ImportSource { uri: uri_a }, 1_000, "tester".to_string()).await.unwrap();
        let table_b = commands::import_table(&harness.table_repo, &harness.report_repo, &harness.content_store, ImportSource { uri: uri_b }, 1_000, "tester".to_string()).await.unwrap();

        assert_ne!(table_a.id(), table_b.id(), "two distinct Table records, one per source filename");
        assert_eq!(table_a.table_data_ref().content_ref, table_b.table_data_ref().content_ref, "both point at the SAME Parquet file — content-addressed, not filename-addressed");

        let parquet_files = fs::read_dir(harness._dir.path().join("content")).unwrap().count();
        assert_eq!(parquet_files, 1, "only one Parquet file was ever written for this content");
    }

    #[tokio::test]
    async fn reimport_as_new_produces_a_new_table_id_and_leaves_the_original_and_its_project_references_untouched() {
        let harness = Harness::new();
        let dir = TempDir::new();
        let original_uri = write_csv(&dir, "orders.csv", "id,name\n1,alice\n2,bob\n");
        let original = commands::import_table(&harness.table_repo, &harness.report_repo, &harness.content_store, ImportSource { uri: original_uri }, 1_000, "tester".to_string())
            .await
            .unwrap();

        // An existing Project reference to the original Table.
        let project = Project::new(Uuid::new_v4(), "p".to_string(), 500);
        let existing_dataset = load_into_project(&project, &original, Uuid::new_v4(), 1_500);
        assert_eq!(existing_dataset.source_table_id(), Some(original.id()));

        let reimport_uri = write_csv(&dir, "orders_v2.csv", "id,name\n1,alice\n2,bob\n3,carol\n");
        let reimported = commands::reimport_table_as_new(&harness.table_repo, &harness.report_repo, &harness.content_store, original.id(), ImportSource { uri: reimport_uri }, 2_000, "tester".to_string())
            .await
            .unwrap();

        assert_ne!(reimported.id(), original.id(), "reimport must always produce a new table_id");
        assert_eq!(reimported.version_info().version_family_id, original.version_info().version_family_id, "linked via version_family_id, per Part F");

        let original_after = harness.table_repo.get(original.id()).await.unwrap().unwrap();
        assert_eq!(original_after.row_count(), original.row_count());
        assert_eq!(original_after.table_data_ref().data_fingerprint, original.table_data_ref().data_fingerprint);
        assert_eq!(original_after.table_state(), TableState::Available);

        assert_eq!(existing_dataset.source_table_id(), Some(original.id()), "existing Project reference is completely unaffected");
        assert_eq!(harness.table_repo.list().await.unwrap().len(), 2, "both the original and the reimported Table now exist independently");
    }

    #[tokio::test]
    async fn end_to_end_one_table_one_log_entry_one_available_inspection_report_from_a_real_file() {
        let harness = Harness::new();
        let dir = TempDir::new();
        let uri = write_csv(&dir, "orders.csv", "id,name,amount\n1,alice,10.5\n2,bob,20.0\n3,carol,30.0\n");

        let table = commands::import_table(&harness.table_repo, &harness.report_repo, &harness.content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();

        assert_eq!(harness.table_repo.list().await.unwrap().len(), 1);

        let log = harness.table_repo.event_log(table.id()).await.unwrap();
        assert_eq!(log.entries().len(), 1);
        assert_eq!(log.entries()[0].event(), "imported");
        assert_eq!(table.admin_state().table_event_log_id, Some(log.entries()[0].id()));

        let reports = crate::commands::table_workspace::open_table_reports(&harness.report_repo, table.id()).await.unwrap();
        assert_eq!(reports.len(), 1, "exactly one report exists for this Table");
        match &reports[0] {
            crate::repositories::AnyReport::Inspection(report) => {
                assert_eq!(report.report_state(), ReportState::Available);
                assert_eq!(report.source_context().table_id, Some(table.id()));

                // Not just "a report exists" — what the report's own
                // stored content actually says. This is the exact gap
                // the status audit flagged: a real end-to-end test that
                // never checked this field is why the Inspection
                // fingerprint-fabrication bug shipped past it.
                let content_bytes = harness.report_repo.get_content_bytes(report.content_ref()).expect("content must be readable at its own content_ref");
                let content: serde_json::Value = serde_json::from_slice(&content_bytes).unwrap();
                let reported_fingerprint = content["dataset_identity"]["content_fingerprint"].as_str().unwrap();
                assert_eq!(reported_fingerprint, table.table_data_ref().data_fingerprint, "the Inspection Report must carry the Table's real, registered fingerprint, never a fabricated one");
                assert!(!reported_fingerprint.starts_with("unfingerprinted:"), "must never regress to the old fabrication shape");
                let evidence_trail_fingerprint = content["evidence_trail"]["content_fingerprint"].as_str().unwrap();
                assert_eq!(evidence_trail_fingerprint, table.table_data_ref().data_fingerprint);
            }
            other => panic!("expected an Inspection report, got {other:?}"),
        }
    }
}
