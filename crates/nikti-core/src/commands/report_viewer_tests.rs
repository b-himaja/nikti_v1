//! The Report Viewer task's own backend piece: `commands::open_report`,
//! the one command every Report Viewer instance calls regardless of
//! which workspace opened it. Exercised against a real import-produced
//! Inspection report — not a synthetic fixture — since the whole point
//! is proving actual content is readable back, not just that the
//! command signature compiles.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use uuid::Uuid;

    use crate::commands::{self, ImportSource};
    use crate::repositories::report_repository::testing::InMemoryReportRepository;
    use crate::repositories::table_repository::testing::InMemoryTableRepository;
    use crate::repositories::{AnyReport, ParquetContentStore};

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("nikti-report-viewer-test-{}", Uuid::new_v4()));
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

    #[tokio::test]
    async fn open_report_resolves_the_real_content_of_the_automatic_inspection_report_from_import() {
        let dir = TempDir::new();
        let content_store = ParquetContentStore::new(dir.path().join("content")).unwrap();
        let table_repo = InMemoryTableRepository::new();
        let report_repo = InMemoryReportRepository::new();

        let csv_path = dir.path().join("orders.csv");
        fs::write(&csv_path, "id,amount\n1,10.5\n2,20.0\n").unwrap();
        let table = commands::import_table(&table_repo, &report_repo, &content_store, ImportSource { uri: csv_path.display().to_string() }, 1_000, "tester".to_string())
            .await
            .unwrap();

        let reports = commands::open_table_reports(&report_repo, table.id()).await.unwrap();
        assert_eq!(reports.len(), 1, "the automatic Inspection report from import");
        let AnyReport::Inspection(inspection) = &reports[0] else { panic!("expected an Inspection report") };
        let report_id = inspection.report_id();

        let opened = commands::open_report(&report_repo, report_id).await.unwrap();
        let AnyReport::Inspection(opened_report) = &opened.report else { panic!("expected Inspection") };
        assert_eq!(opened_report.report_id(), report_id);
        // Real content, not a placeholder — the same content_ref the
        // metadata carries really does resolve to real, parseable JSON.
        assert!(opened.content.is_object(), "real parsed JSON content, not a stub value");
        assert!(!opened.content.as_object().unwrap().is_empty());
    }

    #[tokio::test]
    async fn open_report_rejects_an_unknown_report_id_rather_than_fabricating_empty_content() {
        let report_repo = InMemoryReportRepository::new();
        let err = commands::open_report(&report_repo, Uuid::new_v4()).await;
        assert!(err.is_err());
    }
}
