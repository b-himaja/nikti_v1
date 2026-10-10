//! Tests for the "Sweep commands.rs for unwired real implementations"
//! task. Three things, per that task's explicit requirements:
//!
//! 1. One test per newly-wired command (`create_project`, `transform_dataset`,
//!    `apply_recipe`, `purge_table`) proving each now calls its real
//!    implementation — not merely that it no longer panics. Each test
//!    asserts something only the real delegation target could produce
//!    (a persisted repository row, a changed schema + an appended
//!    Project Operation Log entry, a resolved+applied recipe step, a
//!    real fingerprint carried into a `PurgedRef`).
//! 2. A regression test confirming the 18 already-fixed Table Repository
//!    commands untouched by this sweep still work end to end together,
//!    through the exact same commands.rs this task edited.
//! 3. A source-scan completeness test: every `commands.rs` command must
//!    be free of `todo!()`/`unimplemented!()` unless it's one of the ten
//!    names this task's Output explicitly lists as a genuine gap.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use uuid::Uuid;

    use crate::commands::project_workspace;
    use crate::commands::{self, ImportSource};
    use crate::repositories::dataset_repository::testing::InMemoryDatasetRepository;
    use crate::repositories::knowledge_repository::testing::InMemoryKnowledgeRepository;
    use crate::repositories::project_repository::testing::InMemoryProjectRepository;
    use crate::repositories::report_repository::testing::InMemoryReportRepository;
    use crate::repositories::table_repository::testing::InMemoryTableRepository;
    use crate::repositories::{KnowledgeRepository, ParquetContentStore, ProjectRepository, TableRepository};
    use crate::types::{Recipe, TransformationOperation};

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("nikti-sweep-wiring-test-{}", Uuid::new_v4()));
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

    // ---- 1. One test per newly-wired command ----------------------------

    /// `commands::create_project` must genuinely persist the Project via
    /// `ProjectRepository`, exactly what `project_workspace::create_project`
    /// does — a `todo!()` stub could never make this assertion pass, and
    /// neither could a fake that just constructed a `Project` value and
    /// handed it back without inserting it anywhere.
    #[tokio::test]
    async fn create_project_delegates_to_project_workspace_create_project_and_actually_persists() {
        let project_repo = InMemoryProjectRepository::new();

        let created = commands::create_project(&project_repo, "orders project".to_string(), 1_000).await.unwrap();

        let persisted = project_repo.get(created.id()).await.unwrap().unwrap();
        assert_eq!(persisted.name(), "orders project");
        assert_eq!(persisted.dataset_ids().len(), 0);
    }

    async fn dataset_via_real_import(dataset_repo: &InMemoryDatasetRepository, table_repo: &InMemoryTableRepository, project_repo: &InMemoryProjectRepository, report_repo: &InMemoryReportRepository, content_store: &ParquetContentStore, scratch: &TempDir) -> crate::types::Dataset {
        let uri = write_csv(scratch.path(), "orders.csv", "id,amount\n1,10.5\n2,20.0\n3,30.0\n");
        let table = commands::import_table(table_repo, report_repo, content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();
        let project = project_workspace::create_project(project_repo, "P".to_string(), 1_000).await.unwrap();
        project_workspace::load_table_as_dataset(table_repo, dataset_repo, project_repo, project.id(), table.id(), "orders".to_string(), 1_500).await.unwrap()
    }

    /// `commands::transform_dataset` must genuinely delegate to
    /// `project_workspace::run_transformation` — proven by the two things
    /// only that real sequence produces: a changed content fingerprint/
    /// schema (via `NativeTransformationEngine::transform` +
    /// `Dataset::apply_change`) AND an appended `ProjectOperationLogEntry`
    /// carrying the `TransformationOperation` (Rule 6/INV-020). A stub
    /// that merely returned the dataset unchanged, or one that mutated it
    /// without logging, would fail this.
    #[tokio::test]
    async fn transform_dataset_delegates_to_run_transformation_and_both_mutates_and_logs() {
        let table_repo = InMemoryTableRepository::new();
        let dataset_repo = InMemoryDatasetRepository::new();
        let project_repo = InMemoryProjectRepository::new();
        let report_repo = InMemoryReportRepository::new();
        let dir = TempDir::new();
        let content_store = ParquetContentStore::new(dir.path().join("content")).unwrap();
        let scratch = TempDir::new();
        let dataset = dataset_via_real_import(&dataset_repo, &table_repo, &project_repo, &report_repo, &content_store, &scratch).await;

        let operation = TransformationOperation::DropColumns { columns: vec!["amount".to_string()] };
        let updated = commands::transform_dataset(&dataset_repo, &project_repo, &report_repo, &content_store, dataset.id(), operation.clone(), 4_000, "tester".to_string()).await.unwrap();

        assert_ne!(updated.content_fingerprint(), dataset.content_fingerprint());
        assert_eq!(updated.schema().columns.len(), 1, "amount dropped, id kept");

        let project = project_repo.get(dataset.project_id()).await.unwrap().unwrap();
        let transform_entries: Vec<_> = project.operation_log().entries().iter().filter(|e| e.transformation_operation() == Some(&operation)).collect();
        assert_eq!(transform_entries.len(), 1, "the exact operation passed in must be the one that got logged");
    }

    /// `commands::apply_recipe` must genuinely resolve `recipe_id` via
    /// `KnowledgeRepository::get_recipe` and then delegate to
    /// `project_workspace::apply_recipe_to_dataset` — proven by the
    /// Dataset actually reflecting the Recipe's own steps (not some other
    /// operation), and by looking up a nonexistent recipe id correctly
    /// failing rather than silently no-op'ing.
    #[tokio::test]
    async fn apply_recipe_resolves_the_real_recipe_and_applies_its_own_steps() {
        let table_repo = InMemoryTableRepository::new();
        let dataset_repo = InMemoryDatasetRepository::new();
        let project_repo = InMemoryProjectRepository::new();
        let report_repo = InMemoryReportRepository::new();
        let knowledge_repo = InMemoryKnowledgeRepository::new();
        let dir = TempDir::new();
        let content_store = ParquetContentStore::new(dir.path().join("content")).unwrap();
        let scratch = TempDir::new();
        let dataset = dataset_via_real_import(&dataset_repo, &table_repo, &project_repo, &report_repo, &content_store, &scratch).await;

        let recipe = Recipe::new(Uuid::new_v4(), "Drop amount".to_string(), vec![TransformationOperation::DropColumns { columns: vec!["amount".to_string()] }], Uuid::new_v4(), 2_000);
        knowledge_repo.insert_recipe(recipe.clone()).await.unwrap();

        let updated = commands::apply_recipe(&dataset_repo, &project_repo, &knowledge_repo, &report_repo, &content_store, dataset.id(), recipe.id(), 5_000, "tester".to_string()).await.unwrap();
        assert_eq!(updated.schema().columns.len(), 1, "the recipe's own DropColumns step, genuinely applied");

        let missing_recipe_id = Uuid::new_v4();
        let err = commands::apply_recipe(&dataset_repo, &project_repo, &knowledge_repo, &report_repo, &content_store, dataset.id(), missing_recipe_id, 5_000, "tester".to_string()).await;
        assert!(err.is_err(), "a recipe id that was never inserted must not silently succeed");
    }

    /// `commands::purge_table` must genuinely call `TableRepository::purge`
    /// with a `PurgedRef` carrying the Table's REAL `data_fingerprint`
    /// (not a placeholder) — proven by reading it back via
    /// `get_purged_ref` and comparing against the Table's own fingerprint,
    /// computed independently by the real import pipeline.
    #[tokio::test]
    async fn purge_table_records_a_purged_ref_with_the_tables_real_fingerprint() {
        let table_repo = InMemoryTableRepository::new();
        let report_repo = InMemoryReportRepository::new();
        let dir = TempDir::new();
        let content_store = ParquetContentStore::new(dir.path().join("content")).unwrap();
        let scratch = TempDir::new();
        let uri = write_csv(scratch.path(), "orders.csv", "id,amount\n1,10.5\n2,20.0\n3,30.0\n");
        let table = commands::import_table(&table_repo, &report_repo, &content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();

        assert!(table_repo.get_purged_ref(table.id()).await.unwrap().is_none(), "not purged yet");

        let purged_ref = commands::purge_table(&table_repo, table.id(), "retention policy".to_string(), 9_000).await.unwrap();
        assert_eq!(purged_ref.content_fingerprint(), table.table_data_ref().data_fingerprint, "the real fingerprint, not a placeholder");

        let stored = table_repo.get_purged_ref(table.id()).await.unwrap().unwrap();
        assert_eq!(stored.reason(), "retention policy");
        assert_eq!(stored.content_fingerprint(), table.table_data_ref().data_fingerprint);
    }

    // ---- 2. Regression: the 18 already-fixed Table Repository commands --

    /// This sweep touched only `create_project`/`transform_dataset`/
    /// `apply_recipe`/`purge_table`/`create_dataset` in `commands.rs`; the
    /// 18 Table Repository commands proven real in the earlier Table
    /// Repository Workspace and Import Pipeline tasks
    /// (`commands_delegation_tests.rs`) were not touched by this task's
    /// edits at all. This test drives all of them, in one connected
    /// sequence, through the exact `commands.rs` this task edited, as an
    /// explicit collective regression proof for this task specifically —
    /// on top of (not instead of) that file's own per-command tests.
    #[tokio::test]
    async fn the_eighteen_already_fixed_table_repository_commands_are_unaffected_by_this_sweep() {
        let table_repo = InMemoryTableRepository::new();
        let project_repo = InMemoryProjectRepository::new();
        let dataset_repo = InMemoryDatasetRepository::new();
        let report_repo = InMemoryReportRepository::new();
        let dir = TempDir::new();
        let content_store = ParquetContentStore::new(dir.path().join("content")).unwrap();
        let scratch = TempDir::new();

        // 1. import_table
        let uri = write_csv(scratch.path(), "orders.csv", "id,amount\n1,10.5\n2,20.0\n");
        let table = commands::import_table(&table_repo, &report_repo, &content_store, ImportSource { uri: uri.clone() }, 1_000, "tester".to_string()).await.unwrap();

        // 2. reimport_table_as_new
        let uri2 = write_csv(scratch.path(), "orders2.csv", "id,amount\n1,10.5\n2,20.0\n");
        let reimported = commands::reimport_table_as_new(&table_repo, &report_repo, &content_store, table.id(), ImportSource { uri: uri2 }, 1_100, "tester".to_string()).await.unwrap();
        assert_ne!(reimported.id(), table.id());

        // 3. search_tables
        assert_eq!(commands::search_tables(&table_repo, "orders".to_string()).await.unwrap().len(), 2);
        // 4. clear_table_search
        assert_eq!(commands::clear_table_search(&table_repo).await.unwrap().len(), 2);
        // 5. refresh_table_repository
        assert_eq!(commands::refresh_table_repository(&table_repo).await.unwrap().len(), 2);
        // 6. open_table_readonly
        assert!(commands::open_table_readonly(&table_repo, table.id(), false).await.is_ok());
        // 7. open_table_information
        assert!(commands::open_table_information(&table_repo, table.id()).await.is_ok());
        // 8. open_table_dictionary
        assert!(commands::open_table_dictionary(&table_repo, table.id()).await.is_ok());
        // 9. open_table_event_log
        assert_eq!(commands::open_table_event_log(&table_repo, table.id()).await.unwrap().entries().len(), 1);
        // 10. open_table_reports
        assert_eq!(commands::open_table_reports(&report_repo, table.id()).await.unwrap().len(), 1, "the automatic Inspection report from import");

        // 11. load_table_into_project
        let project = commands::create_project(&project_repo, "existing".to_string(), 1_200).await.unwrap();
        let dataset = commands::load_table_into_project(&table_repo, &project_repo, &dataset_repo, table.id(), project.id(), 1_300).await.unwrap();
        let persisted_project = project_repo.get(project.id()).await.unwrap().unwrap();
        assert!(persisted_project.dataset_ids().contains(&dataset.id()));

        // 12. create_project_from_table_command
        let (new_project, dataset2) = commands::create_project_from_table_command(&table_repo, &project_repo, &dataset_repo, table.id(), "brand new".to_string(), 1_400).await.unwrap();
        assert_ne!(new_project.id(), project.id());
        assert!(project_repo.get(new_project.id()).await.unwrap().unwrap().dataset_ids().contains(&dataset2.id()));

        // 13. route_table_validation
        assert!(commands::route_table_validation(table.id(), None).await.is_ok());
        // 14. route_table_analysis
        assert!(commands::route_table_analysis(table.id(), None).await.is_ok());

        // 15. archive_table
        let archived = commands::archive_table(&table_repo, table.id(), 2_000).await.unwrap();
        assert_eq!(archived.table_state(), crate::types::TableState::Archived);
        // 16. restore_table
        let restored = commands::restore_table(&table_repo, table.id()).await.unwrap();
        assert_eq!(restored.table_state(), crate::types::TableState::Available);
        // 17. retry_table_availability_check
        assert!(commands::retry_table_availability_check(&table_repo, table.id()).await.is_ok());
        // 18. copy_table_id
        assert_eq!(commands::copy_table_id(table.id()).await.unwrap(), table.id().to_string());

        // Bonus: table_allowed_actions (not one of the 19 canonical
        // commands, but shares the same real-repository dependency).
        assert!(commands::table_allowed_actions(&table_repo, table.id(), false).await.is_ok());

        // route_table_inspection intentionally excluded: its own body is
        // real, but it delegates to `inspect_table`, itself a documented
        // genuine gap — calling it here would panic, not regress.
    }

    // ---- 3. Completeness: no undocumented todo!()/unimplemented!() ------

    /// Every command in `commands.rs` must be free of `todo!()`/
    /// `unimplemented!()` UNLESS its name appears in this exact allow-list
    /// — the genuine gaps this task's Output names (plus the one
    /// ambiguous case, `open_table`, listed alongside them since it is
    /// likewise intentionally left unresolved rather than guessed).
    /// Fixing any of these in a future task means deleting it from this
    /// list, not leaving a stale exemption around — the same discipline
    /// `commands_delegation_tests::none_of_the_nineteen_table_commands_contain_todo_any_more`
    /// already applies to the 19-command Table Repository set.
    ///
    /// `generate_lineage`/`generate_comparison` were removed here by the
    /// Audit #3 cleanup task: confirmed directly (real, non-stub
    /// `generate_lineage_report`/`generate_comparison_report` functions
    /// exist in this file, extensively tested) that both are fully
    /// wired — these two entries never matched any real function name
    /// anyway (this loop matches on exact function name, and no
    /// function here is literally named `generate_lineage` or
    /// `generate_comparison`), so removing them changes nothing this
    /// test checks; they were purely stale bookkeeping left behind once
    /// `_report` was appended to both real names.
    #[test]
    fn no_command_in_commands_rs_contains_todo_unless_explicitly_listed_as_a_genuine_gap() {
        const ALLOWED_TODO_COMMANDS: &[&str] = &[
            "create_dataset",
            "transform_dataset_many",
            "promote_recipe",
            "open_table",
            "inspect_table",
            "validate_table",
            "analyse_native",
            "analyse_python",
        ];

        let raw = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/commands/commands.rs")).unwrap();
        // Strip doc-comment lines first — several legitimately narrate
        // *other* commands' todo!() status in prose (e.g. `route_table_inspection`'s
        // doc comment mentions `inspect_table` is `todo!()`), which would
        // otherwise leak into a neighboring function's "body" text below.
        let source: String = raw.lines().filter(|l| !l.trim_start().starts_with("///")).collect::<Vec<_>>().join("\n");

        // Every `pub async fn NAME(` / `pub fn NAME(` in the file, in
        // source order, so each function's body is "everything up to the
        // next one" (or EOF for the last).
        let mut fn_starts: Vec<(String, usize)> = Vec::new();
        let mut search_from = 0usize;
        while let Some(rel) = source[search_from..].find("pub ") {
            let abs = search_from + rel;
            let after_pub = &source[abs + 4..];
            let after_pub = after_pub.strip_prefix("async fn ").or_else(|| after_pub.strip_prefix("fn "));
            if let Some(rest) = after_pub {
                if let Some(paren) = rest.find('(') {
                    let name = rest[..paren].trim().to_string();
                    if !name.is_empty() && name.chars().next().unwrap().is_alphabetic() {
                        fn_starts.push((name, abs));
                    }
                }
            }
            search_from = abs + 4;
        }
        assert!(fn_starts.len() >= 30, "sanity check: expected to find most of commands.rs's ~30 commands, found {}", fn_starts.len());

        for i in 0..fn_starts.len() {
            let (name, start) = &fn_starts[i];
            let end = fn_starts.get(i + 1).map(|(_, s)| *s).unwrap_or(source.len());
            let body = &source[*start..end];
            let has_todo = body.contains("todo!()") || body.contains("unimplemented!()");
            let allowed = ALLOWED_TODO_COMMANDS.contains(&name.as_str());
            assert_eq!(has_todo, allowed, "command `{name}`: contains todo!()/unimplemented!() = {has_todo}, but allow-listed as a genuine gap = {allowed} — either wire it, or add/remove it from ALLOWED_TODO_COMMANDS deliberately");
        }
    }
}
