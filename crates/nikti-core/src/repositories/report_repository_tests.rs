#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::repositories::report_repository::testing::InMemoryReportRepository;
    use crate::repositories::ReportRepository;
    use crate::types::common::EngineVersion;
    use crate::types::report_registration::{SourceContext, SourceContextKind, SourceEventRef};
    use crate::types::{
        RegistrationError, RegistrationOutcome, ReportRegistrationRequest, ReportState, ValidationReportRegistrationRequest,
    };

    fn request_with(idempotency_key: &str, content: &[u8]) -> ReportRegistrationRequest {
        let hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(content);
            hex::encode(hasher.finalize())
        };
        ReportRegistrationRequest {
            report_registration_request_id: Uuid::new_v4(),
            idempotency_key: idempotency_key.to_string(),
            report_id: Uuid::new_v4(),
            report_title: "Inspection Report — test".to_string(),
            report_schema_version: "1.0".to_string(),
            created_at: 1_000,
            created_by: "test-user".to_string(),
            source_context: SourceContext { source_context_type: SourceContextKind::Table, table_id: Some(Uuid::new_v4()), project_dataset_id: None, project_id: None },
            source_event_ref: SourceEventRef {
                source_event_type: "table_inspection".to_string(),
                source_execution_id: Uuid::new_v4(),
                source_engine: "inspection_engine".to_string(),
                source_engine_version: EngineVersion { rust_core: "0.1.0".to_string(), python_env_hash: None },
                source_execution_status: "completed".to_string(),
            },
            content_format: "application/json".to_string(),
            content_hash: hash,
            content_bytes: content.to_vec(),
            knowledge_refs: Vec::new(),
            log_refs: Vec::new(),
        }
    }

    #[tokio::test]
    async fn a_new_registration_reaches_available() {
        let repo = InMemoryReportRepository::new();
        let request = request_with("key-1", b"content-a");
        let report_id = request.report_id;
        let outcome = repo.register_inspection_report(request).await.unwrap();
        assert_eq!(outcome, RegistrationOutcome::Registered { report_id });

        let stored = repo.get_inspection_report(report_id).await.unwrap().unwrap();
        assert_eq!(stored.report_state(), ReportState::Available);
        assert!(stored.repository_registered_at().is_some());
    }

    #[tokio::test]
    async fn retry_with_same_key_and_same_hash_is_idempotent_not_a_duplicate() {
        let repo = InMemoryReportRepository::new();
        let first = request_with("key-2", b"content-b");
        let first_id = first.report_id;
        repo.register_inspection_report(first).await.unwrap();

        // A retry with a fresh report_id but the same idempotency_key and
        // the same content — must return the *existing* report_id, never
        // create a second Report.
        let mut retry = request_with("key-2", b"content-b");
        retry.report_id = Uuid::new_v4();
        let outcome = repo.register_inspection_report(retry).await.unwrap();
        assert_eq!(outcome, RegistrationOutcome::Idempotent { report_id: first_id });
        assert_eq!(repo.stored_report_count(), 1, "no duplicate Report was created");
    }

    #[tokio::test]
    async fn retry_with_same_key_and_different_hash_is_rejected() {
        let repo = InMemoryReportRepository::new();
        let first = request_with("key-3", b"content-c");
        repo.register_inspection_report(first).await.unwrap();

        let mut retry = request_with("key-3", b"content-DIFFERENT");
        retry.report_id = Uuid::new_v4();
        let err = repo.register_inspection_report(retry).await.unwrap_err();
        assert_eq!(err, RegistrationError::IdempotencyKeyConflict("key-3".to_string()));
    }

    #[tokio::test]
    async fn incomplete_request_is_rejected() {
        let repo = InMemoryReportRepository::new();
        let mut request = request_with("key-4", b"content-d");
        request.report_title = String::new();
        let err = repo.register_inspection_report(request).await.unwrap_err();
        assert!(matches!(err, RegistrationError::IncompleteRequest(_)));
    }

    #[tokio::test]
    async fn content_hash_mismatch_is_rejected() {
        let repo = InMemoryReportRepository::new();
        let mut request = request_with("key-5", b"content-e");
        request.content_hash = "not-the-real-hash".to_string();
        let err = repo.register_inspection_report(request).await.unwrap_err();
        assert_eq!(err, RegistrationError::ContentHashMismatch);
    }

    #[tokio::test]
    async fn content_validation_failure_yields_unavailable_never_available() {
        let repo = InMemoryReportRepository::with_content_validation_failure();
        let request = request_with("key-6", b"content-f");
        let report_id = request.report_id;
        let outcome = repo.register_inspection_report(request).await.unwrap();
        assert!(matches!(outcome, RegistrationOutcome::RegisteredButUnavailable { .. }));

        let stored = repo.get_inspection_report(report_id).await.unwrap().unwrap();
        assert_eq!(stored.report_state(), ReportState::Unavailable, "confirmed-unreadable content -> Unavailable, never Available");
    }

    #[tokio::test]
    async fn index_write_failure_stays_registered_never_available() {
        let repo = InMemoryReportRepository::with_index_write_failure();
        let request = request_with("key-7", b"content-g");
        let report_id = request.report_id;
        let outcome = repo.register_inspection_report(request).await.unwrap();
        assert!(matches!(outcome, RegistrationOutcome::RegisteredButUnavailable { .. }));

        let stored = repo.get_inspection_report(report_id).await.unwrap().unwrap();
        assert_eq!(
            stored.report_state(),
            ReportState::Registered,
            "index failure alone (content readable) stays Registered, not Unavailable, and never Available"
        );
    }

    #[tokio::test]
    async fn source_table_reference_attach_failure_never_affects_availability() {
        let repo = InMemoryReportRepository::with_source_reference_attach_failure();
        let request = request_with("key-8", b"content-h");
        let report_id = request.report_id;
        let outcome = repo.register_inspection_report(request).await.unwrap();
        assert_eq!(outcome, RegistrationOutcome::Registered { report_id });

        let stored = repo.get_inspection_report(report_id).await.unwrap().unwrap();
        assert_eq!(stored.report_state(), ReportState::Available, "a sync-only failure never blocks availability");
        assert_eq!(repo.source_reference_attach_failure_count(), 1);
        assert_eq!(repo.source_reference_attachment_count(), 0);
        assert_eq!(repo.stored_report_count(), 1, "not duplicated");
    }

    #[tokio::test]
    async fn missing_source_table_disables_navigation_only_never_report_state() {
        let repo = InMemoryReportRepository::new();
        let request = request_with("key-9", b"content-i");
        let report_id = request.report_id;
        let table_id = request.source_context.table_id.unwrap();
        repo.register_inspection_report(request).await.unwrap();

        assert!(repo.is_source_navigable(report_id).await.unwrap());
        repo.mark_source_table_unavailable(table_id);

        assert!(!repo.is_source_navigable(report_id).await.unwrap(), "source Table is gone -> navigation disabled");
        let stored = repo.get_inspection_report(report_id).await.unwrap().unwrap();
        assert_eq!(
            stored.report_state(),
            ReportState::Available,
            "B.4: a missing *source* never changes report_state — only missing *content* may"
        );
    }

    // ===== Part D: the same algorithm, applied to Validation Reports =====

    fn validation_request_with(idempotency_key: &str, content: &[u8]) -> ValidationReportRegistrationRequest {
        let hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(content);
            hex::encode(hasher.finalize())
        };
        ValidationReportRegistrationRequest {
            report_registration_request_id: Uuid::new_v4(),
            idempotency_key: idempotency_key.to_string(),
            report_id: Uuid::new_v4(),
            report_title: "Validation Report — test".to_string(),
            report_schema_version: "1.0".to_string(),
            report_template_version: "1.0".to_string(),
            created_at: 1_000,
            created_by: "test-user".to_string(),
            source_context: SourceContext { source_context_type: SourceContextKind::Table, table_id: Some(Uuid::new_v4()), project_dataset_id: None, project_id: None },
            source_event_ref: SourceEventRef {
                source_event_type: "table_validation".to_string(),
                source_execution_id: Uuid::new_v4(),
                source_engine: "validation_engine".to_string(),
                source_engine_version: EngineVersion { rust_core: "0.1.0".to_string(), python_env_hash: None },
                source_execution_status: "completed".to_string(),
            },
            source_validation_result_id: Uuid::new_v4(),
            execution_id: Uuid::new_v4(),
            content_format: "application/json".to_string(),
            content_hash: hash,
            content_bytes: content.to_vec(),
            knowledge_refs: vec![Uuid::new_v4()],
            log_refs: Vec::new(),
        }
    }

    #[tokio::test]
    async fn validation_report_a_new_registration_reaches_available() {
        let repo = InMemoryReportRepository::new();
        let request = validation_request_with("v-key-1", b"content-a");
        let report_id = request.report_id;
        let outcome = repo.register_validation_report(request).await.unwrap();
        assert_eq!(outcome, RegistrationOutcome::Registered { report_id });

        let stored = repo.get_validation_report(report_id).await.unwrap().unwrap();
        assert_eq!(stored.report_state(), ReportState::Available);
    }

    #[tokio::test]
    async fn validation_report_retry_with_same_key_and_same_hash_is_idempotent() {
        let repo = InMemoryReportRepository::new();
        let first = validation_request_with("v-key-2", b"content-b");
        let first_id = first.report_id;
        repo.register_validation_report(first).await.unwrap();

        let mut retry = validation_request_with("v-key-2", b"content-b");
        retry.report_id = Uuid::new_v4();
        let outcome = repo.register_validation_report(retry).await.unwrap();
        assert_eq!(outcome, RegistrationOutcome::Idempotent { report_id: first_id });
        assert_eq!(repo.stored_report_count(), 1);
    }

    #[tokio::test]
    async fn validation_report_content_validation_failure_yields_unavailable_never_available() {
        let repo = InMemoryReportRepository::with_content_validation_failure();
        let request = validation_request_with("v-key-3", b"content-c");
        let report_id = request.report_id;
        let outcome = repo.register_validation_report(request).await.unwrap();
        assert!(matches!(outcome, RegistrationOutcome::RegisteredButUnavailable { .. }), "no partial report ever registered as available");

        let stored = repo.get_validation_report(report_id).await.unwrap().unwrap();
        assert_eq!(stored.report_state(), ReportState::Unavailable);
    }

    #[tokio::test]
    async fn validation_and_inspection_reports_do_not_collide_in_the_same_repository() {
        let repo = InMemoryReportRepository::new();
        let inspection_request = request_with("shared-namespace-1", b"inspection-content");
        let validation_request = validation_request_with("shared-namespace-2", b"validation-content");
        repo.register_inspection_report(inspection_request).await.unwrap();
        repo.register_validation_report(validation_request).await.unwrap();
        assert_eq!(repo.stored_report_count(), 2);
    }

    // ===== Analysis Reports: the same algorithm, same request shape as
    // Inspection's (`AnalysisReport` adds no extra fields — see its doc
    // comment), `report_type = "analysis_report"`. =====

    fn analysis_request_with(idempotency_key: &str, content: &[u8]) -> ReportRegistrationRequest {
        let mut r = request_with(idempotency_key, content);
        r.report_title = "Analysis Report — test".to_string();
        r.source_event_ref.source_event_type = "table_analysis".to_string();
        r.source_event_ref.source_engine = "analysis_engine".to_string();
        r.knowledge_refs = vec![Uuid::new_v4()];
        r
    }

    #[tokio::test]
    async fn analysis_report_a_new_registration_reaches_available() {
        let repo = InMemoryReportRepository::new();
        let request = analysis_request_with("a-key-1", b"content-a");
        let report_id = request.report_id;
        let outcome = repo.register_analysis_report(request).await.unwrap();
        assert_eq!(outcome, RegistrationOutcome::Registered { report_id });

        let stored = repo.get_analysis_report(report_id).await.unwrap().unwrap();
        assert_eq!(stored.report_state(), ReportState::Available);
    }

    #[tokio::test]
    async fn analysis_report_retry_with_same_key_and_same_hash_is_idempotent() {
        let repo = InMemoryReportRepository::new();
        let first = analysis_request_with("a-key-2", b"content-b");
        let first_id = first.report_id;
        repo.register_analysis_report(first).await.unwrap();

        let mut retry = analysis_request_with("a-key-2", b"content-b");
        retry.report_id = Uuid::new_v4();
        let outcome = repo.register_analysis_report(retry).await.unwrap();
        assert_eq!(outcome, RegistrationOutcome::Idempotent { report_id: first_id });
        assert_eq!(repo.stored_report_count(), 1);
    }

    #[tokio::test]
    async fn analysis_report_content_validation_failure_yields_unavailable_never_available() {
        let repo = InMemoryReportRepository::with_content_validation_failure();
        let request = analysis_request_with("a-key-3", b"content-c");
        let report_id = request.report_id;
        let outcome = repo.register_analysis_report(request).await.unwrap();
        assert!(matches!(outcome, RegistrationOutcome::RegisteredButUnavailable { .. }), "no partial report ever registered as available");

        let stored = repo.get_analysis_report(report_id).await.unwrap().unwrap();
        assert_eq!(stored.report_state(), ReportState::Unavailable);
    }

    #[tokio::test]
    async fn all_three_report_kinds_coexist_without_collision() {
        let repo = InMemoryReportRepository::new();
        repo.register_inspection_report(request_with("tri-1", b"inspection")).await.unwrap();
        repo.register_validation_report(validation_request_with("tri-2", b"validation")).await.unwrap();
        repo.register_analysis_report(analysis_request_with("tri-3", b"analysis")).await.unwrap();
        assert_eq!(repo.stored_report_count(), 3);
    }
}
