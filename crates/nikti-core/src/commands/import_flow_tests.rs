//! Demonstrates, against the real engine/type/repository-test-double
//! layer (not `todo!()`), the exact sequence a real `commands::import_table`
//! implementation must follow once repository/CSV-reading wiring lands:
//! register the Table (Amendment 10: idempotency checked on the
//! `data_fingerprint`+`schema_fingerprint` *pair*) -> append the Table
//! Event Log entry -> only then transition to `Available`, gated on that
//! append succeeding (Amendment 7) -> invoke Inspection unconditionally ->
//! register the Inspection Report — and that steps 3/4 failing never
//! undoes steps 1/2. Mirrors the precedent set by
//! `commands::transform_flow_tests`. Also carries the two Part 0
//! consistency-check tests required before any new Table Repository
//! Workspace code is written.

#[cfg(test)]
mod tests {
    use polars::prelude::*;
    use uuid::Uuid;

    use crate::engines::content::TableContent;
    use crate::engines::inspection::{ImportSummary, InspectionEngine, NativeInspectionEngine};
    use crate::engines::report::{GeneratedInspectionReport, NativeReportEngine, ReportEngine};
    use crate::repositories::report_repository::testing::InMemoryReportRepository;
    use crate::repositories::table_repository::testing::InMemoryTableRepository;
    use crate::repositories::{ReportRepository, TableRepository};
    use crate::types::common::EngineVersion;
    use crate::types::report_registration::{RegistrationOutcome, ReportRegistrationRequest};
    use crate::types::table::{ColumnDef, TableSchema};
    use crate::types::{
        AdminState, CreationSource, CreationSourceType, DescriptiveMetadata, ReportState, SchemaRef, SourceProvenance,
        SourceType, TableDataRef, TableEventLogEntry, TableRegistrationError, TableRegistrationOutcome,
        TableRegistrationRequest, TableState, VersionInfo,
    };

    fn sample_frame() -> LazyFrame {
        df!["id" => [1i64, 2, 3], "name" => ["a", "b", "c"]].unwrap().lazy()
    }

    fn sample_schema() -> TableSchema {
        TableSchema {
            columns: vec![
                ColumnDef { name: "id".to_string(), data_type: "Int64".to_string() },
                ColumnDef { name: "name".to_string(), data_type: "String".to_string() },
            ],
        }
    }

    fn registration_request(idempotency_key: &str, data_fingerprint: &str, schema_fingerprint: &str) -> TableRegistrationRequest {
        TableRegistrationRequest {
            idempotency_key: idempotency_key.to_string(),
            table_id: Uuid::new_v4(),
            creation_source: CreationSource { creation_source_type: CreationSourceType::Import, source_type: SourceType::Csv },
            source_provenance: SourceProvenance { source_label: "orders.csv".to_string(), source_project_id: None, source_dataset_id: None },
            table_data_ref: TableDataRef { content_ref: format!("table-content/{data_fingerprint}.parquet"), data_fingerprint: data_fingerprint.to_string(), row_count: 3 },
            schema_ref: SchemaRef { schema: sample_schema(), schema_fingerprint: schema_fingerprint.to_string() },
            version_info: VersionInfo { version_family_id: Uuid::new_v4() },
            descriptive_metadata: DescriptiveMetadata { table_name: "orders".to_string(), data_dictionary_id: None },
            created_at: 1_000,
            created_by: "import-command".to_string(),
        }
    }

    /// One canonical sequence (INV-023) — steps 1-3 are "the import" (Part 0
    /// / Amendment 7/10); steps 4-5 are automatic follow-on evidence
    /// generation. Returns enough state from each step for the caller to
    /// assert on.
    async fn run_import_sequence(
        table_repo: &InMemoryTableRepository,
        report_repo: &InMemoryReportRepository,
        request: TableRegistrationRequest,
    ) -> (crate::types::Table, GeneratedInspectionReport, Result<RegistrationOutcome, crate::types::RegistrationError>) {
        // 1. Register the Table (Amendment 10: idempotency on the
        // data+schema fingerprint pair). Always at `Registered` here.
        let table_id = match table_repo.register_table(request).await.unwrap() {
            TableRegistrationOutcome::Registered { table_id } => table_id,
            TableRegistrationOutcome::Idempotent { table_id } => table_id,
        };

        // 2. Append the Table Event Log entry.
        let log_entry_id = Uuid::new_v4();
        let append_result = table_repo.append_event(table_id, TableEventLogEntry::new(log_entry_id, table_id, 1_000, "imported".to_string())).await;

        // 3. Amendment 7: only transition to `Available` if the log append
        // succeeded; otherwise `Unavailable`, never `Available` without a
        // `table_event_log_id`.
        let admin_state = match &append_result {
            Ok(()) => AdminState { table_state: TableState::Available, table_event_log_id: Some(log_entry_id), archived_at: None },
            Err(_) => AdminState { table_state: TableState::Unavailable, table_event_log_id: None, archived_at: None },
        };
        table_repo.set_admin_state(table_id, admin_state).await.unwrap();
        let table = table_repo.get(table_id).await.unwrap().unwrap();

        // 4. Invoke Inspection unconditionally (INV-011), regardless of
        // whether step 3 landed on Available or Unavailable — a degraded
        // Table is still worth inspecting, and inspection never gates on
        // table_state.
        // The same fingerprint just registered on `table`, matching the
        // real sequence in `table_workspace::register_content_as_table`
        // this test documents — never an unrelated placeholder.
        let content = TableContent { table_id, content_fingerprint: table.table_data_ref().data_fingerprint.clone(), frame: sample_frame() };
        let inspection_engine = NativeInspectionEngine;
        let import_summary = ImportSummary { rows_read: 3, rows_rejected: 0, parse_error_count: 0, encoding: Some("utf-8".to_string()) };
        let results = inspection_engine.inspect(content, import_summary).unwrap();

        // 5. Build and register the Inspection Report.
        let report_engine = NativeReportEngine;
        let generated = report_engine.generate_inspection_report(
            results,
            table_id,
            Uuid::new_v4(),
            EngineVersion { rust_core: "0.1.0".to_string(), python_env_hash: None },
            "import-command".to_string(),
            1_000,
        );

        let content_bytes = serde_json::to_vec(&generated.content).unwrap();
        let request = ReportRegistrationRequest {
            report_registration_request_id: Uuid::new_v4(),
            idempotency_key: format!("{table_id}:inspection_report:{}", generated.report.content_hash()),
            report_id: generated.report.report_id(),
            report_title: generated.report.report_title().to_string(),
            report_schema_version: generated.report.report_schema_version().to_string(),
            created_at: generated.report.created_at(),
            created_by: generated.report.created_by().to_string(),
            source_context: generated.report.source_context().clone(),
            source_event_ref: generated.report.source_event_ref().clone(),
            content_format: generated.report.content_format().to_string(),
            content_hash: generated.report.content_hash().to_string(),
            content_bytes,
            knowledge_refs: generated.report.knowledge_refs().to_vec(),
            log_refs: vec![log_entry_id],
        };

        let outcome = report_repo.register_inspection_report(request).await;

        (table, generated, outcome)
    }

    #[tokio::test]
    async fn import_then_inspect_then_register_produces_one_available_report_linked_to_the_table() {
        let table_repo = InMemoryTableRepository::new();
        let report_repo = InMemoryReportRepository::new();
        let (table, generated, outcome) = run_import_sequence(&table_repo, &report_repo, registration_request("k-1", "data-fp-1", "schema-fp-1")).await;

        // Step 1-3: the import itself succeeded, is logged, and reached
        // Available only because the log entry exists (Amendment 7).
        let log = table_repo.event_log(table.id()).await.unwrap();
        assert_eq!(log.entries().len(), 1);
        assert_eq!(log.entries()[0].event(), "imported");
        assert_eq!(table.row_count(), 3);
        assert_eq!(table.table_state(), TableState::Available);
        assert_eq!(table.admin_state().table_event_log_id, Some(log.entries()[0].id()));

        // Step 4-5: exactly one Inspection Report, available, correctly
        // linked by nested source_context/source_event_ref.
        let outcome = outcome.unwrap();
        assert_eq!(outcome, RegistrationOutcome::Registered { report_id: generated.report.report_id() });
        assert_eq!(report_repo.stored_report_count(), 1);

        let stored = report_repo.get_inspection_report(generated.report.report_id()).await.unwrap().unwrap();
        assert_eq!(stored.report_state(), ReportState::Available);
        assert_eq!(stored.source_context().table_id, Some(table.id()));
        assert_eq!(stored.source_event_ref().source_engine, "inspection_engine");
        assert_eq!(stored.generated_by_engine(), "report_engine");
        assert_eq!(stored.log_refs(), &[log.entries()[0].id()]);
    }

    #[tokio::test]
    async fn a_degraded_inspection_or_registration_step_never_undoes_the_already_logged_import() {
        // Simulate registration-side failure (e.g. content store down) —
        // the Table and its Table Event Log entry were already created and
        // logged in steps 1-3, which is exactly the "degraded but
        // recoverable" guarantee this test proves.
        let table_repo = InMemoryTableRepository::new();
        let report_repo = InMemoryReportRepository::with_content_validation_failure();
        let (table, generated, outcome) = run_import_sequence(&table_repo, &report_repo, registration_request("k-2", "data-fp-2", "schema-fp-2")).await;

        let log = table_repo.event_log(table.id()).await.unwrap();
        assert_eq!(log.entries().len(), 1, "the import's own log entry is untouched by the later failure");
        assert_eq!(table.row_count(), 3, "the Table itself is untouched by the later failure");
        assert_eq!(table.table_state(), TableState::Available, "the Table's own availability is independent of a later report-registration failure");

        let outcome = outcome.unwrap();
        assert!(
            matches!(outcome, RegistrationOutcome::RegisteredButUnavailable { .. }),
            "content validation failure must surface as a degraded outcome, never be swallowed as success"
        );
        let stored = report_repo.get_inspection_report(generated.report.report_id()).await.unwrap().unwrap();
        assert_eq!(stored.report_state(), ReportState::Unavailable, "never silently Available");
    }

    /// Part 0, consistency-check item 1 (Amendment 10): a retried
    /// registration with the same idempotency key and the same
    /// data_fingerprint, but a DIFFERENT schema_fingerprint, must be
    /// rejected — never silently treated as the same Table just because
    /// the data half of the pair matched.
    #[tokio::test]
    async fn part0_idempotency_checks_the_data_and_schema_fingerprint_pair_not_data_fingerprint_alone() {
        let table_repo = InMemoryTableRepository::new();

        let first = table_repo.register_table(registration_request("retry-key", "same-data-fp", "schema-fp-a")).await.unwrap();
        let TableRegistrationOutcome::Registered { table_id: first_id } = first else { panic!("expected Registered") };

        // Same key, same pair -> genuinely idempotent, no new Table.
        let repeat = table_repo.register_table(registration_request("retry-key", "same-data-fp", "schema-fp-a")).await.unwrap();
        assert_eq!(repeat, TableRegistrationOutcome::Idempotent { table_id: first_id });
        assert_eq!(table_repo.list().await.unwrap().len(), 1, "an identical retry must not create a second Table");

        // Same key, same data_fingerprint, DIFFERENT schema_fingerprint ->
        // rejected outright, not silently deduplicated on data alone.
        let conflict = table_repo.register_table(registration_request("retry-key", "same-data-fp", "schema-fp-b")).await;
        assert_eq!(conflict, Err(TableRegistrationError::FingerprintPairConflict("retry-key".to_string())));
        assert_eq!(table_repo.list().await.unwrap().len(), 1, "a rejected conflicting retry must not create a second Table either");
    }

    /// Part 0, consistency-check item 2 (Amendment 7): if Table Event Log
    /// creation fails during registration, the Table must never reach
    /// `Available` — it stays `Registered`/becomes `Unavailable`, with no
    /// `table_event_log_id`.
    #[tokio::test]
    async fn part0_table_never_reaches_available_without_a_table_event_log_id() {
        let table_repo = InMemoryTableRepository::with_event_append_failure();
        let report_repo = InMemoryReportRepository::new();
        let (table, _generated, _outcome) = run_import_sequence(&table_repo, &report_repo, registration_request("k-3", "data-fp-3", "schema-fp-3")).await;

        assert_ne!(table.table_state(), TableState::Available, "must never reach Available when the log append failed");
        assert_eq!(table.table_state(), TableState::Unavailable);
        assert_eq!(table.admin_state().table_event_log_id, None);
    }
}
