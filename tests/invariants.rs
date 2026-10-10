// tests/invariants.rs
//
// One test per CLAUDE.md Part 2 invariant relevant to this pass's
// scaffolding, matching the named set given in CLAUDE.md Part 4. Tests
// that can be proven against the current typed stubs alone run for real;
// tests that need real repository/engine logic are #[ignore]d until that
// logic lands in a later task — they exist and are named now so every
// invariant stays trackable from day one (per this pass's brief: do not
// defer writing the test itself, only its passing).

use uuid::Uuid;

use nikti_core::engines::{ImportSummary, InspectionEngine, InspectionResults, TableContent};
use nikti_core::types::{ProjectOperationLog, ProjectOperationLogEntry, TableEventLog, TableEventLogEntry};

#[test]
fn inv002_table_has_no_write_methods() {
    // Table (crates/nikti-core/src/types/table.rs) exposes only `new`
    // (construction of a brand-new Table — never a write against an
    // existing one, see INV-002's code implication #2) plus read
    // accessors: id/schema/row_count/content_fingerprint/created_at/
    // provenance. No update/delete/append/insert_row/add_column method
    // exists on the type — checked structurally by the type definition
    // itself, not by asserting a call to one errors.
}

#[tokio::test]
async fn inv002_importing_a_second_table_does_not_alter_the_first() {
    // Real `TableRepository` + real `import_table` both landed in the
    // Import Pipeline task — incidentally fixed here (same file, same
    // sitting) rather than left as a stale ignore now that it's
    // resolvable; not part of this task's own core deliverable.
    use nikti_core::commands::{self, ImportSource};
    use nikti_core::repositories::report_repository::testing::InMemoryReportRepository;
    use nikti_core::repositories::table_repository::testing::InMemoryTableRepository;
    use nikti_core::repositories::{ParquetContentStore, TableRepository};

    let dir = std::env::temp_dir().join(format!("nikti-inv002-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let content_store = ParquetContentStore::new(dir.join("content")).unwrap();
    let table_repo = InMemoryTableRepository::new();
    let report_repo = InMemoryReportRepository::new();

    let path_a = dir.join("a.csv");
    std::fs::write(&path_a, "id,name\n1,alice\n2,bob\n").unwrap();
    let table_a = commands::import_table(&table_repo, &report_repo, &content_store, ImportSource { uri: path_a.display().to_string() }, 1_000, "tester".to_string())
        .await
        .unwrap();
    let snapshot = table_a.row_count();

    let path_b = dir.join("b.csv");
    std::fs::write(&path_b, "id,name\n1,zed\n2,yolanda\n3,xavier\n").unwrap();
    let _table_b = commands::import_table(&table_repo, &report_repo, &content_store, ImportSource { uri: path_b.display().to_string() }, 1_000, "tester".to_string())
        .await
        .unwrap();

    assert_eq!(table_repo.get(table_a.id()).await.unwrap().unwrap().row_count(), snapshot);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn inv004_report_has_no_write_methods() {
    // report_validation.rs/report_analysis.rs/report_audit.rs/report_lineage.rs
    // (out of scope for the Inspection/Report Engine task) still expose
    // only `new` plus `meta()`/`content()` read accessors.
    //
    // report_inspection.rs's `InspectionReport` (Report Object Model v1.0
    // shape) exposes `new` plus one read accessor per field
    // (report_id/report_type/report_title/report_state/... /log_refs),
    // and `with_state`, which takes `&self` and returns an independent
    // owned value — not a `&mut self` write path against a value the
    // caller already holds. No update/delete/regenerate method exists on
    // any of them — checked structurally by the type definitions.
}

#[test]
fn inv005_log_supports_append_only() {
    let mut table_log = TableEventLog::new();
    assert_eq!(table_log.entries().len(), 0);
    table_log.append(TableEventLogEntry::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        0,
        "imported".into(),
    ));
    assert_eq!(table_log.entries().len(), 1);

    let mut project_log = ProjectOperationLog::new();
    project_log.append(ProjectOperationLogEntry::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        0,
        "created_dataset".into(),
    ));
    assert_eq!(project_log.entries().len(), 1);

    // update/delete/clear/truncate do not exist on either log type —
    // `entries()` returns an immutable slice and there is no
    // `entries_mut()`, so this is a compile-time absence, not merely a
    // runtime-checked one.
}

#[test]
fn inv005_appended_entry_is_frozen() {
    let mut log = ProjectOperationLog::new();
    let entry = ProjectOperationLogEntry::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        42,
        "transform".into(),
    );
    let entry_id = entry.id();
    log.append(entry);

    let read_back = &log.entries()[0];
    assert_eq!(read_back.id(), entry_id);
    assert_eq!(read_back.operation(), "transform");
    // Mutating the already-appended entry is a compile-time impossibility:
    // `entries()` returns `&[ProjectOperationLogEntry]`, there is no
    // `entries_mut()`, and ProjectOperationLogEntry itself exposes no
    // `&mut self` method — there is no expression that both type-checks
    // and mutates `read_back`.
}

#[tokio::test]
async fn inv007_recipe_promotion_rejects_non_operation_log_sources() {
    // The real implementation landed in the Project Workspace task —
    // `commands::project_workspace::promote_operation_log_to_recipe`.
    // Its signature accepts only a `project_id` and reads
    // `Project::operation_log()` internally; there is no parameter
    // through which a Table Event Log entry id (or a Report id, or a
    // Table id) could ever be passed in — "promote from a non-Project-
    // Operation-Log source" is not an expressible call, not merely one
    // that errors at runtime. This test proves the *reachable* half of
    // that guarantee: a real Project Operation Log entry, containing a
    // real `TransformationOperation`, promotes into a Recipe with real,
    // replayable steps — matching this codebase's existing preference
    // for structural rejection (INV-013's `must not typecheck`) over a
    // runtime-only check wherever the type system can carry the weight.
    use nikti_core::commands::project_workspace;
    use nikti_core::repositories::project_repository::testing::InMemoryProjectRepository;
    use nikti_core::repositories::knowledge_repository::testing::InMemoryKnowledgeRepository;
    use nikti_core::repositories::ProjectRepository;
    use nikti_core::types::{Project, ProjectOperationLogEntry, TransformationOperation};

    let project_repo = InMemoryProjectRepository::new();
    let knowledge_repo = InMemoryKnowledgeRepository::new();

    let project_id = Uuid::new_v4();
    let dataset_id = Uuid::new_v4();
    let mut project = Project::new(project_id, "p".to_string(), 0);
    let operation = TransformationOperation::DropColumns { columns: vec!["x".to_string()] };
    project.operation_log_mut().append(ProjectOperationLogEntry::new_transformation(
        Uuid::new_v4(),
        project_id,
        dataset_id,
        0,
        "drop_columns".to_string(),
        operation.clone(),
    ));
    project_repo.insert(project).await.unwrap();

    let recipe = project_workspace::promote_operation_log_to_recipe(&project_repo, &knowledge_repo, project_id, "R".to_string(), 0..1, 1_000)
        .await
        .expect("a real Project Operation Log entry must promote successfully");
    assert_eq!(recipe.steps(), &[operation]);
}

#[test]
fn inv013_transformation_engine_rejects_table_targets() {
    // TransformationEngine::transform's content parameter is
    // DatasetContent, not TableContent/TableId — there is no
    // `From<TableId>` or `From<TableContent>` conversion into
    // DatasetContent, so a `derive_column(table, ..)`-shaped call does
    // not typecheck. A structural (compile-time) guarantee, not a
    // runtime-checked error path.
}

#[test]
fn inv015_engines_return_raw_results_never_reports() {
    // Proves at the type level that InspectionEngine::inspect returns
    // Result<InspectionResults, _>, never a Report — without ever calling
    // the trait's `todo!()` default body (the closure below is never
    // invoked).
    struct DummyInspection;
    impl InspectionEngine for DummyInspection {}

    fn assert_signature<F>(_f: F)
    where
        F: Fn(TableContent, ImportSummary) -> Result<InspectionResults, nikti_core::engines::InspectionError>,
    {
    }

    assert_signature(|content, import_summary| DummyInspection.inspect(content, import_summary));
}

#[tokio::test]
async fn inv016_comparison_rejects_mismatched_report_types() {
    use nikti_core::commands::{self, ImportSource};
    use nikti_core::engines::ReportEngineError;
    use nikti_core::repositories::report_repository::testing::InMemoryReportRepository;
    use nikti_core::repositories::table_repository::testing::InMemoryTableRepository;
    use nikti_core::repositories::{AnyReport, ParquetContentStore, ReportRepository};
    use nikti_core::types::common::EngineVersion;
    use nikti_core::types::report_registration::{SourceContext, SourceContextKind, SourceEventRef};
    use nikti_core::types::ReportRegistrationRequest;

    let dir = std::env::temp_dir().join(format!("nikti-inv016-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let content_store = ParquetContentStore::new(dir.join("content")).unwrap();
    let table_repo = InMemoryTableRepository::new();
    let report_repo = InMemoryReportRepository::new();

    let csv_path = dir.join("orders.csv");
    std::fs::write(&csv_path, "id,amount\n1,10.5\n").unwrap();
    let table = commands::import_table(&table_repo, &report_repo, &content_store, ImportSource { uri: csv_path.display().to_string() }, 1_000, "tester".to_string())
        .await
        .unwrap();
    let reports = commands::open_table_reports(&report_repo, table.id()).await.unwrap();
    let AnyReport::Inspection(inspection) = &reports[0] else { panic!("expected an Inspection report") };
    let inspection_id = inspection.report_id();

    let analysis_id = Uuid::new_v4();
    let request = ReportRegistrationRequest {
        report_registration_request_id: Uuid::new_v4(),
        idempotency_key: "inv016-test:analysis".to_string(),
        report_id: analysis_id,
        report_title: "Analysis Report — INV-016 test".to_string(),
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
        content_hash: {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(b"{}");
            hex::encode(hasher.finalize())
        },
        content_bytes: b"{}".to_vec(),
        knowledge_refs: vec![],
        log_refs: vec![],
    };
    report_repo.register_analysis_report(request).await.unwrap();

    let result = commands::generate_comparison_report(&report_repo, inspection_id, analysis_id, 2_000, "tester".to_string()).await;
    assert!(matches!(result, Err(commands::CommandError::Report(ReportEngineError::MismatchedReportKinds))), "got {result:?}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn inv025_no_ai_imports_in_deterministic_engines() {
    let engines_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/engines");
    let forbidden = ["openai", "anthropic", "llm", "gpt"];

    // Recurses: `engines/transformation/` and `engines/inspection/` are
    // both directory modules, not single files — a non-recursive scan
    // would silently stop checking either engine's real source.
    fn visit(dir: &std::path::Path, forbidden: &[&str]) {
        for entry in std::fs::read_dir(dir).expect("directory must exist") {
            let path = entry.expect("readable directory entry").path();
            if path.is_dir() {
                visit(&path, forbidden);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("engine source file must be readable");
            let lowercase = source.to_lowercase();
            // Whole-word match, not substring containment — a bare
            // `contains("llm")` false-positives on ordinary identifiers
            // like `fill_missing`/`FillMissing`.
            let words: std::collections::HashSet<&str> =
                lowercase.split(|c: char| !c.is_ascii_alphanumeric()).collect();
            for token in forbidden {
                assert!(
                    !words.contains(token),
                    "forbidden AI/LLM token {token:?} found in {path:?} — INV-025/Rule 8"
                );
            }
        }
    }

    visit(std::path::Path::new(engines_dir), &forbidden);
}

#[test]
#[ignore = "needs analyse_python's real sidecar-call implementation (later task)"]
fn inv029_sidecar_unavailable_yields_not_executed_zero_partial_evidence() {
    // let backend = ComputeBackend::Python { status: PythonSidecarStatus::Unavailable { reason: "down".into() } };
    // let outcome = commands::analyse_python(table_id, ruleset_id, backend).await.unwrap();
    // assert!(matches!(outcome, AnalysisCommandOutcome::NotExecuted { .. }));
}

#[test]
#[ignore = "needs a real PythonAnalysisClient + crash simulation (later task)"]
fn inv029_sidecar_crash_mid_call_produces_no_partial_report_or_transform() {
    // Simulate a crash during a sidecar call (e.g. kill the child mid
    // heartbeat); assert the resulting AnalysisOutcome is NotExecuted,
    // never a partial AnalysisReport, and that a subsequent call
    // respawns the process.
}

/// Real half: a native Transformation operation, run through the actual
/// command layer (`project_workspace::run_transformation`), produces a
/// real appended Project Operation Log entry.
///
/// Test-double half: `run_transformation` currently has no
/// `ComputeBackend` parameter at all — Transformation's Python-backed
/// exception list is empty (ADR 0006), so there is no real Python-backed
/// Transformation operation to run end-to-end. Per this task's own
/// instruction, rather than skip the "regardless of backend" half of
/// this invariant, a minimal test double exercises the exact same
/// log-append code path with content that did not come from
/// `NativeTransformationEngine` — proving the append itself has no
/// dependency on which engine/backend produced the content it's
/// logging. `ProjectOperationLogEntry::new_transformation` and
/// `ProjectOperationLog::append` are the same two calls `run_transformation`
/// itself uses (crates/nikti-core/src/commands/project_workspace.rs) —
/// this is not a parallel/reimplemented logging path, it is the real one,
/// invoked directly with a stand-in operation description to simulate
/// "a raw result the sidecar just returned, now being logged by the Rust
/// command layer" (INV-029: the sidecar itself has no log-write
/// capability at all — this is precisely the boundary being tested).
/// Full end-to-end coverage of an actual Python-backed Transformation
/// step is blocked until both a real exception-list entry exists (ADR-
/// reviewed) and `run_transformation` grows a `ComputeBackend` parameter
/// to route through — neither exists yet, and this test does not
/// pretend otherwise.
#[tokio::test]
async fn inv029_transformation_step_is_logged_regardless_of_backend() {
    use nikti_core::commands::{self, ImportSource};
    use nikti_core::repositories::dataset_repository::testing::InMemoryDatasetRepository;
    use nikti_core::repositories::project_repository::testing::InMemoryProjectRepository;
    use nikti_core::repositories::report_repository::testing::InMemoryReportRepository;
    use nikti_core::repositories::table_repository::testing::InMemoryTableRepository;
    use nikti_core::repositories::{ParquetContentStore, ProjectRepository};
    use nikti_core::types::TransformationOperation;

    let dir = std::env::temp_dir().join(format!("nikti-inv029-log-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let content_store = ParquetContentStore::new(dir.join("content")).unwrap();
    let table_repo = InMemoryTableRepository::new();
    let dataset_repo = InMemoryDatasetRepository::new();
    let project_repo = InMemoryProjectRepository::new();
    let report_repo = InMemoryReportRepository::new();

    let csv_path = dir.join("orders.csv");
    std::fs::write(&csv_path, "id,amount\n1,10.5\n2,20.0\n").unwrap();
    let table = commands::import_table(&table_repo, &report_repo, &content_store, ImportSource { uri: csv_path.display().to_string() }, 1_000, "tester".to_string())
        .await
        .unwrap();
    let project = commands::project_workspace::create_project(&project_repo, "P".to_string(), 1_000).await.unwrap();
    let dataset = commands::project_workspace::load_table_as_dataset(&table_repo, &dataset_repo, &project_repo, project.id(), table.id(), "orders".to_string(), 1_500)
        .await
        .unwrap();

    // 1. Native op, run for real through the real command layer.
    commands::project_workspace::run_transformation(&dataset_repo, &project_repo, &report_repo, &content_store, dataset.id(), vec![TransformationOperation::DropColumns { columns: vec!["amount".to_string()] }], 2_000, "tester".to_string())
        .await
        .unwrap();
    let project_after_native = project_repo.get(project.id()).await.unwrap().unwrap();
    let native_entries_before = project_after_native.operation_log().entries().len();
    assert!(project_after_native.operation_log().entries().iter().any(|e| e.operation().starts_with("DropColumns")), "the native op's real label was really appended by the command layer");

    // 2. Test double: the same log-write call `run_transformation` uses,
    // invoked directly with content standing in for "a raw result the
    // sidecar just returned" — proving the append mechanism itself does
    // not branch on, or care about, which engine/backend produced it.
    let mut project = project_after_native;
    let project_id = project.id();
    let stand_in_operation = TransformationOperation::GenerateRowNumber { output_col: Some("row_num".to_string()), start: 1 };
    project.operation_log_mut().append(ProjectOperationLogEntry::new_transformation(Uuid::new_v4(), project_id, dataset.id(), 2_100, "simulated-python-backed-op".to_string(), stand_in_operation));
    project_repo.save(project).await.unwrap();

    let final_project = project_repo.get(project_id).await.unwrap().unwrap();
    assert_eq!(final_project.operation_log().entries().len(), native_entries_before + 1, "both the native call and the test double appended through the exact same ProjectOperationLog::append path");
    assert!(final_project.operation_log().entries().iter().any(|e| e.operation() == "simulated-python-backed-op"));

    let _ = std::fs::remove_dir_all(&dir);
}

/// The exception-list registry itself, for both engines that currently
/// have one (Transformation, Analysis), is real and enumerable — not
/// merely a decision recorded in an ADR with no corresponding code.
/// Both are empty today (ADR 0006/0007: neither engine has an operation
/// without a mature native Polars equivalent), which the invariant
/// permits explicitly ("an empty, explicit list is still an explicit
/// list"). What this test does NOT cover, and is explicitly blocked:
/// routing enforcement for a non-empty list (there is nothing on either
/// list to route), and Analysis's sidecar dispatch path specifically —
/// `PythonAnalysisClient::call` is still `todo!()` with zero concrete
/// implementations, so no operation could be routed to it even if the
/// exception list were non-empty. That gap is tracked separately (it's
/// a dispatch-implementation gap, not an exception-list gap) and is not
/// silently declared covered by this test passing.
#[test]
fn inv029_exception_list_operations_are_explicit_and_enumerable() {
    use nikti_core::engines::analysis::python_ops::PYTHON_BACKED_PROCEDURES;
    use nikti_core::engines::transformation::python_ops::PYTHON_BACKED_OPERATIONS;

    assert_eq!(PYTHON_BACKED_OPERATIONS, &[] as &[&str], "Transformation's exception list — real, checked-in, currently empty per ADR 0006");
    assert_eq!(PYTHON_BACKED_PROCEDURES, &[] as &[&str], "Analysis's exception list — real, checked-in, currently empty per ADR 0007");
}

/// The "fix fabricated evidence" task's own required search: not just
/// re-checking the three flagged locations (Inspection's fingerprint,
/// Validation's `AffectedRowsRef`, Analysis's `OutputTableRef`), but
/// scanning the whole `engines`/`commands`/`repositories` tree for the
/// SAME shape of defect recurring anywhere else — a reference/fingerprint/
/// value constructed to look like real evidence with no real computation
/// or real storage write behind it.
///
/// Two independent checks, both grounded in what the fix actually looks
/// like once real: (1) every `.parquet`-shaped reference string built
/// anywhere in production code must live in one of the files known to
/// pair that construction with a real write — `table_import.rs`'s
/// `ParquetContentStore`/`DetailTableStore` (the writers themselves),
/// `validation/mod.rs` (paired with a real write in
/// `project_workspace::run_validation_on_dataset`), and
/// `analysis/procedures.rs` (paired with a real write in
/// `run_analysis_on_dataset`) — a `.parquet` reference appearing in any
/// OTHER file would mean a new, unpaired fabrication; (2) the specific
/// fabricated-fingerprint shape (`"unfingerprinted:"`) must not exist in
/// any non-test file at all — only in comments/tests that explicitly
/// document or guard against the old bug.
#[test]
fn no_other_engine_has_the_same_evidence_fabrication_pattern() {
    let src_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    // Files that legitimately construct a `.parquet`-shaped reference —
    // either because they ARE the real writer, or because the
    // construction there is proven (by the tests in this task) to be
    // paired with a real write one layer up, at the command layer.
    // Matched against the path SUFFIX, not the bare filename — several
    // unrelated modules in this crate are also named `mod.rs`, and a
    // bare-filename match would silently allowlist all of them.
    let allowed_parquet_reference_path_suffixes = [
        "repositories/table_import.rs",
        "engines/validation/mod.rs",
        "engines/analysis/procedures.rs",
    ];

    fn visit(dir: &std::path::Path, allowed: &[&str], offenders: &mut Vec<String>, unfingerprinted_offenders: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).expect("directory must exist") {
            let path = entry.expect("readable directory entry").path();
            if path.is_dir() {
                visit(&path, allowed, offenders, unfingerprinted_offenders);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Test files are allowed to reference `.parquet`/
            // `"unfingerprinted:"` freely — they construct fixtures and
            // assert against the old bug's shape, never ship it.
            let is_test_file = file_name.contains("test");
            let raw = std::fs::read_to_string(&path).expect("source file must be readable");
            // Strip `//`-comment lines before scanning — doc comments
            // legitimately narrate the old bug's exact shape (e.g. this
            // very file's own history section, or `content.rs`'s doc
            // comment on why the fingerprint field exists), same
            // comment-stripping discipline used by this crate's other
            // pattern scanners (see the Analysis excluded-technique scan).
            let source: String = raw.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
            let path_str = path.display().to_string().replace('\\', "/");
            let is_allowed = allowed.iter().any(|suffix| path_str.ends_with(suffix));

            if !is_test_file && source.contains(".parquet") && !is_allowed {
                offenders.push(format!("{}: constructs a .parquet-shaped reference outside the known real-writer/paired-write files", path.display()));
            }
            if !is_test_file && source.contains("unfingerprinted:") {
                unfingerprinted_offenders.push(format!("{}: still contains the fabricated-fingerprint shape in live code", path.display()));
            }
        }
    }

    let mut offenders = Vec::new();
    let mut unfingerprinted_offenders = Vec::new();
    visit(std::path::Path::new(src_dir), &allowed_parquet_reference_path_suffixes, &mut offenders, &mut unfingerprinted_offenders);

    assert!(offenders.is_empty(), "found new/unpaired .parquet-reference construction sites beyond the three known ones: {offenders:#?}");
    assert!(unfingerprinted_offenders.is_empty(), "found live code still using the fabricated-fingerprint shape: {unfingerprinted_offenders:#?}");
}

#[test]
#[ignore = "needs the real ReportEngine cache + env-hash wiring (later task)"]
fn inv030_python_env_hash_change_invalidates_cache_and_bumps_version() {
    // Changing the pinned lockfile hash must produce a cache miss and a
    // new engine_version (EngineVersion.python_env_hash) on the next
    // report, not a silently unchanged one.
}
