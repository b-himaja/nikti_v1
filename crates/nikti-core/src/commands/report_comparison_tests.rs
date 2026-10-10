//! `commands::generate_comparison_report` — real reports, real content,
//! real diffs. Every assertion checks a specific, known difference (or
//! known absence of one), never just "the call succeeded."

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use uuid::Uuid;

    use crate::commands::{self, CommandError, ImportSource};
    use crate::engines::ReportEngineError;
    use crate::repositories::report_repository::testing::InMemoryReportRepository;
    use crate::repositories::table_repository::testing::InMemoryTableRepository;
    use crate::repositories::{AnyReport, ParquetContentStore, ReportRepository};
    use crate::types::common::EngineVersion;
    use crate::types::report_registration::{SourceContext, SourceContextKind, SourceEventRef};
    use crate::types::{ComparisonOutcome, RegistrationOutcome, ReportRegistrationRequest};

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("nikti-comparison-test-{}", Uuid::new_v4()));
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

    fn write_csv(dir: &std::path::Path, name: &str, content: &str) -> String {
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        path.display().to_string()
    }

    /// Same hashing `ReportRepository`'s own registration algorithm uses
    /// internally (`content_hash_of`, private to that module) — needed
    /// here only to build a valid `ReportRegistrationRequest` fixture for
    /// a scenario (a cross-type / schema-mismatched pair) this task's own
    /// real code paths don't otherwise produce.
    fn sha256_hex(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hex::encode(hasher.finalize())
    }

    fn only_inspection_report_id(reports: &[AnyReport]) -> Uuid {
        let AnyReport::Inspection(r) = &reports[0] else { panic!("expected an Inspection report") };
        r.report_id()
    }

    async fn extract_content(h: &Harness, outcome: RegistrationOutcome) -> crate::types::ComparisonContent {
        let RegistrationOutcome::Registered { report_id } = outcome else { panic!("expected Registered, got {outcome:?}") };
        let report = h.report_repo.get_comparison_report(report_id).await.unwrap().unwrap();
        let bytes = h.report_repo.get_content_bytes(report.content_ref()).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// Real reports (two imports with deliberately different, known
    /// content — different row counts), real content, a real structural
    /// diff — never a hand-constructed fixture standing in for it.
    #[tokio::test]
    async fn two_same_kind_reports_with_known_differences_produce_a_correct_diff() {
        let h = Harness::new();
        let uri_a = write_csv(h._dir.path(), "a.csv", "id,amount\n1,10.5\n2,20.0\n");
        let table_a = commands::import_table(&h.table_repo, &h.report_repo, &h.content_store, ImportSource { uri: uri_a }, 1_000, "tester".to_string()).await.unwrap();
        let uri_b = write_csv(h._dir.path(), "b.csv", "id,amount\n1,10.5\n2,20.0\n3,30.0\n");
        let table_b = commands::import_table(&h.table_repo, &h.report_repo, &h.content_store, ImportSource { uri: uri_b }, 1_000, "tester".to_string()).await.unwrap();

        let reports_a = commands::open_table_reports(&h.report_repo, table_a.id()).await.unwrap();
        let reports_b = commands::open_table_reports(&h.report_repo, table_b.id()).await.unwrap();
        let report_a_id = only_inspection_report_id(&reports_a);
        let report_b_id = only_inspection_report_id(&reports_b);

        let outcome = commands::generate_comparison_report(&h.report_repo, report_a_id, report_b_id, 2_000, "tester".to_string()).await.unwrap();
        let content = extract_content(&h, outcome).await;

        // The command layer normalizes comparison order internally (by
        // report id) so that comparing (A,B) and (B,A) is genuinely the
        // same comparison — so which input ends up as `report_a_id`
        // versus `report_b_id` in the stored content isn't guaranteed to
        // match the caller's argument order. What IS guaranteed: the
        // pair of ids is exactly the two real inputs.
        let mut stored_pair = [content.report_a_id, content.report_b_id];
        stored_pair.sort();
        let mut input_pair = [report_a_id, report_b_id];
        input_pair.sort();
        assert_eq!(stored_pair, input_pair);
        let ComparisonOutcome::Diffed(entries) = content.outcome else { panic!("expected Diffed, got {:?}", content.outcome) };
        assert!(!entries.is_empty(), "table_a has 2 rows, table_b has 3 — the real Inspection content must differ somewhere");
    }

    /// Part A, enforced with a real, specific, typed rejection — not a
    /// warning, not a degraded/empty comparison.
    #[tokio::test]
    async fn a_cross_type_pair_is_rejected_with_a_specific_error() {
        let h = Harness::new();
        let uri = write_csv(h._dir.path(), "a.csv", "id,amount\n1,10.5\n");
        let table = commands::import_table(&h.table_repo, &h.report_repo, &h.content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();
        let inspection_reports = commands::open_table_reports(&h.report_repo, table.id()).await.unwrap();
        let inspection_id = only_inspection_report_id(&inspection_reports);

        // A real Analysis report of a genuinely different kind,
        // registered through the real registration path (Analysis
        // reuses the generic `ReportRegistrationRequest` shape, same as
        // Inspection — no report-kind-specific extra fields, matching
        // `AnalysisReport`'s own doc comment).
        let analysis_id = Uuid::new_v4();
        let request = ReportRegistrationRequest {
            report_registration_request_id: Uuid::new_v4(),
            idempotency_key: "cross-type-test:analysis".to_string(),
            report_id: analysis_id,
            report_title: "Analysis Report — cross-type test".to_string(),
            report_schema_version: "1.0".to_string(),
            created_at: 1_000,
            created_by: "tester".to_string(),
            source_context: SourceContext { source_context_type: SourceContextKind::Table, table_id: Some(table.id()), project_dataset_id: None, project_id: None },
            source_event_ref: SourceEventRef {
                source_event_type: "table_analysis".to_string(),
                source_execution_id: Uuid::new_v4(),
                source_engine: "analysis_engine".to_string(),
                source_engine_version: EngineVersion { rust_core: "0.1.0".to_string(), python_env_hash: None },
                source_execution_status: "completed".to_string(),
            },
            content_format: "application/json".to_string(),
            content_hash: sha256_hex(b"{}"),
            content_bytes: b"{}".to_vec(),
            knowledge_refs: vec![],
            log_refs: vec![],
        };
        h.report_repo.register_analysis_report(request).await.unwrap();

        let err = commands::generate_comparison_report(&h.report_repo, inspection_id, analysis_id, 2_000, "tester".to_string()).await;
        assert!(matches!(err, Err(CommandError::Report(ReportEngineError::MismatchedReportKinds))), "got {err:?}");
    }

    /// Audit #3's "source data missing at generation time" failure mode
    /// for Comparison, checked directly against `generate_comparison_report`'s
    /// real body: a nonexistent `report_id` on either side must be
    /// rejected with a clean error, never produce a partial/fabricated
    /// comparison. This path already existed (`.ok_or(CommandError::NotFound)?`
    /// on both fetches) but had no test — confirmed here for real, and
    /// confirmed no Comparison report is registered as a side effect of
    /// the rejected attempt.
    #[tokio::test]
    async fn generate_comparison_report_rejects_an_unknown_report_id_rather_than_fabricating_a_partial_comparison() {
        let h = Harness::new();
        let uri = write_csv(h._dir.path(), "a.csv", "id,amount\n1,10.5\n");
        let table = commands::import_table(&h.table_repo, &h.report_repo, &h.content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();
        let inspection_reports = commands::open_table_reports(&h.report_repo, table.id()).await.unwrap();
        let inspection_id = only_inspection_report_id(&inspection_reports);

        let err = commands::generate_comparison_report(&h.report_repo, inspection_id, Uuid::new_v4(), 2_000, "tester".to_string()).await;
        assert!(matches!(err, Err(CommandError::NotFound)), "got {err:?}");

        let comparisons = h.report_repo.list_by_kind(crate::types::common::ReportKind::Comparison).await.unwrap();
        assert!(comparisons.is_empty(), "a rejected precondition must never leave a partial Comparison report behind");
    }

    /// Audit #3 found Comparison had no kind-specific test proving what
    /// happens when its OWN report registration fails content
    /// validation — same gap, same fix shape as Lineage's
    /// `a_degraded_lineage_report_registration_is_surfaced_as_unavailable_never_silently_available`.
    /// Note: `a_schema_version_mismatch_is_surfaced_honestly_not_silently_diffed`
    /// above is NOT this test in disguise — a schema mismatch is a
    /// successfully completed comparison with informative content
    /// (`ComparisonOutcome::SchemaVersionMismatch`, registered as
    /// `Available`, already covered). The only real path to
    /// `Unavailable` is the registration layer itself, exercised here.
    #[tokio::test]
    async fn a_degraded_comparison_report_registration_is_surfaced_as_unavailable_never_silently_available() {
        let dir = TempDir::new();
        let content_store = ParquetContentStore::new(dir.path().join("content")).unwrap();
        let table_repo = InMemoryTableRepository::new();
        let report_repo = InMemoryReportRepository::new();

        // The two INPUT Inspection reports must register successfully —
        // `generate_comparison_report` reads their real content
        // (`fetch_report_content`) before it ever gets to registering
        // the Comparison itself, so failing content validation from the
        // start would break the inputs, not exercise what this test is
        // actually after. Only flip the failure on once both inputs are
        // genuinely `Available`.
        let uri_a = write_csv(dir.path(), "a.csv", "id,amount\n1,10.5\n2,20.0\n");
        let table_a = commands::import_table(&table_repo, &report_repo, &content_store, ImportSource { uri: uri_a }, 1_000, "tester".to_string()).await.unwrap();
        let uri_b = write_csv(dir.path(), "b.csv", "id,amount\n1,10.5\n");
        let table_b = commands::import_table(&table_repo, &report_repo, &content_store, ImportSource { uri: uri_b }, 1_500, "tester".to_string()).await.unwrap();
        let inspection_a = only_inspection_report_id(&commands::open_table_reports(&report_repo, table_a.id()).await.unwrap());
        let inspection_b = only_inspection_report_id(&commands::open_table_reports(&report_repo, table_b.id()).await.unwrap());

        report_repo.set_fail_content_validation(true);
        let outcome = commands::generate_comparison_report(&report_repo, inspection_a, inspection_b, 2_000, "tester".to_string()).await.unwrap();
        let RegistrationOutcome::RegisteredButUnavailable { reason, .. } = outcome else { panic!("expected RegisteredButUnavailable, got {outcome:?}") };
        assert!(!reason.is_empty(), "a real reason, not a silent degradation");

        let reports = report_repo.list_by_kind(crate::types::common::ReportKind::Comparison).await.unwrap();
        assert_eq!(reports.len(), 1);
        let AnyReport::Comparison(comparison) = &reports[0] else { panic!("expected Comparison") };
        assert_eq!(comparison.report_state(), crate::types::report_registration::ReportState::Unavailable, "content validation failure must surface as degraded, never as success");
    }

    /// Comparing a report against itself: identical content, a clean
    /// empty diff, and idempotent on repeat (same report_id both times).
    #[tokio::test]
    async fn comparing_a_report_against_itself_produces_a_clean_empty_diff_and_is_idempotent() {
        let h = Harness::new();
        let uri = write_csv(h._dir.path(), "a.csv", "id,amount\n1,10.5\n2,20.0\n");
        let table = commands::import_table(&h.table_repo, &h.report_repo, &h.content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();
        let reports = commands::open_table_reports(&h.report_repo, table.id()).await.unwrap();
        let report_id = only_inspection_report_id(&reports);

        let first = commands::generate_comparison_report(&h.report_repo, report_id, report_id, 2_000, "tester".to_string()).await.unwrap();
        let content = extract_content(&h, first.clone()).await;
        assert_eq!(content.outcome, ComparisonOutcome::Diffed(vec![]), "a report compared with itself has zero real differences");

        let RegistrationOutcome::Registered { report_id: first_comparison_id } = first else { panic!("expected Registered") };
        let second = commands::generate_comparison_report(&h.report_repo, report_id, report_id, 5_000, "tester".to_string()).await.unwrap();
        assert_eq!(second, RegistrationOutcome::Idempotent { report_id: first_comparison_id });
    }

    /// Order-independence: comparing (A,B) and (B,A) is the same
    /// comparison — same idempotency key, same report_id.
    #[tokio::test]
    async fn comparing_the_same_pair_in_either_order_is_idempotent() {
        let h = Harness::new();
        let uri_a = write_csv(h._dir.path(), "a.csv", "id,amount\n1,10.5\n");
        let table_a = commands::import_table(&h.table_repo, &h.report_repo, &h.content_store, ImportSource { uri: uri_a }, 1_000, "tester".to_string()).await.unwrap();
        let uri_b = write_csv(h._dir.path(), "b.csv", "id,amount\n1,10.5\n2,20.0\n");
        let table_b = commands::import_table(&h.table_repo, &h.report_repo, &h.content_store, ImportSource { uri: uri_b }, 1_000, "tester".to_string()).await.unwrap();
        let report_a_id = only_inspection_report_id(&commands::open_table_reports(&h.report_repo, table_a.id()).await.unwrap());
        let report_b_id = only_inspection_report_id(&commands::open_table_reports(&h.report_repo, table_b.id()).await.unwrap());

        let first = commands::generate_comparison_report(&h.report_repo, report_a_id, report_b_id, 2_000, "tester".to_string()).await.unwrap();
        let RegistrationOutcome::Registered { report_id: first_id } = first else { panic!("expected Registered") };
        let second = commands::generate_comparison_report(&h.report_repo, report_b_id, report_a_id, 3_000, "tester".to_string()).await.unwrap();
        assert_eq!(second, RegistrationOutcome::Idempotent { report_id: first_id });
    }

    /// Part C: a same-kind pair with incompatible schema versions is
    /// surfaced honestly, not silently diffed as if the shapes lined up.
    #[tokio::test]
    async fn a_schema_version_mismatch_is_surfaced_honestly_not_silently_diffed() {
        let h = Harness::new();
        let content_bytes = b"{\"measurements\":{}}".to_vec();
        let content_hash = sha256_hex(&content_bytes);

        let make_request = |report_id: Uuid, schema_version: &str| ReportRegistrationRequest {
            report_registration_request_id: Uuid::new_v4(),
            idempotency_key: format!("schema-mismatch-test:{report_id}"),
            report_id,
            report_title: "Inspection Report — schema mismatch test".to_string(),
            report_schema_version: schema_version.to_string(),
            created_at: 1_000,
            created_by: "tester".to_string(),
            source_context: SourceContext { source_context_type: SourceContextKind::Table, table_id: None, project_dataset_id: None, project_id: None },
            source_event_ref: SourceEventRef {
                source_event_type: "table_inspection".to_string(),
                source_execution_id: Uuid::new_v4(),
                source_engine: "inspection_engine".to_string(),
                source_engine_version: EngineVersion { rust_core: "0.1.0".to_string(), python_env_hash: None },
                source_execution_status: "completed".to_string(),
            },
            content_format: "application/json".to_string(),
            content_hash: content_hash.clone(),
            content_bytes: content_bytes.clone(),
            knowledge_refs: vec![],
            log_refs: vec![],
        };

        // The command layer normalizes comparison order by report id
        // (see `generate_comparison_report`'s own doc comment) — sort
        // these two explicitly so this test's expected
        // schema-version-per-slot assertion doesn't depend on the
        // random ordering of two freshly-generated UUIDs.
        let mut ids = [Uuid::new_v4(), Uuid::new_v4()];
        ids.sort();
        let [id_a, id_b] = ids;
        h.report_repo.register_inspection_report(make_request(id_a, "1.0")).await.unwrap();
        h.report_repo.register_inspection_report(make_request(id_b, "2.0")).await.unwrap();

        let outcome = commands::generate_comparison_report(&h.report_repo, id_a, id_b, 2_000, "tester".to_string()).await.unwrap();
        let content = extract_content(&h, outcome).await;
        assert_eq!(content.outcome, ComparisonOutcome::SchemaVersionMismatch { report_a_schema_version: "1.0".to_string(), report_b_schema_version: "2.0".to_string() });
    }

    /// INV-015: `ComparisonReport::new` is constructed in exactly the
    /// two legitimate places every other report kind's own object is —
    /// the Report Engine and its registration algorithm (see the
    /// identically-shaped test in `report_lineage_tests.rs`).
    #[test]
    fn comparison_report_is_only_ever_constructed_by_the_report_engine_and_its_registration_algorithm() {
        fn visit(dir: &std::path::Path, hits: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).expect("directory must exist") {
                let path = entry.expect("readable directory entry").path();
                if path.is_dir() {
                    visit(&path, hits);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.ends_with("_tests.rs") || name == "report_comparison.rs" {
                    continue;
                }
                let source = std::fs::read_to_string(&path).unwrap();
                if source.contains("ComparisonReport::new(") {
                    hits.push(path.display().to_string());
                }
            }
        }

        let src_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut hits = Vec::new();
        visit(std::path::Path::new(src_dir), &mut hits);
        hits.sort();
        let mut expected = vec![
            concat!(env!("CARGO_MANIFEST_DIR"), "/src/engines/report.rs").to_string(),
            concat!(env!("CARGO_MANIFEST_DIR"), "/src/repositories/report_repository.rs").to_string(),
        ];
        expected.sort();
        assert_eq!(hits, expected, "exactly these two legitimate call sites — got {hits:?}");
    }
}
