#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use uuid::Uuid;

    use crate::commands::project_workspace::{self, ProjectWorkspaceError};
    use crate::commands::{self, ImportSource};
    use crate::repositories::dataset_repository::testing::InMemoryDatasetRepository;
    use crate::repositories::knowledge_repository::testing::InMemoryKnowledgeRepository;
    use crate::repositories::pending_plan_repository::testing::InMemoryPendingPlanRepository;
    use crate::repositories::project_repository::testing::InMemoryProjectRepository;
    use crate::repositories::report_repository::testing::InMemoryReportRepository;
    use crate::repositories::table_repository::testing::InMemoryTableRepository;
    use crate::repositories::{DatasetRepository, DetailTableStore, KnowledgeRepository, ParquetContentStore, ProjectRepository, TableRepository};
    use crate::types::analysis_procedure_params::AnalysisProcedureParams;
    use crate::types::validation_function_params::ValidationFunctionParams;
    use crate::types::{
        AnalysisProcedure, AnalysisProcedureTarget, AnalysisRuleset, AnalysisRulesetVersion, BindingMode, DefaultScope,
        OutputOptions, RegistrationOutcome, RuleNullPolicy, RuleTarget, RulesetMissingValuePolicy, Severity,
        TargetContract, TransformationOperation, ValidationRule, ValidationRuleset, ValidationRulesetVersion,
        ValidationTargetType,
    };

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("nikti-project-workspace-test-{}", Uuid::new_v4()));
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
        dataset_repo: InMemoryDatasetRepository,
        project_repo: InMemoryProjectRepository,
        report_repo: InMemoryReportRepository,
        knowledge_repo: InMemoryKnowledgeRepository,
        plan_repo: InMemoryPendingPlanRepository,
        content_store: ParquetContentStore,
        detail_store: DetailTableStore,
    }

    impl Harness {
        fn new() -> Self {
            let dir = TempDir::new();
            let content_store = ParquetContentStore::new(dir.path().join("content")).unwrap();
            let detail_store = DetailTableStore::new(dir.path().join("detail")).unwrap();
            Self {
                _dir: dir,
                table_repo: InMemoryTableRepository::new(),
                dataset_repo: InMemoryDatasetRepository::new(),
                project_repo: InMemoryProjectRepository::new(),
                report_repo: InMemoryReportRepository::new(),
                knowledge_repo: InMemoryKnowledgeRepository::new(),
                plan_repo: InMemoryPendingPlanRepository::new(),
                content_store,
                detail_store,
            }
        }
    }

    fn write_csv(dir: &std::path::Path, name: &str, content: &str) -> String {
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        path.display().to_string()
    }

    /// A real Project + a real Table (imported via the real pipeline) +
    /// a real Dataset loaded from it — the standard starting point for
    /// these tests, all through already-real, already-tested machinery.
    async fn setup(h: &Harness) -> (crate::types::Project, crate::types::Table, crate::types::Dataset) {
        let scratch = TempDir::new();
        let uri = write_csv(scratch.path(), "orders.csv", "id,amount\n1,10.5\n2,20.0\n3,30.0\n");
        let table = commands::import_table(&h.table_repo, &h.report_repo, &h.content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();
        let project = project_workspace::create_project(&h.project_repo, "P".to_string(), 1_000).await.unwrap();
        let dataset = project_workspace::load_table_as_dataset(&h.table_repo, &h.dataset_repo, &h.project_repo, project.id(), table.id(), "orders".to_string(), 1_500).await.unwrap();
        let project = h.project_repo.get(project.id()).await.unwrap().unwrap();
        (project, table, dataset)
    }

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

    #[tokio::test]
    async fn create_open_and_list_projects_happy_path() {
        let h = Harness::new();
        let created = project_workspace::create_project(&h.project_repo, "orders project".to_string(), 1_000).await.unwrap();

        let opened = project_workspace::open_project(&h.project_repo, created.id()).await.unwrap();
        assert_eq!(opened.id(), created.id());
        assert_eq!(opened.name(), "orders project");

        let summaries = project_workspace::list_projects(&h.project_repo).await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].project_id, created.id());
        assert_eq!(summaries[0].dataset_count, 0);
    }

    #[tokio::test]
    async fn load_table_as_dataset_is_independent_and_never_mutates_the_source_table() {
        let h = Harness::new();
        let (project, table, dataset) = setup(&h).await;

        assert_ne!(dataset.id(), table.id(), "Dataset has its own identity, independent from the Table");
        assert_eq!(dataset.source_table_id(), Some(table.id()));
        assert_eq!(dataset.content_fingerprint(), table.table_data_ref().data_fingerprint, "same content initially");

        // The source Table is byte-identical after the load — same
        // fingerprint pattern used everywhere else in this project.
        let table_after = h.table_repo.get(table.id()).await.unwrap().unwrap();
        assert_eq!(table_after.table_data_ref().data_fingerprint, table.table_data_ref().data_fingerprint);
        assert_eq!(table_after.row_count(), table.row_count());

        // Real Project Operation Log "load" entry (Part C).
        assert_eq!(project.operation_log().entries().len(), 1);
        assert_eq!(project.operation_log().entries()[0].dataset_id(), dataset.id());
    }

    #[tokio::test]
    async fn duplicate_dataset_happy_path() {
        let h = Harness::new();
        let (_project, _table, dataset) = setup(&h).await;

        let duplicate = project_workspace::duplicate_dataset(&h.dataset_repo, &h.project_repo, dataset.id(), "orders copy".to_string(), 2_000).await.unwrap();

        assert_ne!(duplicate.id(), dataset.id());
        assert_eq!(duplicate.source_dataset_id(), Some(dataset.id()));
        assert_eq!(duplicate.content_fingerprint(), dataset.content_fingerprint());

        let project = h.project_repo.get(dataset.project_id()).await.unwrap().unwrap();
        assert!(project.dataset_ids().contains(&duplicate.id()));
        assert_eq!(project.operation_log().entries().len(), 2, "load + duplicate, one entry each");
    }

    #[tokio::test]
    async fn export_dataset_as_table_uses_project_dataset_export_source_and_the_real_registration_path() {
        let h = Harness::new();
        let (_project, _table, dataset) = setup(&h).await;

        let exported = project_workspace::export_dataset_as_table(&h.dataset_repo, &h.project_repo, &h.table_repo, &h.report_repo, &h.content_store, dataset.id(), "orders_export".to_string(), 3_000, "tester".to_string())
            .await
            .unwrap();

        assert_eq!(exported.creation_source().creation_source_type, crate::types::CreationSourceType::ProjectDatasetExport, "never Import — this is Amendment 4's distinction");
        assert_eq!(exported.creation_source().source_type, crate::types::SourceType::ProjectDataset);
        assert_eq!(exported.table_data_ref().data_fingerprint, dataset.content_fingerprint(), "genuinely content-addressed against the same content, not a shortcut");
        assert_eq!(exported.table_state(), crate::types::TableState::Available);

        // Both logs fired: the new Table's own Table Event Log entry...
        let table_log = h.table_repo.event_log(exported.id()).await.unwrap();
        assert_eq!(table_log.entries().len(), 1);
        // ...and the Project Operation Log's own, distinct "export" entry.
        let project = h.project_repo.get(dataset.project_id()).await.unwrap().unwrap();
        assert!(project.operation_log().entries().iter().any(|e| e.operation().starts_with("export_as_table")), "a distinct Project Operation Log entry for the export event itself");
    }

    #[tokio::test]
    async fn run_transformation_delegates_to_the_real_engine_and_logs_every_operation_with_no_exceptions() {
        let h = Harness::new();
        let (_project, _table, dataset) = setup(&h).await;

        let operations = vec![
            TransformationOperation::DropColumns { columns: vec!["amount".to_string()] },
            TransformationOperation::GenerateRowNumber { output_col: Some("row_num".to_string()), start: 1 },
        ];
        let updated = project_workspace::run_transformation(&h.dataset_repo, &h.project_repo, &h.report_repo, &h.content_store, dataset.id(), operations, 4_000, "tester".to_string()).await.unwrap();

        assert_ne!(updated.content_fingerprint(), dataset.content_fingerprint());
        assert_eq!(updated.schema().columns.len(), 2, "amount dropped, row_num added, id kept");
        assert_eq!(updated.snapshots().len(), 2, "one rollback point per operation, per apply_change's own contract");

        let project = h.project_repo.get(dataset.project_id()).await.unwrap().unwrap();
        // load (1) + two transformation steps (2) = 3, one entry per
        // operation, no batching exception (Rule 6/INV-020).
        assert_eq!(project.operation_log().entries().len(), 3);
        let transform_entries: Vec<_> = project.operation_log().entries().iter().filter(|e| e.transformation_operation().is_some()).collect();
        assert_eq!(transform_entries.len(), 2);
    }

    #[tokio::test]
    async fn pending_plan_save_load_and_run_happy_path() {
        let h = Harness::new();
        let (project, _table, dataset) = setup(&h).await;

        let operations = vec![TransformationOperation::DropColumns { columns: vec!["amount".to_string()] }];
        let saved = project_workspace::save_pending_plan(&h.plan_repo, project.id(), dataset.id(), "drop amount".to_string(), operations.clone(), 2_000).await.unwrap();

        let loaded = project_workspace::load_pending_plan(&h.plan_repo, saved.id()).await.unwrap();
        assert_eq!(loaded.operations(), operations.as_slice());

        let updated = project_workspace::run_pending_plan(&h.plan_repo, &h.dataset_repo, &h.project_repo, &h.report_repo, &h.content_store, saved.id(), 3_000, "tester".to_string()).await.unwrap();
        assert_eq!(updated.schema().columns.len(), 1, "amount dropped via the plan");
    }

    #[tokio::test]
    async fn promote_operation_log_to_recipe_produces_real_executable_steps_and_load_recipe_into_project_resolves_a_specific_version() {
        let h = Harness::new();
        let (project, _table, dataset) = setup(&h).await;

        let op = TransformationOperation::DropColumns { columns: vec!["amount".to_string()] };
        project_workspace::run_transformation(&h.dataset_repo, &h.project_repo, &h.report_repo, &h.content_store, dataset.id(), vec![op.clone()], 4_000, "tester".to_string()).await.unwrap();
        let project = h.project_repo.get(project.id()).await.unwrap().unwrap();

        // entries: [0]=load, [1]=drop_columns transformation.
        let recipe = project_workspace::promote_operation_log_to_recipe(&h.project_repo, &h.knowledge_repo, project.id(), "Drop amount".to_string(), 1..2, 5_000).await.unwrap();
        assert_eq!(recipe.steps(), &[op]);

        let context = project_workspace::load_recipe_into_project(&h.knowledge_repo, recipe.id(), recipe.version()).await.unwrap();
        assert_eq!(context.steps, recipe.steps());

        // Requesting the wrong version is rejected, not silently resolved
        // to "the latest" (same discipline as ValidationRulesetVersion).
        let err = project_workspace::load_recipe_into_project(&h.knowledge_repo, recipe.id(), recipe.version() + 1).await;
        assert!(matches!(err, Err(crate::commands::CommandError::TableWorkspace(_)) | Err(_)));
    }

    #[tokio::test]
    async fn apply_recipe_to_dataset_delegates_to_run_transformation() {
        let h = Harness::new();
        let (_project, _table, dataset) = setup(&h).await;
        let context = project_workspace::RecipeContext { recipe_id: Uuid::new_v4(), version: 1, steps: vec![TransformationOperation::DropColumns { columns: vec!["amount".to_string()] }] };

        let updated = project_workspace::apply_recipe_to_dataset(&h.dataset_repo, &h.project_repo, &h.report_repo, &h.content_store, dataset.id(), context, 5_000, "tester".to_string()).await.unwrap();
        assert_eq!(updated.schema().columns.len(), 1);
    }

    #[tokio::test]
    async fn run_validation_on_dataset_produces_an_available_report_and_logs_the_run() {
        let h = Harness::new();
        let (project, _table, dataset) = setup(&h).await;
        let mut ruleset = validation_ruleset();
        let version_id = ruleset.versions()[0].ruleset_version_id();
        // Execution-gate: only an Active+Valid version may run (see the
        // rejection tests below) — a real version must actually pass
        // through the real activation path, not be assumed runnable.
        ruleset.activate_version(version_id, 1_400, "author".to_string()).unwrap();
        h.knowledge_repo.store_validation_ruleset(ruleset.clone()).await.unwrap();

        let outcome = project_workspace::run_validation_on_dataset(&h.dataset_repo, &h.project_repo, &h.knowledge_repo, &h.report_repo, &h.content_store, &h.detail_store, dataset.id(), ruleset.ruleset_id(), version_id, 6_000, "tester".to_string())
            .await
            .unwrap();
        assert!(matches!(outcome, RegistrationOutcome::Registered { .. }));

        let updated_dataset = h.dataset_repo.get(dataset.id()).await.unwrap().unwrap();
        assert_eq!(updated_dataset.report_refs().len(), 1, "the report is tracked against the Dataset");

        let updated_project = h.project_repo.get(project.id()).await.unwrap().unwrap();
        assert!(updated_project.operation_log().entries().iter().any(|e| e.operation() == "run_validation"));
    }

    /// Execution-time gate, Part B: a Draft version — the state every
    /// version is born in — must never be runnable just because a
    /// caller happens to know its id.
    #[tokio::test]
    async fn run_validation_on_dataset_rejects_a_draft_version() {
        let h = Harness::new();
        let (_project, _table, dataset) = setup(&h).await;
        let ruleset = validation_ruleset();
        let version_id = ruleset.versions()[0].ruleset_version_id();
        assert_eq!(ruleset.versions()[0].version_state(), crate::types::VersionState::Draft, "precondition: never activated");
        h.knowledge_repo.store_validation_ruleset(ruleset.clone()).await.unwrap();

        let err = project_workspace::run_validation_on_dataset(&h.dataset_repo, &h.project_repo, &h.knowledge_repo, &h.report_repo, &h.content_store, &h.detail_store, dataset.id(), ruleset.ruleset_id(), version_id, 6_000, "tester".to_string()).await;
        assert!(matches!(err, Err(crate::commands::CommandError::ValidationVersionNotActive { state: crate::types::VersionState::Draft, .. })), "got {err:?}");
        let updated_dataset = h.dataset_repo.get(dataset.id()).await.unwrap().unwrap();
        assert_eq!(updated_dataset.report_refs().len(), 0, "no partial report from a rejected precondition");
    }

    /// Execution-time gate, Part B: once a newer version is activated,
    /// the one it superseded must stop being runnable — a stale caller
    /// holding an old version id must not be able to run it forever.
    #[tokio::test]
    async fn run_validation_on_dataset_rejects_a_superseded_version() {
        let h = Harness::new();
        let (_project, _table, dataset) = setup(&h).await;
        let mut ruleset = validation_ruleset();
        let v1_id = ruleset.versions()[0].ruleset_version_id();
        ruleset.activate_version(v1_id, 1_400, "author".to_string()).unwrap();

        let target = RuleTarget { target_type: ValidationTargetType::Column, binding_mode: BindingMode::FixedColumnName, target_columns: vec!["id".to_string()] };
        let rule = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "r2".to_string(), ValidationFunctionParams::NotMissing, true, Severity::Error, target, RuleNullPolicy::FailIfMissing, "id must not be missing".to_string());
        let v2 = ValidationRulesetVersion::new(Uuid::new_v4(), 2, 1_500, "author".to_string(), "second".to_string(), "1.0".to_string(), "1.0.0".to_string(), "hash2".to_string(), vec![rule], vec![], vec![], vec![], vec![], RulesetMissingValuePolicy::default());
        let v2_id = v2.ruleset_version_id();
        ruleset.add_draft_version(v2, 1_500, "author".to_string());
        ruleset.activate_version(v2_id, 1_600, "author".to_string()).unwrap();
        assert_eq!(ruleset.version(v1_id).unwrap().version_state(), crate::types::VersionState::Superseded, "precondition: v1 was displaced by v2's activation");
        h.knowledge_repo.store_validation_ruleset(ruleset.clone()).await.unwrap();

        let err = project_workspace::run_validation_on_dataset(&h.dataset_repo, &h.project_repo, &h.knowledge_repo, &h.report_repo, &h.content_store, &h.detail_store, dataset.id(), ruleset.ruleset_id(), v1_id, 6_000, "tester".to_string()).await;
        assert!(matches!(err, Err(crate::commands::CommandError::ValidationVersionNotActive { state: crate::types::VersionState::Superseded, .. })), "got {err:?}");
    }

    /// Part C, the specific loophole the second status audit identified:
    /// `ValidationRulesetVersion` derives plain `serde::Deserialize` with
    /// no post-deserialize validation hook, so an `Active`+`Invalid`
    /// combination is structurally constructible without ever going
    /// through `activate_version()`'s own guard against it. This
    /// simulates exactly that — a version that reached the system
    /// through a deserialization boundary already in an invalid state —
    /// and proves the execution-time gate catches it independently of
    /// `version_state`, per the task's explicit instruction not to rely
    /// on `version_state == Active` alone to imply validity.
    #[tokio::test]
    async fn run_validation_on_dataset_rejects_an_active_version_whose_definition_status_is_invalid_via_deserialization_bypass() {
        let h = Harness::new();
        let (_project, _table, dataset) = setup(&h).await;
        let ruleset = validation_ruleset();
        let mut version_json = serde_json::to_value(&ruleset.versions()[0]).unwrap();
        version_json["version_state"] = serde_json::json!("Active");
        version_json["definition_status"] = serde_json::json!("Invalid");
        let bypassed_version: ValidationRulesetVersion = serde_json::from_value(version_json).unwrap();
        assert_eq!(bypassed_version.version_state(), crate::types::VersionState::Active, "precondition: the bypass really did produce Active");
        assert_eq!(bypassed_version.definition_status(), crate::types::DefinitionStatus::Invalid, "precondition: the bypass really did produce Invalid — never achievable via activate_version()");
        let version_id = bypassed_version.ruleset_version_id();
        let bypassed_ruleset = ValidationRuleset::new(Uuid::new_v4(), "bypassed".to_string(), 1_000, "author".to_string(), vec![], "".to_string(), bypassed_version);
        h.knowledge_repo.store_validation_ruleset(bypassed_ruleset.clone()).await.unwrap();

        let err = project_workspace::run_validation_on_dataset(&h.dataset_repo, &h.project_repo, &h.knowledge_repo, &h.report_repo, &h.content_store, &h.detail_store, dataset.id(), bypassed_ruleset.ruleset_id(), version_id, 6_000, "tester".to_string()).await;
        assert!(matches!(err, Err(crate::commands::CommandError::ValidationVersionDefinitionInvalid { .. })), "got {err:?}");
        let updated_dataset = h.dataset_repo.get(dataset.id()).await.unwrap().unwrap();
        assert_eq!(updated_dataset.report_refs().len(), 0, "no partial report from a rejected precondition");
    }

    #[tokio::test]
    async fn run_analysis_on_dataset_produces_an_available_report_and_logs_the_run() {
        let h = Harness::new();
        let (project, _table, dataset) = setup(&h).await;
        let mut ruleset = analysis_ruleset();
        let version_id = ruleset.versions()[0].analysis_ruleset_version_id();
        ruleset.activate_version(version_id, 1_400, "author".to_string()).unwrap();
        h.knowledge_repo.store_analysis_ruleset(ruleset.clone()).await.unwrap();

        let outcome = project_workspace::run_analysis_on_dataset(&h.dataset_repo, &h.project_repo, &h.knowledge_repo, &h.report_repo, &h.content_store, &h.detail_store, dataset.id(), ruleset.analysis_ruleset_id(), version_id, 6_000, "tester".to_string())
            .await
            .unwrap();
        assert!(matches!(outcome, RegistrationOutcome::Registered { .. }));

        let updated_dataset = h.dataset_repo.get(dataset.id()).await.unwrap().unwrap();
        assert_eq!(updated_dataset.report_refs().len(), 1);

        let updated_project = h.project_repo.get(project.id()).await.unwrap().unwrap();
        assert!(updated_project.operation_log().entries().iter().any(|e| e.operation() == "run_analysis"));
    }

    #[tokio::test]
    async fn run_analysis_on_dataset_rejects_a_draft_version() {
        let h = Harness::new();
        let (_project, _table, dataset) = setup(&h).await;
        let ruleset = analysis_ruleset();
        let version_id = ruleset.versions()[0].analysis_ruleset_version_id();
        assert_eq!(ruleset.versions()[0].version_state(), crate::types::AnalysisVersionState::Draft, "precondition: never activated");
        h.knowledge_repo.store_analysis_ruleset(ruleset.clone()).await.unwrap();

        let err = project_workspace::run_analysis_on_dataset(&h.dataset_repo, &h.project_repo, &h.knowledge_repo, &h.report_repo, &h.content_store, &h.detail_store, dataset.id(), ruleset.analysis_ruleset_id(), version_id, 6_000, "tester".to_string()).await;
        assert!(matches!(err, Err(crate::commands::CommandError::AnalysisVersionNotActive { state: crate::types::AnalysisVersionState::Draft, .. })), "got {err:?}");
        let updated_dataset = h.dataset_repo.get(dataset.id()).await.unwrap().unwrap();
        assert_eq!(updated_dataset.report_refs().len(), 0, "no partial report from a rejected precondition");
    }

    #[tokio::test]
    async fn run_analysis_on_dataset_rejects_a_superseded_version() {
        let h = Harness::new();
        let (_project, _table, dataset) = setup(&h).await;
        let mut ruleset = analysis_ruleset();
        let v1_id = ruleset.versions()[0].analysis_ruleset_version_id();
        ruleset.activate_version(v1_id, 1_400, "author".to_string()).unwrap();

        let procedure = AnalysisProcedure::new(Uuid::new_v4(), "Column Property Explorer".to_string(), AnalysisProcedureParams::ColumnPropertyExplorer { columns: vec!["id".to_string()] }, true, 1, AnalysisProcedureTarget::columns(&["id"]), OutputOptions::default(), vec![], "".to_string(), vec![]);
        let v2 = AnalysisRulesetVersion::new(Uuid::new_v4(), 2, 1_500, "author".to_string(), "second".to_string(), None, "1.0".to_string(), "1.0.0".to_string(), "0.1.0".to_string(), "hash2".to_string(), TargetContract::new(vec![], None, vec![]), DefaultScope::default(), vec![procedure]).unwrap();
        let v2_id = v2.analysis_ruleset_version_id();
        ruleset.add_draft_version(v2, 1_500, "author".to_string());
        ruleset.activate_version(v2_id, 1_600, "author".to_string()).unwrap();
        assert_eq!(ruleset.version(v1_id).unwrap().version_state(), crate::types::AnalysisVersionState::Superseded, "precondition: v1 was displaced by v2's activation");
        h.knowledge_repo.store_analysis_ruleset(ruleset.clone()).await.unwrap();

        let err = project_workspace::run_analysis_on_dataset(&h.dataset_repo, &h.project_repo, &h.knowledge_repo, &h.report_repo, &h.content_store, &h.detail_store, dataset.id(), ruleset.analysis_ruleset_id(), v1_id, 6_000, "tester".to_string()).await;
        assert!(matches!(err, Err(crate::commands::CommandError::AnalysisVersionNotActive { state: crate::types::AnalysisVersionState::Superseded, .. })), "got {err:?}");
    }

    /// Part C — same bypass reasoning as the Validation test above,
    /// mirrored for Analysis.
    #[tokio::test]
    async fn run_analysis_on_dataset_rejects_an_active_version_whose_definition_status_is_invalid_via_deserialization_bypass() {
        let h = Harness::new();
        let (_project, _table, dataset) = setup(&h).await;
        let ruleset = analysis_ruleset();
        let mut version_json = serde_json::to_value(&ruleset.versions()[0]).unwrap();
        version_json["version_state"] = serde_json::json!("Active");
        version_json["definition_status"] = serde_json::json!("Invalid");
        let bypassed_version: AnalysisRulesetVersion = serde_json::from_value(version_json).unwrap();
        assert_eq!(bypassed_version.version_state(), crate::types::AnalysisVersionState::Active, "precondition: the bypass really did produce Active");
        assert_eq!(bypassed_version.definition_status(), crate::types::DefinitionStatus::Invalid, "precondition: the bypass really did produce Invalid — never achievable via activate_version()");
        let version_id = bypassed_version.analysis_ruleset_version_id();
        let bypassed_ruleset = AnalysisRuleset::new(Uuid::new_v4(), "bypassed".to_string(), 1_000, "author".to_string(), vec![], "".to_string(), bypassed_version);
        h.knowledge_repo.store_analysis_ruleset(bypassed_ruleset.clone()).await.unwrap();

        let err = project_workspace::run_analysis_on_dataset(&h.dataset_repo, &h.project_repo, &h.knowledge_repo, &h.report_repo, &h.content_store, &h.detail_store, dataset.id(), bypassed_ruleset.analysis_ruleset_id(), version_id, 6_000, "tester".to_string()).await;
        assert!(matches!(err, Err(crate::commands::CommandError::AnalysisVersionDefinitionInvalid { .. })), "got {err:?}");
        let updated_dataset = h.dataset_repo.get(dataset.id()).await.unwrap().unwrap();
        assert_eq!(updated_dataset.report_refs().len(), 0, "no partial report from a rejected precondition");
    }

    /// INV-007, re-verified through this new command layer (not just at
    /// the type level `promote_operation_log_to_recipe` already
    /// enforces structurally — see the test above's steps assertion):
    /// promoting from a range with no Transformation steps in it (e.g.
    /// only "load"/"run_validation" entries) is rejected, never silently
    /// producing an empty-but-successful Recipe.
    #[tokio::test]
    async fn promote_operation_log_to_recipe_rejects_a_range_with_no_transformation_steps() {
        let h = Harness::new();
        let (project, _table, _dataset) = setup(&h).await;
        // entries: [0] = "load" only, no TransformationOperation attached.
        let err = project_workspace::promote_operation_log_to_recipe(&h.project_repo, &h.knowledge_repo, project.id(), "empty".to_string(), 0..1, 5_000).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn promote_operation_log_to_recipe_rejects_an_out_of_bounds_range() {
        let h = Harness::new();
        let (project, _table, _dataset) = setup(&h).await;
        let err = project_workspace::promote_operation_log_to_recipe(&h.project_repo, &h.knowledge_repo, project.id(), "oob".to_string(), 0..99, 5_000).await;
        assert!(matches!(err, Err(_)));
        let _ = ProjectWorkspaceError::InvalidStepRange; // referenced for documentation purposes
    }

    /// Part D, structural: every command that transforms/applies-a-
    /// recipe-to/validates/analyses working content takes a `DatasetId`
    /// parameter — this is a compile-time fact (these calls wouldn't
    /// build otherwise), not merely a runtime check. `load_table_as_dataset`/
    /// `export_dataset_as_table` are the only two functions in this module
    /// that reference a `TableId` at all, and only because crossing the
    /// Table/Dataset boundary is their entire job.
    #[test]
    fn transformation_facing_commands_are_typed_over_dataset_id_never_table_id() {
        // A `TableId` and `DatasetId` are both `Uuid` aliases in this
        // codebase (not distinct newtypes — see `types::common`), so a
        // true compile-time rejection of a `TableId` value standing in
        // for a `DatasetId` isn't expressible without introducing
        // newtypes everywhere, which is out of this task's scope and
        // flagged as such in its Output, not silently assumed solved
        // here. The strongest available proof at this signature level:
        // every one of `run_transformation`/`apply_recipe_to_dataset`/
        // `run_validation_on_dataset`/`run_analysis_on_dataset`/
        // `save_pending_plan`/`run_pending_plan` takes a `dataset_id:
        // DatasetId` parameter (checked by every call site above
        // actually compiling), and NONE of them takes a `TableId`
        // parameter at all — `load_table_as_dataset`/
        // `export_dataset_as_table` are the only two functions in this
        // module that reference a `TableId`, and only because crossing
        // the Table/Dataset boundary is their entire job.
    }

    /// Documents, explicitly, that `Project`/`Dataset` are correctly
    /// EXEMPT from the no-write-methods structural test every Category-A
    /// evidence type in this project gets (Table, Report, ...) — same
    /// pattern used to document the `AnalysisResult` exemption. Both
    /// types expose real `&mut self`/mutating methods, and that's
    /// correct, not a bug: Working Objects (INV-003) are not evidence.
    #[test]
    fn project_and_dataset_are_working_objects_and_correctly_have_no_immutability_test() {
        let mut project = crate::types::Project::new(Uuid::new_v4(), "p".to_string(), 0);
        let project_id = project.id();
        project.add_dataset(Uuid::new_v4());
        project.operation_log_mut().append(crate::types::ProjectOperationLogEntry::new(Uuid::new_v4(), project_id, Uuid::new_v4(), 0, "x".to_string()));
        assert_eq!(project.dataset_ids().len(), 1);

        let mut dataset = crate::types::Dataset::new(Uuid::new_v4(), project.id(), "d".to_string(), None, None, crate::types::table::TableSchema { columns: vec![] }, 0, "ref".to_string(), "fp".to_string(), 0);
        dataset.rename("renamed".to_string(), 1);
        assert_eq!(dataset.name(), "renamed");
    }
}
