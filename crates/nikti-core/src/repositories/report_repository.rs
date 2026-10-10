use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::types::common::{ReportId, TableId};
use crate::types::{
    AnalysisReport, AuditReport, ComparisonReport, InspectionReport, LineageReport, RegistrationError, RegistrationOutcome, ReportKind,
    ReportRegistrationRequest, ReportState, ValidationReport, ValidationReportRegistrationRequest,
};

use super::errors::RepositoryError;

/// A stored report of any subtype. Read-only wrapper — matches Report's
/// Category A contract; nothing here exposes a write path.
#[derive(Debug, Serialize)]
pub enum AnyReport {
    Inspection(InspectionReport),
    Validation(ValidationReport),
    Analysis(AnalysisReport),
    Audit(AuditReport),
    Lineage(LineageReport),
    Comparison(ComparisonReport),
}

/// INV-004/015: reports are insert-only once constructed by the Report
/// Engine. `register_inspection_report`/`register_validation_report` are
/// the Report Object Model's variant of that — see B.3's algorithm on
/// `register_inspection_report`'s doc comment — everything else keeps
/// the simpler placeholder shape, out of scope for this task.
pub trait ReportRepository {
    /// Audit's variant of `register_inspection_report` — same algorithm.
    /// Replaces the old bare `insert_audit` (never implemented, carried
    /// no idempotency or state-machine semantics at all) — same fix the
    /// Lineage Report task already made for `insert_lineage`.
    async fn register_audit_report(
        &self,
        request: ReportRegistrationRequest,
    ) -> Result<RegistrationOutcome, RegistrationError>;

    async fn get_audit_report(&self, id: ReportId) -> Result<Option<AuditReport>, RepositoryError>;

    /// Report Repository Integration Contract v1.0, B.3's nine-step
    /// sequence:
    /// 1-2 (caller/engine side): `content_hash` + `idempotency_key`
    ///     already computed, carried on `request`.
    /// 3. Validate the request (incomplete / hash mismatch -> `Err`).
    /// 4. Idempotency check: same key + same hash -> return the existing
    ///    `report_id` (`Idempotent`), never a duplicate; same key +
    ///    different hash -> `Err(IdempotencyKeyConflict)`.
    /// 5. Create the Report record at `Registered`.
    /// 6. Validate `content_ref`/`content_hash` are actually loadable.
    /// 7. Index the Report's metadata.
    /// 8. Only if 6 and 7 both succeed: transition to `Available`.
    /// 9. Attempt to attach a source-Table reference — its failure never
    ///    affects steps 1-8's outcome (B.3 point 9 / B.4).
    ///
    /// No partial Report ever reaches `Available`: if 6 fails the Report
    /// becomes `Unavailable` (confirmed-unreadable content); if 7 fails
    /// alone it stays `Registered` (not confirmed unreadable, just not
    /// yet indexed) — either way `RegisteredButUnavailable` is returned,
    /// never `Registered { .. }` silently treated as usable.
    async fn register_inspection_report(
        &self,
        request: ReportRegistrationRequest,
    ) -> Result<RegistrationOutcome, RegistrationError>;

    async fn get_inspection_report(&self, id: ReportId) -> Result<Option<InspectionReport>, RepositoryError>;

    /// Validation's variant of `register_inspection_report` — the exact
    /// same nine-step algorithm (Part D: "call into the same registration
    /// path with report_type = 'validation_report'"), applied to
    /// `ValidationReport` instead. See `register_inspection_report`'s doc
    /// comment for the step-by-step sequence; not repeated here.
    async fn register_validation_report(
        &self,
        request: ValidationReportRegistrationRequest,
    ) -> Result<RegistrationOutcome, RegistrationError>;

    async fn get_validation_report(&self, id: ReportId) -> Result<Option<ValidationReport>, RepositoryError>;

    /// Analysis's variant of `register_inspection_report` — same
    /// algorithm, same request shape (`AnalysisReport` has no extra
    /// fields beyond the shared baseline, per Part F's doc comment on
    /// `AnalysisReport`), `report_type = "analysis_report"`.
    async fn register_analysis_report(
        &self,
        request: ReportRegistrationRequest,
    ) -> Result<RegistrationOutcome, RegistrationError>;

    async fn get_analysis_report(&self, id: ReportId) -> Result<Option<AnalysisReport>, RepositoryError>;

    /// Lineage's variant of `register_inspection_report` — same
    /// algorithm, same request shape (`LineageReport` has no extra
    /// fields beyond the shared baseline, same as `AnalysisReport`).
    /// Replaces the old bare `insert_lineage` (never implemented,
    /// carried no idempotency or state-machine semantics at all) — the
    /// Lineage Report task requires the same idempotent, state-machine-
    /// gated flow every other report kind gets.
    async fn register_lineage_report(
        &self,
        request: ReportRegistrationRequest,
    ) -> Result<RegistrationOutcome, RegistrationError>;

    async fn get_lineage_report(&self, id: ReportId) -> Result<Option<LineageReport>, RepositoryError>;

    /// Comparison's variant of `register_inspection_report` — same
    /// algorithm. `ComparisonReport` has two extra fields beyond the
    /// shared baseline (`compared_report_a_id`/`compared_report_b_id`),
    /// same precedent `ValidationReport` already established, so this
    /// still reuses the generic `ReportRegistrationRequest` — the two
    /// extra fields are threaded through by the command layer, not
    /// carried on the request.
    async fn register_comparison_report(
        &self,
        request: ReportRegistrationRequest,
        compared_report_a_id: ReportId,
        compared_report_b_id: ReportId,
    ) -> Result<RegistrationOutcome, RegistrationError>;

    async fn get_comparison_report(&self, id: ReportId) -> Result<Option<ComparisonReport>, RepositoryError>;

    /// B.4: a missing/purged *source Table* disables navigation to that
    /// source only — it must never, by itself, change `report_state`.
    async fn is_source_navigable(&self, id: ReportId) -> Result<bool, RepositoryError>;

    async fn get(&self, id: ReportId) -> Result<Option<AnyReport>, RepositoryError>;

    async fn list_by_kind(&self, kind: ReportKind) -> Result<Vec<AnyReport>, RepositoryError>;

    /// Reads back a Report's actual stored content bytes given the
    /// `content_ref` from its `ReportBaseline` — the piece the Report
    /// Viewer task found missing from this trait: `get()` above resolves
    /// only metadata, never the JSON body a report's `content_ref`
    /// points at. Read-only, same as every other method here — no
    /// `ReportRepository` method ever writes content after registration
    /// (INV-004).
    async fn get_content(&self, content_ref: &str) -> Result<Vec<u8>, RepositoryError>;
}

fn content_hash_of(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Test/dev in-memory reference implementation. NOT the frozen "Embedded
/// relational store (Rust-native)" tech decision — that real
/// sqlx+SQLite-backed implementation is separate, later work. This exists
/// so B.3's stateful algorithm (idempotency, state transitions, degraded-
/// but-recoverable failure handling) is genuinely exercised against real
/// logic rather than asserted only in prose, matching the precedent set
/// by `commands::transform_flow_tests` testing real engine/type logic
/// ahead of real repository wiring.
pub mod testing {
    use super::*;

    #[derive(Default)]
    struct Inner {
        inspection_reports: HashMap<ReportId, InspectionReport>,
        validation_reports: HashMap<ReportId, ValidationReport>,
        analysis_reports: HashMap<ReportId, AnalysisReport>,
        lineage_reports: HashMap<ReportId, LineageReport>,
        comparison_reports: HashMap<ReportId, ComparisonReport>,
        audit_reports: HashMap<ReportId, AuditReport>,
        idempotency_index: HashMap<String, (ReportId, String)>,
        content_store: HashMap<String, Vec<u8>>,
        indexed_report_ids: HashSet<ReportId>,
        unavailable_source_tables: HashSet<TableId>,
        source_reference_attachments: Vec<ReportId>,
        source_reference_attach_failures: Vec<ReportId>,
    }

    /// The report-kind-agnostic outcome of steps 3-8 of B.3's algorithm —
    /// idempotency dedup, hash/content validation, index write, and the
    /// resulting state transition. `register_inspection_report` and
    /// `register_validation_report` each construct their own typed Report
    /// object around this, so the bug-prone stateful logic exists exactly
    /// once regardless of how many report kinds this repository grows to
    /// support.
    enum CoreOutcome {
        Idempotent(ReportId),
        Unavailable,
        StaysRegistered,
        Available,
    }

    fn run_registration_core(
        inner: &mut Inner,
        fail_content_validation: bool,
        fail_index_write: bool,
        report_id: ReportId,
        idempotency_key: &str,
        content_hash: &str,
        content_bytes: &[u8],
        content_ref: String,
    ) -> Result<CoreOutcome, RegistrationError> {
        if let Some((existing_id, existing_hash)) = inner.idempotency_index.get(idempotency_key).cloned() {
            return if existing_hash == content_hash {
                Ok(CoreOutcome::Idempotent(existing_id))
            } else {
                Err(RegistrationError::IdempotencyKeyConflict(idempotency_key.to_string()))
            };
        }
        inner.idempotency_index.insert(idempotency_key.to_string(), (report_id, content_hash.to_string()));

        if fail_content_validation {
            return Ok(CoreOutcome::Unavailable);
        }
        inner.content_store.insert(content_ref, content_bytes.to_vec());

        if fail_index_write {
            return Ok(CoreOutcome::StaysRegistered);
        }
        inner.indexed_report_ids.insert(report_id);
        Ok(CoreOutcome::Available)
    }

    fn validate_request_fields(report_title: &str, content_hash: &str, content_bytes: &[u8]) -> Result<(), RegistrationError> {
        if report_title.is_empty() {
            return Err(RegistrationError::IncompleteRequest("report_title".to_string()));
        }
        if content_hash.is_empty() || content_bytes.is_empty() {
            return Err(RegistrationError::MissingContentRef);
        }
        if content_hash_of(content_bytes) != content_hash {
            return Err(RegistrationError::ContentHashMismatch);
        }
        Ok(())
    }

    #[derive(Default)]
    pub struct InMemoryReportRepository {
        inner: Mutex<Inner>,
        // Interior-mutable (not a plain `bool`, unlike the sibling
        // `fail_index_write`/`fail_source_reference_attach` flags below):
        // a Comparison-report Unavailable-transition test needs its two
        // INPUT Inspection reports to register successfully first (real,
        // fetchable content) and only the Comparison registration itself
        // to fail — `set_fail_content_validation` lets one repo instance
        // do both in sequence, added for the Audit #3 cleanup task's
        // Part A. See `report_comparison_tests.rs`'s
        // `a_degraded_comparison_report_registration_...` test.
        fail_content_validation: AtomicBool,
        fail_index_write: bool,
        fail_source_reference_attach: bool,
    }

    impl InMemoryReportRepository {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn with_content_validation_failure() -> Self {
            Self { fail_content_validation: AtomicBool::new(true), ..Self::default() }
        }

        pub fn set_fail_content_validation(&self, value: bool) {
            self.fail_content_validation.store(value, Ordering::SeqCst);
        }

        pub fn with_index_write_failure() -> Self {
            Self { fail_index_write: true, ..Self::default() }
        }

        pub fn with_source_reference_attach_failure() -> Self {
            Self { fail_source_reference_attach: true, ..Self::default() }
        }

        pub fn mark_source_table_unavailable(&self, table_id: TableId) {
            self.inner.lock().unwrap().unavailable_source_tables.insert(table_id);
        }

        pub fn source_reference_attachment_count(&self) -> usize {
            self.inner.lock().unwrap().source_reference_attachments.len()
        }

        pub fn source_reference_attach_failure_count(&self) -> usize {
            self.inner.lock().unwrap().source_reference_attach_failures.len()
        }

        pub fn stored_report_count(&self) -> usize {
            let inner = self.inner.lock().unwrap();
            inner.inspection_reports.len() + inner.validation_reports.len() + inner.analysis_reports.len()
        }

        /// Reads back the actual stored content bytes for a `content_ref`
        /// — lets a test assert on what a report's own content genuinely
        /// says, not just that the report exists and is `Available`. See
        /// `commands::table_import_flow_tests`'s end-to-end test, which
        /// deserializes this to check `content_fingerprint` is the real
        /// one, not a fabricated placeholder.
        pub fn get_content_bytes(&self, content_ref: &str) -> Option<Vec<u8>> {
            self.inner.lock().unwrap().content_store.get(content_ref).cloned()
        }

        fn note_source_reference_attach(&self, inner: &mut Inner, report_id: ReportId) {
            if self.fail_source_reference_attach {
                inner.source_reference_attach_failures.push(report_id);
            } else {
                inner.source_reference_attachments.push(report_id);
            }
        }
    }

    impl ReportRepository for InMemoryReportRepository {
        async fn register_audit_report(
            &self,
            request: ReportRegistrationRequest,
        ) -> Result<RegistrationOutcome, RegistrationError> {
            validate_request_fields(&request.report_title, &request.content_hash, &request.content_bytes)?;
            let content_ref = format!("audit-report-content/{}.json", request.content_hash);

            let mut inner = self.inner.lock().unwrap();
            let core = run_registration_core(
                &mut inner,
                self.fail_content_validation.load(Ordering::SeqCst),
                self.fail_index_write,
                request.report_id,
                &request.idempotency_key,
                &request.content_hash,
                &request.content_bytes,
                content_ref.clone(),
            )?;
            if let CoreOutcome::Idempotent(report_id) = core {
                return Ok(RegistrationOutcome::Idempotent { report_id });
            }

            let registered = AuditReport::new(
                request.report_id,
                request.report_title.clone(),
                ReportState::Registered,
                request.report_schema_version.clone(),
                request.created_at,
                request.created_by.clone(),
                request.source_context.clone(),
                request.source_event_ref.clone(),
                content_ref,
                request.content_format.clone(),
                request.content_hash.clone(),
                None,
                request.knowledge_refs.clone(),
                request.log_refs.clone(),
            );

            let (final_report, outcome) = match core {
                CoreOutcome::Unavailable => (
                    registered.with_state(ReportState::Unavailable, None),
                    Ok(RegistrationOutcome::RegisteredButUnavailable { report_id: request.report_id, reason: "content_ref failed load/validation".to_string() }),
                ),
                CoreOutcome::StaysRegistered => (
                    registered.clone(),
                    Ok(RegistrationOutcome::RegisteredButUnavailable { report_id: request.report_id, reason: "metadata index write failed".to_string() }),
                ),
                CoreOutcome::Available => {
                    (registered.with_state(ReportState::Available, Some(request.created_at)), Ok(RegistrationOutcome::Registered { report_id: request.report_id }))
                }
                CoreOutcome::Idempotent(_) => unreachable!(),
            };
            inner.audit_reports.insert(request.report_id, final_report);

            if matches!(outcome, Ok(RegistrationOutcome::Registered { .. })) {
                self.note_source_reference_attach(&mut inner, request.report_id);
            }
            outcome
        }

        async fn get_audit_report(&self, id: ReportId) -> Result<Option<AuditReport>, RepositoryError> {
            Ok(self.inner.lock().unwrap().audit_reports.get(&id).cloned())
        }

        async fn register_inspection_report(
            &self,
            request: ReportRegistrationRequest,
        ) -> Result<RegistrationOutcome, RegistrationError> {
            validate_request_fields(&request.report_title, &request.content_hash, &request.content_bytes)?;
            let content_ref = format!("inspection-report-content/{}.json", request.content_hash);

            let mut inner = self.inner.lock().unwrap();
            let core = run_registration_core(
                &mut inner,
                self.fail_content_validation.load(Ordering::SeqCst),
                self.fail_index_write,
                request.report_id,
                &request.idempotency_key,
                &request.content_hash,
                &request.content_bytes,
                content_ref.clone(),
            )?;
            if let CoreOutcome::Idempotent(report_id) = core {
                return Ok(RegistrationOutcome::Idempotent { report_id });
            }

            let registered = InspectionReport::new(
                request.report_id,
                request.report_title.clone(),
                ReportState::Registered,
                request.report_schema_version.clone(),
                request.created_at,
                request.created_by.clone(),
                request.source_context.clone(),
                request.source_event_ref.clone(),
                content_ref,
                request.content_format.clone(),
                request.content_hash.clone(),
                None,
                request.knowledge_refs.clone(),
                request.log_refs.clone(),
            );

            let (final_report, outcome) = match core {
                CoreOutcome::Unavailable => (
                    registered.with_state(ReportState::Unavailable, None),
                    Ok(RegistrationOutcome::RegisteredButUnavailable { report_id: request.report_id, reason: "content_ref failed load/validation".to_string() }),
                ),
                CoreOutcome::StaysRegistered => (
                    registered.clone(),
                    Ok(RegistrationOutcome::RegisteredButUnavailable { report_id: request.report_id, reason: "metadata index write failed".to_string() }),
                ),
                CoreOutcome::Available => {
                    (registered.with_state(ReportState::Available, Some(request.created_at)), Ok(RegistrationOutcome::Registered { report_id: request.report_id }))
                }
                CoreOutcome::Idempotent(_) => unreachable!(),
            };
            inner.inspection_reports.insert(request.report_id, final_report);

            if matches!(outcome, Ok(RegistrationOutcome::Registered { .. })) {
                self.note_source_reference_attach(&mut inner, request.report_id);
            }
            outcome
        }

        async fn get_inspection_report(&self, id: ReportId) -> Result<Option<InspectionReport>, RepositoryError> {
            Ok(self.inner.lock().unwrap().inspection_reports.get(&id).cloned())
        }

        async fn register_validation_report(
            &self,
            request: ValidationReportRegistrationRequest,
        ) -> Result<RegistrationOutcome, RegistrationError> {
            validate_request_fields(&request.report_title, &request.content_hash, &request.content_bytes)?;
            let content_ref = format!("validation-report-content/{}.json", request.content_hash);

            let mut inner = self.inner.lock().unwrap();
            let core = run_registration_core(
                &mut inner,
                self.fail_content_validation.load(Ordering::SeqCst),
                self.fail_index_write,
                request.report_id,
                &request.idempotency_key,
                &request.content_hash,
                &request.content_bytes,
                content_ref.clone(),
            )?;
            if let CoreOutcome::Idempotent(report_id) = core {
                return Ok(RegistrationOutcome::Idempotent { report_id });
            }

            let registered = ValidationReport::new(
                request.report_id,
                request.report_title.clone(),
                ReportState::Registered,
                request.report_schema_version.clone(),
                request.report_template_version.clone(),
                request.created_at,
                request.created_by.clone(),
                request.source_context.clone(),
                request.source_event_ref.clone(),
                request.source_validation_result_id,
                request.execution_id,
                content_ref,
                request.content_format.clone(),
                request.content_hash.clone(),
                None,
                request.knowledge_refs.clone(),
                request.log_refs.clone(),
            );

            let (final_report, outcome) = match core {
                CoreOutcome::Unavailable => (
                    registered.with_state(ReportState::Unavailable, None),
                    Ok(RegistrationOutcome::RegisteredButUnavailable { report_id: request.report_id, reason: "content_ref failed load/validation".to_string() }),
                ),
                CoreOutcome::StaysRegistered => (
                    registered.clone(),
                    Ok(RegistrationOutcome::RegisteredButUnavailable { report_id: request.report_id, reason: "metadata index write failed".to_string() }),
                ),
                CoreOutcome::Available => {
                    (registered.with_state(ReportState::Available, Some(request.created_at)), Ok(RegistrationOutcome::Registered { report_id: request.report_id }))
                }
                CoreOutcome::Idempotent(_) => unreachable!(),
            };
            inner.validation_reports.insert(request.report_id, final_report);

            if matches!(outcome, Ok(RegistrationOutcome::Registered { .. })) {
                self.note_source_reference_attach(&mut inner, request.report_id);
            }
            outcome
        }

        async fn get_validation_report(&self, id: ReportId) -> Result<Option<ValidationReport>, RepositoryError> {
            Ok(self.inner.lock().unwrap().validation_reports.get(&id).cloned())
        }

        async fn register_analysis_report(
            &self,
            request: ReportRegistrationRequest,
        ) -> Result<RegistrationOutcome, RegistrationError> {
            validate_request_fields(&request.report_title, &request.content_hash, &request.content_bytes)?;
            let content_ref = format!("analysis-report-content/{}.json", request.content_hash);

            let mut inner = self.inner.lock().unwrap();
            let core = run_registration_core(
                &mut inner,
                self.fail_content_validation.load(Ordering::SeqCst),
                self.fail_index_write,
                request.report_id,
                &request.idempotency_key,
                &request.content_hash,
                &request.content_bytes,
                content_ref.clone(),
            )?;
            if let CoreOutcome::Idempotent(report_id) = core {
                return Ok(RegistrationOutcome::Idempotent { report_id });
            }

            let registered = AnalysisReport::new(
                request.report_id,
                request.report_title.clone(),
                ReportState::Registered,
                request.report_schema_version.clone(),
                request.created_at,
                request.created_by.clone(),
                request.source_context.clone(),
                request.source_event_ref.clone(),
                content_ref,
                request.content_format.clone(),
                request.content_hash.clone(),
                None,
                request.knowledge_refs.clone(),
                request.log_refs.clone(),
            );

            let (final_report, outcome) = match core {
                CoreOutcome::Unavailable => (
                    registered.with_state(ReportState::Unavailable, None),
                    Ok(RegistrationOutcome::RegisteredButUnavailable { report_id: request.report_id, reason: "content_ref failed load/validation".to_string() }),
                ),
                CoreOutcome::StaysRegistered => (
                    registered.clone(),
                    Ok(RegistrationOutcome::RegisteredButUnavailable { report_id: request.report_id, reason: "metadata index write failed".to_string() }),
                ),
                CoreOutcome::Available => {
                    (registered.with_state(ReportState::Available, Some(request.created_at)), Ok(RegistrationOutcome::Registered { report_id: request.report_id }))
                }
                CoreOutcome::Idempotent(_) => unreachable!(),
            };
            inner.analysis_reports.insert(request.report_id, final_report);

            if matches!(outcome, Ok(RegistrationOutcome::Registered { .. })) {
                self.note_source_reference_attach(&mut inner, request.report_id);
            }
            outcome
        }

        async fn get_analysis_report(&self, id: ReportId) -> Result<Option<AnalysisReport>, RepositoryError> {
            Ok(self.inner.lock().unwrap().analysis_reports.get(&id).cloned())
        }

        async fn register_lineage_report(
            &self,
            request: ReportRegistrationRequest,
        ) -> Result<RegistrationOutcome, RegistrationError> {
            validate_request_fields(&request.report_title, &request.content_hash, &request.content_bytes)?;
            let content_ref = format!("lineage-report-content/{}.json", request.content_hash);

            let mut inner = self.inner.lock().unwrap();
            let core = run_registration_core(
                &mut inner,
                self.fail_content_validation.load(Ordering::SeqCst),
                self.fail_index_write,
                request.report_id,
                &request.idempotency_key,
                &request.content_hash,
                &request.content_bytes,
                content_ref.clone(),
            )?;
            if let CoreOutcome::Idempotent(report_id) = core {
                return Ok(RegistrationOutcome::Idempotent { report_id });
            }

            let registered = LineageReport::new(
                request.report_id,
                request.report_title.clone(),
                ReportState::Registered,
                request.report_schema_version.clone(),
                request.created_at,
                request.created_by.clone(),
                request.source_context.clone(),
                request.source_event_ref.clone(),
                content_ref,
                request.content_format.clone(),
                request.content_hash.clone(),
                None,
                request.knowledge_refs.clone(),
                request.log_refs.clone(),
            );

            let (final_report, outcome) = match core {
                CoreOutcome::Unavailable => (
                    registered.with_state(ReportState::Unavailable, None),
                    Ok(RegistrationOutcome::RegisteredButUnavailable { report_id: request.report_id, reason: "content_ref failed load/validation".to_string() }),
                ),
                CoreOutcome::StaysRegistered => (
                    registered.clone(),
                    Ok(RegistrationOutcome::RegisteredButUnavailable { report_id: request.report_id, reason: "metadata index write failed".to_string() }),
                ),
                CoreOutcome::Available => {
                    (registered.with_state(ReportState::Available, Some(request.created_at)), Ok(RegistrationOutcome::Registered { report_id: request.report_id }))
                }
                CoreOutcome::Idempotent(_) => unreachable!(),
            };
            inner.lineage_reports.insert(request.report_id, final_report);

            if matches!(outcome, Ok(RegistrationOutcome::Registered { .. })) {
                self.note_source_reference_attach(&mut inner, request.report_id);
            }
            outcome
        }

        async fn get_lineage_report(&self, id: ReportId) -> Result<Option<LineageReport>, RepositoryError> {
            Ok(self.inner.lock().unwrap().lineage_reports.get(&id).cloned())
        }

        async fn register_comparison_report(
            &self,
            request: ReportRegistrationRequest,
            compared_report_a_id: ReportId,
            compared_report_b_id: ReportId,
        ) -> Result<RegistrationOutcome, RegistrationError> {
            validate_request_fields(&request.report_title, &request.content_hash, &request.content_bytes)?;
            let content_ref = format!("comparison-report-content/{}.json", request.content_hash);

            let mut inner = self.inner.lock().unwrap();
            let core = run_registration_core(
                &mut inner,
                self.fail_content_validation.load(Ordering::SeqCst),
                self.fail_index_write,
                request.report_id,
                &request.idempotency_key,
                &request.content_hash,
                &request.content_bytes,
                content_ref.clone(),
            )?;
            if let CoreOutcome::Idempotent(report_id) = core {
                return Ok(RegistrationOutcome::Idempotent { report_id });
            }

            let registered = ComparisonReport::new(
                request.report_id,
                request.report_title.clone(),
                ReportState::Registered,
                request.report_schema_version.clone(),
                request.created_at,
                request.created_by.clone(),
                request.source_context.clone(),
                request.source_event_ref.clone(),
                content_ref,
                request.content_format.clone(),
                request.content_hash.clone(),
                None,
                request.knowledge_refs.clone(),
                request.log_refs.clone(),
                compared_report_a_id,
                compared_report_b_id,
            );

            let (final_report, outcome) = match core {
                CoreOutcome::Unavailable => (
                    registered.with_state(ReportState::Unavailable, None),
                    Ok(RegistrationOutcome::RegisteredButUnavailable { report_id: request.report_id, reason: "content_ref failed load/validation".to_string() }),
                ),
                CoreOutcome::StaysRegistered => (
                    registered.clone(),
                    Ok(RegistrationOutcome::RegisteredButUnavailable { report_id: request.report_id, reason: "metadata index write failed".to_string() }),
                ),
                CoreOutcome::Available => {
                    (registered.with_state(ReportState::Available, Some(request.created_at)), Ok(RegistrationOutcome::Registered { report_id: request.report_id }))
                }
                CoreOutcome::Idempotent(_) => unreachable!(),
            };
            inner.comparison_reports.insert(request.report_id, final_report);

            if matches!(outcome, Ok(RegistrationOutcome::Registered { .. })) {
                self.note_source_reference_attach(&mut inner, request.report_id);
            }
            outcome
        }

        async fn get_comparison_report(&self, id: ReportId) -> Result<Option<ComparisonReport>, RepositoryError> {
            Ok(self.inner.lock().unwrap().comparison_reports.get(&id).cloned())
        }

        async fn is_source_navigable(&self, id: ReportId) -> Result<bool, RepositoryError> {
            let inner = self.inner.lock().unwrap();
            let table_id = if let Some(report) = inner.inspection_reports.get(&id) {
                report.source_context().table_id
            } else if let Some(report) = inner.validation_reports.get(&id) {
                report.source_context().table_id
            } else if let Some(report) = inner.analysis_reports.get(&id) {
                report.source_context().table_id
            } else {
                return Ok(false);
            };
            let navigable = match table_id {
                Some(table_id) => !inner.unavailable_source_tables.contains(&table_id),
                None => false,
            };
            Ok(navigable)
        }

        async fn get(&self, id: ReportId) -> Result<Option<AnyReport>, RepositoryError> {
            let inner = self.inner.lock().unwrap();
            if let Some(r) = inner.inspection_reports.get(&id) {
                return Ok(Some(AnyReport::Inspection(r.clone())));
            }
            if let Some(r) = inner.validation_reports.get(&id) {
                return Ok(Some(AnyReport::Validation(r.clone())));
            }
            if let Some(r) = inner.analysis_reports.get(&id) {
                return Ok(Some(AnyReport::Analysis(r.clone())));
            }
            if let Some(r) = inner.lineage_reports.get(&id) {
                return Ok(Some(AnyReport::Lineage(r.clone())));
            }
            if let Some(r) = inner.comparison_reports.get(&id) {
                return Ok(Some(AnyReport::Comparison(r.clone())));
            }
            if let Some(r) = inner.audit_reports.get(&id) {
                return Ok(Some(AnyReport::Audit(r.clone())));
            }
            Ok(None)
        }

        async fn list_by_kind(&self, kind: ReportKind) -> Result<Vec<AnyReport>, RepositoryError> {
            let inner = self.inner.lock().unwrap();
            match kind {
                ReportKind::Inspection => Ok(inner.inspection_reports.values().cloned().map(AnyReport::Inspection).collect()),
                ReportKind::Validation => Ok(inner.validation_reports.values().cloned().map(AnyReport::Validation).collect()),
                ReportKind::Analysis => Ok(inner.analysis_reports.values().cloned().map(AnyReport::Analysis).collect()),
                ReportKind::Lineage => Ok(inner.lineage_reports.values().cloned().map(AnyReport::Lineage).collect()),
                ReportKind::Comparison => Ok(inner.comparison_reports.values().cloned().map(AnyReport::Comparison).collect()),
                ReportKind::Audit => Ok(inner.audit_reports.values().cloned().map(AnyReport::Audit).collect()),
            }
        }

        async fn get_content(&self, content_ref: &str) -> Result<Vec<u8>, RepositoryError> {
            self.inner.lock().unwrap().content_store.get(content_ref).cloned().ok_or(RepositoryError::NotFound)
        }
    }
}
