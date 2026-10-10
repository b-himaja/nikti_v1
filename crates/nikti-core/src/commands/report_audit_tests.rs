//! `run_transformation`/`apply_recipe_to_dataset`'s automatic Audit
//! Report trigger — real transformations, real Recipe application, real
//! before/after counts. Every assertion checks a specific field this
//! task's own Part A/B actually had to fix, not just "the call
//! succeeded."

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use strum::IntoEnumIterator;
    use uuid::Uuid;

    use crate::commands::{self, project_workspace, ImportSource};
    use crate::repositories::knowledge_repository::testing::InMemoryKnowledgeRepository;
    use crate::repositories::report_repository::testing::InMemoryReportRepository;
    use crate::repositories::table_repository::testing::InMemoryTableRepository;
    use crate::repositories::{AnyReport, DatasetRepository, DetailTableStore, KnowledgeRepository, ParquetContentStore, ProjectRepository, ReportRepository};
    use crate::types::analysis_procedure_params::AnalysisProcedureParams;
    use crate::types::common::ReportKind;
    use crate::types::report_registration::ReportState;
    use crate::types::validation_function_params::ValidationFunctionParams;
    use crate::types::{
        AnalysisProcedure, AnalysisProcedureTarget, AnalysisRuleset, AnalysisRulesetVersion, BindingMode, DefaultScope, OutputOptions, RuleNullPolicy, RuleTarget,
        RulesetMissingValuePolicy, Severity, TargetContract, TransformationOperation, ValidationRule, ValidationRuleset, ValidationRulesetVersion, ValidationTargetType,
    };

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("nikti-audit-test-{}", Uuid::new_v4()));
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

    fn write_csv(dir: &std::path::Path, name: &str, content: &str) -> String {
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        path.display().to_string()
    }

    // Mirrors `project_workspace_tests.rs`'s own helpers exactly — the
    // point of this file's exhaustiveness test is to construct a real
    // report of every kind, and Validation/Analysis have never actually
    // been constructed in THIS file before (see the rewritten test
    // below for why that matters).
    fn validation_ruleset() -> ValidationRuleset {
        let target = RuleTarget { target_type: ValidationTargetType::Column, binding_mode: BindingMode::FixedColumnName, target_columns: vec!["id".to_string()] };
        let rule = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "r".to_string(), ValidationFunctionParams::NotMissing, true, Severity::Error, target, RuleNullPolicy::FailIfMissing, "id must not be missing".to_string());
        let version = ValidationRulesetVersion::new(Uuid::new_v4(), 1, 1_000, "author".to_string(), "initial".to_string(), "1.0".to_string(), "1.0.0".to_string(), "hash".to_string(), vec![rule], vec![], vec![], vec![], vec![], RulesetMissingValuePolicy::default());
        ValidationRuleset::new(Uuid::new_v4(), "R".to_string(), 1_000, "author".to_string(), vec![], "".to_string(), version)
    }

    fn analysis_ruleset() -> AnalysisRuleset {
        let procedure = AnalysisProcedure::new(Uuid::new_v4(), "Column Property Explorer".to_string(), AnalysisProcedureParams::ColumnPropertyExplorer { columns: vec!["id".to_string()] }, true, 1, AnalysisProcedureTarget::columns(&["id"]), OutputOptions::default(), vec![], "".to_string(), vec![]);
        let version = AnalysisRulesetVersion::new(Uuid::new_v4(), 1, 1_000, "author".to_string(), "initial".to_string(), None, "1.0".to_string(), "1.0.0".to_string(), "0.1.0".to_string(), "hash".to_string(), TargetContract::new(vec![], None, vec![]), DefaultScope::default(), vec![procedure]).unwrap();
        AnalysisRuleset::new(Uuid::new_v4(), "AR".to_string(), 1_000, "author".to_string(), vec![], "".to_string(), version)
    }

    async fn only_audit_content(report_repo: &InMemoryReportRepository) -> (AnyReport, crate::types::AuditContent) {
        let reports = report_repo.list_by_kind(ReportKind::Audit).await.unwrap();
        assert_eq!(reports.len(), 1, "expected exactly one Audit Report, got {}", reports.len());
        let AnyReport::Audit(audit) = &reports[0] else { panic!("expected Audit") };
        let bytes = report_repo.get_content_bytes(audit.content_ref()).unwrap();
        let content: crate::types::AuditContent = serde_json::from_slice(&bytes).unwrap();
        (reports.into_iter().next().unwrap(), content)
    }

    /// A manual `run_transformation` call (no Recipe involved) produces
    /// an Audit Report whose `recipe_id`/`recipe_version` are honestly
    /// `None` — never a guessed or coincidentally-matching value.
    #[tokio::test]
    async fn manual_run_transformation_produces_an_audit_report_with_no_recipe_id() {
        let dir = TempDir::new();
        let content_store = ParquetContentStore::new(dir.path().join("content")).unwrap();
        let table_repo = InMemoryTableRepository::new();
        let report_repo = InMemoryReportRepository::new();
        let dataset_repo = crate::repositories::dataset_repository::testing::InMemoryDatasetRepository::new();
        let project_repo = crate::repositories::project_repository::testing::InMemoryProjectRepository::new();

        let uri = write_csv(dir.path(), "orders.csv", "id,amount\n1,10.5\n2,20.0\n");
        let table = commands::import_table(&table_repo, &report_repo, &content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();
        let project = project_workspace::create_project(&project_repo, "P".to_string(), 1_000).await.unwrap();
        let dataset = project_workspace::load_table_as_dataset(&table_repo, &dataset_repo, &project_repo, project.id(), table.id(), "orders".to_string(), 1_500).await.unwrap();

        project_workspace::run_transformation(&dataset_repo, &project_repo, &report_repo, &content_store, dataset.id(), vec![TransformationOperation::DropColumns { columns: vec!["amount".to_string()] }], 2_000, "tester".to_string())
            .await
            .unwrap();

        let (report, content) = only_audit_content(&report_repo).await;
        let AnyReport::Audit(audit) = &report else { unreachable!() };
        assert_eq!(audit.report_state(), ReportState::Available);
        assert_eq!(content.recipe_id, None, "a manual run_transformation call must never carry a recipe_id");
        assert_eq!(content.recipe_version, None);
        assert_eq!(content.input_dataset_id, dataset.id());
        assert_eq!(content.row_count_before, 2);
        assert_eq!(content.column_count_before, 2);
        assert_eq!(content.column_count_after, 1, "amount column really was dropped");
        assert_eq!(content.steps.len(), 1);
        assert!(content.steps[0].operation_log_entry_id != Uuid::nil());
        assert_eq!(content.transformation_catalogue_version, crate::engines::TRANSFORMATION_CATALOGUE_VERSION);
        assert!(!content.input_schema_fingerprint.is_empty(), "a real fingerprint, not a placeholder");
    }

    /// This test would have FAILED before Part A's root-cause fix —
    /// `apply_recipe_to_dataset` used to forward into `run_transformation`
    /// with no trace of the Recipe at all, so `recipe_id` had nothing
    /// real to be populated from. Exercises the new field for real, not
    /// a pre-populated fixture: the Recipe id/version come from a
    /// freshly-constructed `RecipeContext`, threaded all the way through
    /// to the Project Operation Log entry and then to this report.
    #[tokio::test]
    async fn apply_recipe_to_dataset_produces_an_audit_report_with_a_real_non_null_recipe_id() {
        let dir = TempDir::new();
        let content_store = ParquetContentStore::new(dir.path().join("content")).unwrap();
        let table_repo = InMemoryTableRepository::new();
        let report_repo = InMemoryReportRepository::new();
        let dataset_repo = crate::repositories::dataset_repository::testing::InMemoryDatasetRepository::new();
        let project_repo = crate::repositories::project_repository::testing::InMemoryProjectRepository::new();

        let uri = write_csv(dir.path(), "orders.csv", "id,amount\n1,10.5\n2,20.0\n");
        let table = commands::import_table(&table_repo, &report_repo, &content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();
        let project = project_workspace::create_project(&project_repo, "P".to_string(), 1_000).await.unwrap();
        let dataset = project_workspace::load_table_as_dataset(&table_repo, &dataset_repo, &project_repo, project.id(), table.id(), "orders".to_string(), 1_500).await.unwrap();

        let recipe_id = Uuid::new_v4();
        let recipe_version = 3u32;
        let context = project_workspace::RecipeContext { recipe_id, version: recipe_version, steps: vec![TransformationOperation::DropColumns { columns: vec!["amount".to_string()] }] };
        project_workspace::apply_recipe_to_dataset(&dataset_repo, &project_repo, &report_repo, &content_store, dataset.id(), context, 2_000, "tester".to_string()).await.unwrap();

        let (_report, content) = only_audit_content(&report_repo).await;
        assert_eq!(content.recipe_id, Some(recipe_id), "the real Recipe id this call was actually given, not a fixture");
        assert_eq!(content.recipe_version, Some(recipe_version));

        // The root-cause fix, verified at its actual source: the Project
        // Operation Log entry itself carries the Recipe reference now,
        // not just this report derived from it.
        let project_after = project_repo.get(project.id()).await.unwrap().unwrap();
        let entry = project_after.operation_log().entries().iter().find(|e| e.transformation_operation().is_some()).unwrap();
        assert_eq!(entry.recipe_id(), Some(recipe_id));
        assert_eq!(entry.recipe_version(), Some(recipe_version));
    }

    /// Part C, the ordering proven directly rather than only asserted:
    /// when Audit Report registration degrades (simulated the same way
    /// the original import task proved Inspection's own degraded-but-
    /// recoverable guarantee — `InMemoryReportRepository::
    /// with_content_validation_failure()`), the transformation itself —
    /// the Dataset's new content AND its Project Operation Log entry —
    /// is already committed and completely unaffected. `run_transformation`
    /// still returns `Ok` with the correctly-updated Dataset.
    #[tokio::test]
    async fn a_degraded_audit_report_registration_never_undoes_the_already_committed_transformation() {
        let dir = TempDir::new();
        let content_store = ParquetContentStore::new(dir.path().join("content")).unwrap();
        let table_repo = InMemoryTableRepository::new();
        let report_repo = InMemoryReportRepository::with_content_validation_failure();
        let dataset_repo = crate::repositories::dataset_repository::testing::InMemoryDatasetRepository::new();
        let project_repo = crate::repositories::project_repository::testing::InMemoryProjectRepository::new();

        let uri = write_csv(dir.path(), "orders.csv", "id,amount\n1,10.5\n2,20.0\n");
        let table = commands::import_table(&table_repo, &report_repo, &content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();
        let project = project_workspace::create_project(&project_repo, "P".to_string(), 1_000).await.unwrap();
        let dataset = project_workspace::load_table_as_dataset(&table_repo, &dataset_repo, &project_repo, project.id(), table.id(), "orders".to_string(), 1_500).await.unwrap();

        let result = project_workspace::run_transformation(&dataset_repo, &project_repo, &report_repo, &content_store, dataset.id(), vec![TransformationOperation::DropColumns { columns: vec!["amount".to_string()] }], 2_000, "tester".to_string()).await;

        // The transformation itself succeeded — never turned into an
        // error, never rolled back, by a downstream report-registration
        // problem.
        let updated = result.unwrap();
        assert_eq!(updated.schema().columns.len(), 1, "the transformation's own effect happened for real");

        let updated_dataset = dataset_repo.get(dataset.id()).await.unwrap().unwrap();
        assert_eq!(updated_dataset.schema().columns.len(), 1, "and was durably saved");

        let project_after = project_repo.get(project.id()).await.unwrap().unwrap();
        assert!(project_after.operation_log().entries().iter().any(|e| e.operation().starts_with("DropColumns")), "the transformation's own log entry is untouched by the later Audit Report failure");

        // The Audit Report itself did reach the repository, just
        // degraded — never silently dropped, never silently treated as
        // fully available. Content genuinely isn't stored for an
        // `Unavailable` report (registration's own step 6 fails before
        // the content write), so this checks metadata only — not
        // `only_audit_content`, which assumes readable content.
        let reports = report_repo.list_by_kind(ReportKind::Audit).await.unwrap();
        assert_eq!(reports.len(), 1);
        let AnyReport::Audit(audit) = &reports[0] else { panic!("expected Audit") };
        assert_eq!(audit.report_state(), ReportState::Unavailable, "content validation failure must surface as a degraded outcome, never be swallowed as success");
    }

    /// No duplicate/partial Audit Reports on a retried registration of
    /// the exact same audit event — same idempotency discipline every
    /// other report kind gets, proven directly against the shared
    /// registration algorithm (the realistic "interrupted then retried"
    /// scenario: the same report_id/idempotency_key/content submitted
    /// twice, e.g. after a crash between registering and confirming).
    #[tokio::test]
    async fn repeated_registration_of_the_same_audit_event_is_idempotent_not_duplicated() {
        let report_repo = InMemoryReportRepository::new();
        let content_bytes = b"{\"steps\":[]}".to_vec();
        let content_hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&content_bytes);
            hex::encode(hasher.finalize())
        };
        let report_id = Uuid::new_v4();
        let make_request = || crate::types::ReportRegistrationRequest {
            report_registration_request_id: Uuid::new_v4(),
            idempotency_key: "audit-idempotency-test".to_string(),
            report_id,
            report_title: "Transformation Audit Report — idempotency test".to_string(),
            report_schema_version: "1.0".to_string(),
            created_at: 1_000,
            created_by: "tester".to_string(),
            source_context: crate::types::report_registration::SourceContext::for_project_dataset(Uuid::new_v4(), Uuid::new_v4()),
            source_event_ref: crate::types::report_registration::SourceEventRef {
                source_event_type: "engine_execution".to_string(),
                source_execution_id: Uuid::new_v4(),
                source_engine: "transformation_engine".to_string(),
                source_engine_version: crate::types::common::EngineVersion { rust_core: "0.1.0".to_string(), python_env_hash: None },
                source_execution_status: "completed".to_string(),
            },
            content_format: "application/json".to_string(),
            content_hash: content_hash.clone(),
            content_bytes: content_bytes.clone(),
            knowledge_refs: vec![],
            log_refs: vec![],
        };

        let first = report_repo.register_audit_report(make_request()).await.unwrap();
        let first_id = match first {
            crate::types::RegistrationOutcome::Registered { report_id } => report_id,
            other => panic!("expected Registered, got {other:?}"),
        };

        let second = report_repo.register_audit_report(make_request()).await.unwrap();
        assert_eq!(second, crate::types::RegistrationOutcome::Idempotent { report_id: first_id });
        assert_eq!(report_repo.list_by_kind(ReportKind::Audit).await.unwrap().len(), 1, "never a duplicate");
    }

    /// Part E, rebuilt per status audit 3's Part B: the exact class of
    /// bug that let Lineage/Comparison sit registered-but-unopenable for
    /// a full task cycle before the compiler forced a *fix* (adding a
    /// match arm — Rust's exhaustiveness check says nothing about
    /// whether that arm is a real implementation or a stub returning an
    /// error). The original version of this test iterated a hardcoded
    /// `[Inspection, Audit, Lineage, Comparison]` array that itself
    /// excluded Validation/Analysis on the strength of a comment
    /// claiming they were "already covered elsewhere" —
    /// `report_viewer_tests.rs` was checked directly and that claim was
    /// false: it only ever calls `open_report` against an Inspection
    /// report. So this version does two things the original didn't:
    /// (1) iterates `ReportKind::iter()` (a `strum::EnumIter` derive on
    /// the real enum in `common.rs`) instead of a second hand-typed
    /// list, so a 7th variant is included automatically rather than
    /// requiring a human to remember to add it here too; (2) actually
    /// constructs a real Validation and a real Analysis report through
    /// their real execution paths and opens them, closing the gap the
    /// old comment merely asserted was already closed.
    #[tokio::test]
    async fn every_reportkind_variant_that_can_be_constructed_is_openable_via_open_report() {
        let dir = TempDir::new();
        let content_store = ParquetContentStore::new(dir.path().join("content")).unwrap();
        let detail_store = DetailTableStore::new(dir.path().join("detail")).unwrap();
        let table_repo = InMemoryTableRepository::new();
        let report_repo = InMemoryReportRepository::new();
        let dataset_repo = crate::repositories::dataset_repository::testing::InMemoryDatasetRepository::new();
        let project_repo = crate::repositories::project_repository::testing::InMemoryProjectRepository::new();
        let knowledge_repo = InMemoryKnowledgeRepository::new();

        let uri = write_csv(dir.path(), "orders.csv", "id,amount\n1,10.5\n2,20.0\n");
        let table = commands::import_table(&table_repo, &report_repo, &content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();
        let project = project_workspace::create_project(&project_repo, "P".to_string(), 1_000).await.unwrap();
        let dataset = project_workspace::load_table_as_dataset(&table_repo, &dataset_repo, &project_repo, project.id(), table.id(), "orders".to_string(), 1_500).await.unwrap();
        // Real Inspection report — produced automatically by import_table above.

        // Real Validation report — never exercised by this test before.
        let mut vruleset = validation_ruleset();
        let vversion_id = vruleset.versions()[0].ruleset_version_id();
        vruleset.activate_version(vversion_id, 1_400, "author".to_string()).unwrap();
        knowledge_repo.store_validation_ruleset(vruleset.clone()).await.unwrap();
        project_workspace::run_validation_on_dataset(&dataset_repo, &project_repo, &knowledge_repo, &report_repo, &content_store, &detail_store, dataset.id(), vruleset.ruleset_id(), vversion_id, 1_800, "tester".to_string())
            .await
            .unwrap();

        // Real Analysis report — never exercised by this test before.
        let mut aruleset = analysis_ruleset();
        let aversion_id = aruleset.versions()[0].analysis_ruleset_version_id();
        aruleset.activate_version(aversion_id, 1_900, "author".to_string()).unwrap();
        knowledge_repo.store_analysis_ruleset(aruleset.clone()).await.unwrap();
        project_workspace::run_analysis_on_dataset(&dataset_repo, &project_repo, &knowledge_repo, &report_repo, &content_store, &detail_store, dataset.id(), aruleset.analysis_ruleset_id(), aversion_id, 1_950, "tester".to_string())
            .await
            .unwrap();

        // Real Audit report.
        project_workspace::run_transformation(&dataset_repo, &project_repo, &report_repo, &content_store, dataset.id(), vec![TransformationOperation::DropColumns { columns: vec!["amount".to_string()] }], 2_000, "tester".to_string())
            .await
            .unwrap();
        // Real Lineage report.
        commands::generate_lineage_report(&table_repo, &dataset_repo, &project_repo, &report_repo, commands::LineageQueryObject::Table(table.id()), 3_000, "tester".to_string()).await.unwrap();

        // Two same-kind Inspection reports to compare — real Comparison report.
        let uri2 = write_csv(dir.path(), "orders2.csv", "id,amount\n1,10.5\n");
        let table2 = commands::import_table(&table_repo, &report_repo, &content_store, ImportSource { uri: uri2 }, 4_000, "tester".to_string()).await.unwrap();
        let inspection_reports_1 = commands::open_table_reports(&report_repo, table.id()).await.unwrap();
        let inspection_reports_2 = commands::open_table_reports(&report_repo, table2.id()).await.unwrap();
        let AnyReport::Inspection(insp1) = &inspection_reports_1[0] else { panic!() };
        let AnyReport::Inspection(insp2) = &inspection_reports_2[0] else { panic!() };
        commands::generate_comparison_report(&report_repo, insp1.report_id(), insp2.report_id(), 5_000, "tester".to_string()).await.unwrap();

        // Every real `ReportKind` variant, derived from the enum itself
        // — not a second list a future report kind could be added
        // without ever reaching.
        let mut kinds_checked = 0;
        for kind in ReportKind::iter() {
            kinds_checked += 1;
            let reports = report_repo.list_by_kind(kind).await.unwrap();
            assert!(!reports.is_empty(), "expected at least one real {kind:?} report to test open_report against — every ReportKind variant must be constructed by this test, not just some of them");
            for report in reports {
                let id = match &report {
                    AnyReport::Inspection(r) => r.report_id(),
                    AnyReport::Validation(r) => r.report_id(),
                    AnyReport::Analysis(r) => r.report_id(),
                    AnyReport::Lineage(r) => r.report_id(),
                    AnyReport::Comparison(r) => r.report_id(),
                    AnyReport::Audit(r) => r.report_id(),
                };
                let opened = commands::open_report(&report_repo, id).await;
                assert!(opened.is_ok(), "open_report must resolve real content for every constructible ReportKind — {kind:?} failed: {opened:?}");
            }
        }
        assert_eq!(kinds_checked, 6, "ReportKind::iter() must yield exactly the 6 variants this test knows how to construct — if this fails, a variant was added to the enum without this test being updated to construct one, which is exactly the staleness this rewrite exists to surface");
    }

    /// `generate_audit_report` (the Report Engine method) is unreachable
    /// except through the automatic trigger inside
    /// `run_transformation_steps` — enforced structurally by Rust's own
    /// privacy (the trigger function is not `pub`, so no other module
    /// can name it), confirmed here independent of the compiler: no
    /// `#[tauri::command]` wrapper exists for it, and the only call to
    /// `ReportEngine::generate_audit_report(` anywhere in this crate's
    /// non-test source is inside `project_workspace.rs`.
    #[test]
    fn generate_audit_report_is_unreachable_except_through_the_automatic_trigger() {
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
                if name.ends_with("_tests.rs") {
                    continue;
                }
                let source = std::fs::read_to_string(&path).unwrap();
                if source.contains(".generate_audit_report(") {
                    hits.push(path.display().to_string());
                }
            }
        }

        let src_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut hits = Vec::new();
        visit(std::path::Path::new(src_dir), &mut hits);
        assert_eq!(hits, vec![concat!(env!("CARGO_MANIFEST_DIR"), "/src/commands/project_workspace.rs").to_string()], "exactly one call site: the automatic trigger inside run_transformation_steps — got {hits:?}");

        // No Tauri-facing command exists for it either — checked against
        // the app crate's own ipc_commands.rs from here, same
        // "never manually triggered" guarantee at the UI-facing layer.
        let ipc_commands_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../nikti-app/src/ipc_commands.rs");
        let ipc_source = std::fs::read_to_string(ipc_commands_path).unwrap();
        assert!(!ipc_source.contains("generate_audit_report"), "no #[tauri::command] wrapper may exist for a manual Audit Report trigger");
    }
}
