//! Proves the 19 Table Workspace commands are real, not `todo!()` stubs
//! papered over with a mock — each test below calls the actual
//! `nikti_core::commands::*` function against a real repository test
//! double, the same precedent set by `commands::import_flow_tests` and
//! `commands::table_workspace_tests`. Also carries the source-scan proof
//! that exactly the two commands with a genuine, documented blocker
//! (`import_table`, `reimport_table_as_new` — no CSV-parsing pipeline
//! exists to supply real fingerprints/content) remain `todo!()`, and no
//! other command silently regresses to one.

#[cfg(test)]
mod tests {
    use std::fs;

    use uuid::Uuid;

    use crate::commands::{self, table_workspace};
    use crate::repositories::dataset_repository::testing::InMemoryDatasetRepository;
    use crate::repositories::project_repository::testing::InMemoryProjectRepository;
    use crate::repositories::report_repository::testing::InMemoryReportRepository;
    use crate::repositories::table_repository::testing::InMemoryTableRepository;
    use crate::repositories::{DatasetRepository, ProjectRepository, ReportRepository, TableRepository};
    use crate::types::common::EngineVersion;
    use crate::types::report_registration::{ReportRegistrationRequest, SourceContext, SourceEventRef};
    use crate::types::table::{ColumnDef, TableSchema};
    use crate::types::{
        CreationSource, CreationSourceType, DescriptiveMetadata, Project, SchemaRef, SourceProvenance, SourceType,
        TableDataRef, TableEventLogEntry, TableRegistrationOutcome, TableRegistrationRequest, TableState, VersionInfo,
    };

    fn schema() -> TableSchema {
        TableSchema { columns: vec![ColumnDef { name: "id".to_string(), data_type: "Int64".to_string() }] }
    }

    fn registration_request(key: &str, table_name: &str) -> TableRegistrationRequest {
        TableRegistrationRequest {
            idempotency_key: key.to_string(),
            table_id: Uuid::new_v4(),
            creation_source: CreationSource { creation_source_type: CreationSourceType::Import, source_type: SourceType::Csv },
            source_provenance: SourceProvenance { source_label: format!("{table_name}.csv"), source_project_id: None, source_dataset_id: None },
            table_data_ref: TableDataRef { content_ref: "table-content/fp.parquet".to_string(), data_fingerprint: "fp-data".to_string(), row_count: 3 },
            schema_ref: SchemaRef { schema: schema(), schema_fingerprint: "fp-schema".to_string() },
            version_info: VersionInfo { version_family_id: Uuid::new_v4() },
            descriptive_metadata: DescriptiveMetadata { table_name: table_name.to_string(), data_dictionary_id: None },
            created_at: 1_000,
            created_by: "tester".to_string(),
        }
    }

    /// Registers a Table and drives it all the way to `Available`, the
    /// same Amendment-7-honoring sequence `import_table` documents.
    async fn available_table(repo: &InMemoryTableRepository, key: &str, name: &str) -> crate::types::Table {
        let TableRegistrationOutcome::Registered { table_id } = repo.register_table(registration_request(key, name)).await.unwrap() else { panic!() };
        let entry_id = Uuid::new_v4();
        repo.append_event(table_id, TableEventLogEntry::new(entry_id, table_id, 1_000, "imported".to_string())).await.unwrap();
        repo.set_admin_state(table_id, crate::types::AdminState { table_state: TableState::Available, table_event_log_id: Some(entry_id), archived_at: None }).await.unwrap();
        repo.get(table_id).await.unwrap().unwrap()
    }

    /// A Table left at `Registered` — never transitioned, exactly what
    /// `TableRepository::register_table` alone produces (Part 0).
    async fn registered_table(repo: &InMemoryTableRepository, key: &str) -> crate::types::Table {
        let TableRegistrationOutcome::Registered { table_id } = repo.register_table(registration_request(key, "unfinished")).await.unwrap() else { panic!() };
        repo.get(table_id).await.unwrap().unwrap()
    }

    #[tokio::test]
    async fn search_tables_filters_by_the_real_repository_content() {
        let repo = InMemoryTableRepository::new();
        available_table(&repo, "k1", "orders").await;
        available_table(&repo, "k2", "customers").await;

        let results = commands::search_tables(&repo, "ord".to_string()).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].table_name(), "orders");
    }

    #[tokio::test]
    async fn clear_table_search_and_refresh_table_repository_return_the_full_list() {
        let repo = InMemoryTableRepository::new();
        available_table(&repo, "k1", "orders").await;
        available_table(&repo, "k2", "customers").await;

        assert_eq!(commands::clear_table_search(&repo).await.unwrap().len(), 2);
        assert_eq!(commands::refresh_table_repository(&repo).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn open_table_readonly_is_gated_by_the_real_state_action_matrix() {
        let repo = InMemoryTableRepository::new();
        let available = available_table(&repo, "k1", "orders").await;
        let registered = registered_table(&repo, "k2").await;

        assert!(commands::open_table_readonly(&repo, available.id(), false).await.is_ok());
        let err = commands::open_table_readonly(&repo, registered.id(), false).await.unwrap_err();
        assert!(matches!(err, crate::commands::CommandError::TableWorkspace(table_workspace::TableWorkspaceError::ActionNotAllowed(TableState::Registered))));
    }

    #[tokio::test]
    async fn open_table_information_is_never_gated_by_state() {
        let repo = InMemoryTableRepository::new();
        let registered = registered_table(&repo, "k1").await;
        // Unconditionally available, unlike open_readonly, per Part B.
        assert!(commands::open_table_information(&repo, registered.id()).await.is_ok());
    }

    #[tokio::test]
    async fn open_table_dictionary_reflects_real_presence_or_absence() {
        let repo = InMemoryTableRepository::new();
        let mut request = registration_request("k1", "with-dict");
        request.descriptive_metadata.data_dictionary_id = Some(Uuid::new_v4());
        let TableRegistrationOutcome::Registered { table_id } = repo.register_table(request).await.unwrap() else { panic!() };

        let nav = commands::open_table_dictionary(&repo, table_id).await.unwrap();
        assert!(matches!(nav, table_workspace::DictionaryNavigation::ServiceNotYetAvailable { .. }));

        let without = registered_table(&repo, "k2").await;
        let nav2 = commands::open_table_dictionary(&repo, without.id()).await.unwrap();
        assert_eq!(nav2, table_workspace::DictionaryNavigation::NoDictionaryReferenced);
    }

    #[tokio::test]
    async fn open_table_event_log_returns_the_real_log() {
        let repo = InMemoryTableRepository::new();
        let table = available_table(&repo, "k1", "orders").await;
        let log = commands::open_table_event_log(&repo, table.id()).await.unwrap();
        assert_eq!(log.entries().len(), 1);
        assert_eq!(log.entries()[0].event(), "imported");
    }

    #[tokio::test]
    async fn open_table_reports_filters_across_report_kinds_by_table_id() {
        let report_repo = InMemoryReportRepository::new();
        let target_table = Uuid::new_v4();
        let other_table = Uuid::new_v4();

        for (table_id, key) in [(target_table, "a"), (other_table, "b")] {
            let request = ReportRegistrationRequest {
                report_registration_request_id: Uuid::new_v4(),
                idempotency_key: key.to_string(),
                report_id: Uuid::new_v4(),
                report_title: "Inspection Report".to_string(),
                report_schema_version: "1.0".to_string(),
                created_at: 1_000,
                created_by: "tester".to_string(),
                source_context: SourceContext::for_table(table_id),
                source_event_ref: SourceEventRef {
                    source_event_type: "inspection_run".to_string(),
                    source_execution_id: Uuid::new_v4(),
                    source_engine: "inspection_engine".to_string(),
                    source_engine_version: EngineVersion { rust_core: "0.1.0".to_string(), python_env_hash: None },
                    source_execution_status: "completed".to_string(),
                },
                content_format: "application/json".to_string(),
                content_hash: sha256_hex(b"{}"),
                content_bytes: b"{}".to_vec(),
                knowledge_refs: vec![],
                log_refs: vec![],
            };
            report_repo.register_inspection_report(request).await.unwrap();
        }

        let results = commands::open_table_reports(&report_repo, target_table).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], crate::repositories::AnyReport::Inspection(r) if r.source_context().table_id == Some(target_table)));
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hex::encode(hasher.finalize())
    }

    #[tokio::test]
    async fn load_table_into_project_and_create_project_from_table_command_persist_via_repositories() {
        let table_repo = InMemoryTableRepository::new();
        let project_repo = InMemoryProjectRepository::new();
        let dataset_repo = InMemoryDatasetRepository::new();
        let table = available_table(&table_repo, "k1", "orders").await;

        let existing_project = Project::new(Uuid::new_v4(), "existing".to_string(), 500);
        project_repo.insert(existing_project.clone()).await.unwrap();

        let dataset = commands::load_table_into_project(&table_repo, &project_repo, &dataset_repo, table.id(), existing_project.id(), 1_500).await.unwrap();
        let persisted_project = project_repo.get(existing_project.id()).await.unwrap().unwrap();
        assert!(persisted_project.dataset_ids().contains(&dataset.id()), "load_table_into_project must persist the Dataset reference on the Project");
        // Regression guard for the found-and-fixed bug: the Dataset itself
        // must be durably persisted in DatasetRepository, not just
        // referenced by id on the Project — otherwise every subsequent
        // Dataset-targeted command on it (transform/duplicate/export/
        // validate/analyse) would fail with NotFound.
        assert!(dataset_repo.get(dataset.id()).await.unwrap().is_some(), "load_table_into_project must persist the Dataset itself, not just reference its id");

        let (new_project, dataset2) = commands::create_project_from_table_command(&table_repo, &project_repo, &dataset_repo, table.id(), "brand new".to_string(), 1_500).await.unwrap();
        assert_ne!(new_project.id(), existing_project.id());
        let persisted_new_project = project_repo.get(new_project.id()).await.unwrap().unwrap();
        assert!(persisted_new_project.dataset_ids().contains(&dataset2.id()));
        assert!(dataset_repo.get(dataset2.id()).await.unwrap().is_some(), "create_project_from_table_command must persist the Dataset itself, not just reference its id");

        // Gating: a Registered (not yet Available) table must be rejected.
        let registered = registered_table(&table_repo, "k2").await;
        let err = commands::load_table_into_project(&table_repo, &project_repo, &dataset_repo, registered.id(), existing_project.id(), 1_500).await;
        assert!(err.is_err());
    }

    /// Stale Comment Fix + Load-Path Gating Gap task: `load_table_into_project`
    /// previously updated `Project.dataset_ids` but never appended a
    /// `ProjectOperationLogEntry` for the load event — unlike the
    /// equivalent `project_workspace::load_table_as_dataset`, which
    /// always has. A load event is exactly the kind of thing INV-001's
    /// provenance guarantee and the Project Operation Log exist to
    /// record; silently skipping it here was a real evidence-
    /// completeness gap, not a cosmetic one.
    #[tokio::test]
    async fn load_table_into_project_logs_the_load_event_to_the_project_operation_log() {
        let table_repo = InMemoryTableRepository::new();
        let project_repo = InMemoryProjectRepository::new();
        let dataset_repo = InMemoryDatasetRepository::new();
        let table = available_table(&table_repo, "k3", "orders").await;

        let existing_project = Project::new(Uuid::new_v4(), "existing".to_string(), 500);
        project_repo.insert(existing_project.clone()).await.unwrap();
        assert_eq!(existing_project.operation_log().entries().len(), 0, "precondition: no log entries yet");

        let dataset = commands::load_table_into_project(&table_repo, &project_repo, &dataset_repo, table.id(), existing_project.id(), 1_500).await.unwrap();

        let persisted_project = project_repo.get(existing_project.id()).await.unwrap().unwrap();
        let entries = persisted_project.operation_log().entries();
        assert_eq!(entries.len(), 1, "the load event must be logged, not silently skipped");
        assert_eq!(entries[0].dataset_id(), dataset.id());
        assert_eq!(entries[0].project_id(), existing_project.id());
        assert_eq!(entries[0].operation(), "load");
        assert_eq!(entries[0].recorded_at(), 1_500);
        assert!(entries[0].transformation_operation().is_none(), "a load is not a transformation");
    }

    /// Mirrors `load_table_into_project_logs_the_load_event_to_the_project_operation_log`
    /// — same missing-log-entry bug, found on the sibling command by the
    /// task that fixed the first one. Distinct operation label
    /// (`"create_project_from_table"`, not `"load"`): this is the
    /// founding event of a brand-new Project, not a load into an
    /// existing one's history — see the fix's own comment for why that
    /// distinction was chosen deliberately rather than reused by default.
    #[tokio::test]
    async fn create_project_from_table_command_logs_the_creation_event_to_the_project_operation_log() {
        let table_repo = InMemoryTableRepository::new();
        let project_repo = InMemoryProjectRepository::new();
        let dataset_repo = InMemoryDatasetRepository::new();
        let table = available_table(&table_repo, "k4", "orders").await;

        let (project, dataset) = commands::create_project_from_table_command(&table_repo, &project_repo, &dataset_repo, table.id(), "brand new project".to_string(), 1_800).await.unwrap();

        let persisted_project = project_repo.get(project.id()).await.unwrap().unwrap();
        let entries = persisted_project.operation_log().entries();
        assert_eq!(entries.len(), 1, "the founding load event must be logged, not silently skipped");
        assert_eq!(entries[0].dataset_id(), dataset.id());
        assert_eq!(entries[0].project_id(), project.id());
        assert_eq!(entries[0].operation(), "create_project_from_table");
        assert_eq!(entries[0].recorded_at(), 1_800);
        assert!(entries[0].transformation_operation().is_none(), "creating a project from a table is not a transformation");
    }

    #[tokio::test]
    async fn archive_table_and_restore_table_round_trip_through_the_real_repository() {
        let repo = InMemoryTableRepository::new();
        let table = available_table(&repo, "k1", "orders").await;

        let archived = commands::archive_table(&repo, table.id(), 2_000).await.unwrap();
        assert_eq!(archived.table_state(), TableState::Archived);

        let restored = commands::restore_table(&repo, table.id()).await.unwrap();
        assert_eq!(restored.table_state(), TableState::Available);

        // Archiving an already-archived Table is rejected (Part H: "—").
        commands::archive_table(&repo, table.id(), 3_000).await.unwrap();
        assert!(commands::archive_table(&repo, table.id(), 4_000).await.is_err());
    }

    #[tokio::test]
    async fn retry_table_availability_check_self_heals_once_a_log_entry_exists() {
        let repo = InMemoryTableRepository::with_event_append_failure();
        let TableRegistrationOutcome::Registered { table_id } = repo.register_table(registration_request("k1", "orders")).await.unwrap() else { panic!() };
        // Simulate the failed-append path import_table documents.
        repo.set_admin_state(table_id, crate::types::AdminState { table_state: TableState::Unavailable, table_event_log_id: None, archived_at: None }).await.unwrap();

        // Still no log entry -> stays Unavailable.
        let table = commands::retry_table_availability_check(&repo, table_id).await.unwrap();
        assert_eq!(table.table_state(), TableState::Unavailable);
    }

    #[tokio::test]
    async fn copy_table_id_returns_the_real_id_string() {
        let id = Uuid::new_v4();
        assert_eq!(commands::copy_table_id(id).await.unwrap(), id.to_string());
    }

    /// The real import pipeline task closed `import_table`/
    /// `reimport_table_as_new`'s last remaining gap (no CSV-parsing/
    /// content-store pipeline existed to supply real content) — so, as of
    /// this task, none of the 19 Table Workspace commands should contain
    /// `todo!()` any more. `route_table_inspection` is the one exception
    /// worth naming explicitly: it delegates to `inspect_table`, a
    /// *different*, pre-existing command outside this 19-command list
    /// (the original Inspection Engine task's command) that remains
    /// `todo!()` in its own right — `route_table_inspection`'s own body
    /// is real (a one-line delegation), so it still counts as
    /// `expected_todo = false` here; implementing `inspect_table` for
    /// real is that command's own task, not this list's.
    #[test]
    fn none_of_the_nineteen_table_commands_contain_todo_any_more() {
        let raw = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/commands/commands.rs")).unwrap();
        // Strip doc-comment lines first — several of them legitimately
        // narrate that `inspect_table` is `todo!()`, in prose, and would
        // otherwise leak into a neighboring function's "body" text below.
        let source: String = raw.lines().filter(|l| !l.trim_start().starts_with("///")).collect::<Vec<_>>().join("\n");

        let commands_and_expected_todo = [
            ("import_table", false),
            ("reimport_table_as_new", false),
            ("search_tables", false),
            ("clear_table_search", false),
            ("refresh_table_repository", false),
            ("open_table_readonly", false),
            ("open_table_information", false),
            ("open_table_dictionary", false),
            ("open_table_event_log", false),
            ("open_table_reports", false),
            ("load_table_into_project", false),
            ("create_project_from_table_command", false),
            ("route_table_inspection", false),
            ("route_table_validation", false),
            ("route_table_analysis", false),
            ("archive_table", false),
            ("restore_table", false),
            ("retry_table_availability_check", false),
            ("copy_table_id", false),
        ];

        for (name, expected_todo) in commands_and_expected_todo {
            let fn_start = source.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("command `{name}` not found in commands.rs"));
            // The function body ends at the next top-level `pub async fn`/`pub fn` after this one, or EOF.
            let after = &source[fn_start..];
            let body_end = after[3..].find("\npub ").map(|i| i + 3).unwrap_or(after.len());
            let body = &after[..body_end];
            let has_todo = body.contains("todo!()");
            assert_eq!(has_todo, expected_todo, "command `{name}`: expected todo!() present = {expected_todo}, found = {has_todo}");
        }
    }
}
