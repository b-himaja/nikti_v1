#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::commands::table_workspace::{
        allowed_actions, create_project_from_table, dictionary_action_allowed, load_into_project, reimport_as_new,
        rename_table, route_to_ruleset_manager, table_action_allowed, RulesetManagerTarget, TableWorkspaceAction,
    };
    use crate::repositories::table_repository::testing::InMemoryTableRepository;
    use crate::repositories::TableRepository;
    use crate::types::table::{ColumnDef, TableSchema};
    use crate::types::{
        CreationSource, CreationSourceType, DescriptiveMetadata, SchemaRef, SourceProvenance, SourceType, TableDataRef,
        TableRegistrationOutcome, TableRegistrationRequest, TableState, VersionInfo,
    };

    fn schema() -> TableSchema {
        TableSchema { columns: vec![ColumnDef { name: "id".to_string(), data_type: "Int64".to_string() }] }
    }

    fn registration_request(idempotency_key: &str, data_dictionary_id: Option<Uuid>) -> TableRegistrationRequest {
        TableRegistrationRequest {
            idempotency_key: idempotency_key.to_string(),
            table_id: Uuid::new_v4(),
            creation_source: CreationSource { creation_source_type: CreationSourceType::Import, source_type: SourceType::Csv },
            source_provenance: SourceProvenance { source_label: "orders.csv".to_string(), source_project_id: None, source_dataset_id: None },
            table_data_ref: TableDataRef { content_ref: "table-content/fp.parquet".to_string(), data_fingerprint: "data-fp".to_string(), row_count: 3 },
            schema_ref: SchemaRef { schema: schema(), schema_fingerprint: "schema-fp".to_string() },
            version_info: VersionInfo { version_family_id: Uuid::new_v4() },
            descriptive_metadata: DescriptiveMetadata { table_name: "orders".to_string(), data_dictionary_id },
            created_at: 1_000,
            created_by: "tester".to_string(),
        }
    }

    /// Registers and returns a Table already at `Available`, mirroring the
    /// real import sequence (Amendment 7) closely enough for these tests,
    /// which care about post-import behavior, not registration itself.
    async fn available_table(repo: &InMemoryTableRepository, data_dictionary_id: Option<Uuid>) -> crate::types::Table {
        let request = registration_request(&Uuid::new_v4().to_string(), data_dictionary_id);
        let TableRegistrationOutcome::Registered { table_id } = repo.register_table(request).await.unwrap() else { panic!() };
        let log_entry_id = Uuid::new_v4();
        repo.append_event(table_id, crate::types::TableEventLogEntry::new(log_entry_id, table_id, 1_000, "imported".to_string())).await.unwrap();
        repo.set_admin_state(table_id, crate::types::AdminState { table_state: TableState::Available, table_event_log_id: Some(log_entry_id), archived_at: None }).await.unwrap();
        repo.get(table_id).await.unwrap().unwrap()
    }

    /// Part H: every disallowed combination in the full state-action
    /// matrix is actually disabled, not just documented.
    #[test]
    fn part_h_state_action_matrix_is_fully_enforced() {
        use TableState::*;
        use TableWorkspaceAction::*;

        let actions = [OpenReadonly, LoadIntoProject, RouteInspection, RouteValidation, RouteAnalysis, Archive, Restore];

        for policy in [true, false] {
            for action in actions {
                assert!(!table_action_allowed(Registered, action, policy), "Registered/{action:?} must always be disallowed");
            }

            for action in [OpenReadonly, LoadIntoProject, RouteInspection, RouteValidation, RouteAnalysis, Archive] {
                assert!(table_action_allowed(Available, action, policy), "Available/{action:?} must be allowed");
            }
            assert!(!table_action_allowed(Available, Restore, policy), "Available/Restore is not applicable");

            assert_eq!(table_action_allowed(Archived, OpenReadonly, policy), policy, "Archived/OpenReadonly follows the policy flag");
            for action in [LoadIntoProject, RouteInspection, RouteValidation, RouteAnalysis, Archive] {
                assert!(!table_action_allowed(Archived, action, policy), "Archived/{action:?} must be disallowed");
            }
            assert!(table_action_allowed(Archived, Restore, policy), "Archived/Restore must be allowed");

            for action in actions {
                assert!(!table_action_allowed(Unavailable, action, policy), "Unavailable/{action:?} must always be disallowed");
            }
        }
    }

    /// `allowed_actions` (the UI-facing query, Part C of the workspace UI
    /// task) must always agree with `table_action_allowed` field-by-field
    /// — it exists only to package that function's output for IPC, never
    /// to compute anything independently.
    #[test]
    fn allowed_actions_agrees_with_table_action_allowed_for_every_state_and_policy() {
        use TableWorkspaceAction::*;
        for state in [TableState::Registered, TableState::Available, TableState::Archived, TableState::Unavailable] {
            for policy in [true, false] {
                let snapshot = allowed_actions(state, policy);
                assert_eq!(snapshot.open_readonly, table_action_allowed(state, OpenReadonly, policy));
                assert_eq!(snapshot.load_into_project, table_action_allowed(state, LoadIntoProject, policy));
                assert_eq!(snapshot.route_inspection, table_action_allowed(state, RouteInspection, policy));
                assert_eq!(snapshot.route_validation, table_action_allowed(state, RouteValidation, policy));
                assert_eq!(snapshot.route_analysis, table_action_allowed(state, RouteAnalysis, policy));
                assert_eq!(snapshot.archive, table_action_allowed(state, Archive, policy));
                assert_eq!(snapshot.restore, table_action_allowed(state, Restore, policy));
            }
        }
    }

    /// Route Inspection/Validation/Analysis share exactly the same rule in
    /// the matrix (a single column, not three) — confirmed explicitly so a
    /// future edit that accidentally special-cases one of the three is
    /// caught.
    #[test]
    fn the_three_route_actions_are_governed_by_one_identical_rule() {
        use TableWorkspaceAction::*;
        for state in [TableState::Registered, TableState::Available, TableState::Archived, TableState::Unavailable] {
            for policy in [true, false] {
                let insp = table_action_allowed(state, RouteInspection, policy);
                let val = table_action_allowed(state, RouteValidation, policy);
                let ana = table_action_allowed(state, RouteAnalysis, policy);
                assert_eq!(insp, val);
                assert_eq!(val, ana);
            }
        }
    }

    /// `table.load_into_project` and `table.create_project_from_table` are
    /// genuinely distinct: one requires an existing target Project (and
    /// creates none), the other creates a brand-new Project — not two
    /// labels on the same underlying call.
    #[tokio::test]
    async fn load_into_project_and_create_project_from_table_are_genuinely_distinct() {
        let table_repo = InMemoryTableRepository::new();
        let table = available_table(&table_repo, None).await;

        // load_into_project requires an existing Project to be passed in —
        // it is a compile-time impossibility to call it without one.
        let existing_project = crate::types::Project::new(Uuid::new_v4(), "existing".to_string(), 500);
        let dataset = load_into_project(&existing_project, &table, Uuid::new_v4(), 1_500);
        assert_eq!(dataset.project_id(), existing_project.id());
        assert_eq!(dataset.source_table_id(), Some(table.id()));

        // create_project_from_table creates its own new Project — the
        // caller supplies only a name/id, never an existing Project value.
        let (new_project, dataset2) = create_project_from_table(Uuid::new_v4(), "brand new".to_string(), Uuid::new_v4(), &table, 1_500);
        assert_ne!(new_project.id(), existing_project.id());
        assert_eq!(new_project.name(), "brand new");
        assert_eq!(dataset2.project_id(), new_project.id());
        assert_eq!(dataset2.source_table_id(), Some(table.id()));

        // Neither action modified the Table itself.
        assert_eq!(table_repo.get(table.id()).await.unwrap().unwrap().row_count(), table.row_count());
    }

    /// Amendment 13/14: routing to Validation/Analysis (opening the
    /// Manager) never writes a Table Event Log entry by itself — only
    /// actual execution does. `route_to_ruleset_manager` takes no
    /// repository handle at all (a structural guarantee, not just a
    /// runtime check), but this also proves it behaviorally: the log is
    /// still empty after routing.
    #[tokio::test]
    async fn routing_to_validation_or_analysis_never_writes_a_table_event_log_entry() {
        let table_repo = InMemoryTableRepository::new();
        let table = available_table(&table_repo, None).await;
        assert_eq!(table_repo.event_log(table.id()).await.unwrap().entries().len(), 1, "only the import's own entry exists so far");

        let nav_v = route_to_ruleset_manager(table.id(), RulesetManagerTarget::Validation, None);
        let nav_a = route_to_ruleset_manager(table.id(), RulesetManagerTarget::Analysis, Some(Uuid::new_v4()));

        assert!(nav_v.requires_ruleset_selection, "no ruleset_version_id supplied -> route opens a selection step first");
        assert!(!nav_a.requires_ruleset_selection, "a ruleset_version_id was supplied -> no selection step needed");

        // Still exactly the one pre-existing (import) entry — routing
        // added nothing.
        assert_eq!(table_repo.event_log(table.id()).await.unwrap().entries().len(), 1);
    }

    /// Part D: a missing `data_dictionary_id` disables only the
    /// dictionary action — `table_state` and Table visibility are
    /// unaffected.
    #[tokio::test]
    async fn missing_data_dictionary_id_disables_only_the_dictionary_action() {
        let table_repo = InMemoryTableRepository::new();
        let table_without = available_table(&table_repo, None).await;
        let table_with = available_table(&table_repo, Some(Uuid::new_v4())).await;

        assert!(!dictionary_action_allowed(table_without.descriptive_metadata().data_dictionary_id));
        assert!(dictionary_action_allowed(table_with.descriptive_metadata().data_dictionary_id));

        // table_state is untouched by dictionary presence/absence either way.
        assert_eq!(table_without.table_state(), TableState::Available);
        assert_eq!(table_with.table_state(), TableState::Available);
        // Both Tables remain fully retrievable (never hidden).
        assert!(table_repo.get(table_without.id()).await.unwrap().is_some());
        assert!(table_repo.get(table_with.id()).await.unwrap().is_some());
    }

    /// Part I: `table.reimport_as_new` always produces a new `table_id`
    /// and never mutates or redirects the original Table's existing
    /// Project references (simulated here via a Dataset that already
    /// points at the original).
    #[tokio::test]
    async fn reimport_as_new_always_produces_a_new_table_id_and_never_touches_the_original() {
        let table_repo = InMemoryTableRepository::new();
        let original = available_table(&table_repo, None).await;

        // An existing "Project reference" to the original Table.
        let project = crate::types::Project::new(Uuid::new_v4(), "p".to_string(), 500);
        let existing_dataset = load_into_project(&project, &original, Uuid::new_v4(), 1_500);
        assert_eq!(existing_dataset.source_table_id(), Some(original.id()));

        let reimport_request = registration_request("reimport-key", None);
        let outcome = reimport_as_new(&table_repo, &original, reimport_request).await.unwrap();
        let TableRegistrationOutcome::Registered { table_id: new_id } = outcome else { panic!("expected a genuinely new Table") };

        assert_ne!(new_id, original.id(), "reimport must always produce a new table_id");

        // The original Table is completely unaffected.
        let original_after = table_repo.get(original.id()).await.unwrap().unwrap();
        assert_eq!(original_after.row_count(), original.row_count());
        assert_eq!(original_after.table_data_ref().data_fingerprint, original.table_data_ref().data_fingerprint);
        assert_eq!(original_after.table_state(), TableState::Available);

        // The existing Project's Dataset reference still points at the
        // original table_id, never silently redirected to the new one.
        assert_eq!(existing_dataset.source_table_id(), Some(original.id()));
        assert_ne!(existing_dataset.source_table_id(), Some(new_id));
    }

    /// Part A: renaming a Table (`table_name` change) never touches
    /// `table_id`, fingerprints, `content_ref`, or `schema_ref`.
    #[tokio::test]
    async fn renaming_a_table_never_touches_identity_fingerprints_or_refs() {
        let table_repo = InMemoryTableRepository::new();
        let before = available_table(&table_repo, None).await;

        rename_table(&table_repo, before.id(), "renamed".to_string()).await.unwrap();
        let after = table_repo.get(before.id()).await.unwrap().unwrap();

        assert_eq!(after.table_name(), "renamed");
        assert_ne!(after.table_name(), before.table_name());

        assert_eq!(after.id(), before.id());
        assert_eq!(after.table_data_ref().data_fingerprint, before.table_data_ref().data_fingerprint);
        assert_eq!(after.table_data_ref().content_ref, before.table_data_ref().content_ref);
        assert_eq!(after.schema_ref().schema_fingerprint, before.schema_ref().schema_fingerprint);
        assert_eq!(after.table_state(), before.table_state());
    }

    /// INV-023: every canonical command listed in the task's command list
    /// exists and is reachable. Compile-time proof (this test simply
    /// wouldn't compile if any were missing or misnamed) for the commands
    /// whose signatures take no generic repository parameter; the
    /// generic ones (`search_tables`, `open_table_readonly`, etc. — now
    /// real, repository-backed delegations, not bare stubs) are instead
    /// proven to exist AND behave correctly by
    /// `commands::commands_delegation_tests`, which calls each one
    /// against a real repository double, plus the source-scan there
    /// confirming all 19 names are present in `commands.rs`.
    #[test]
    fn the_non_generic_canonical_commands_are_defined() {
        // `import_table`/`reimport_table_as_new` now take repository/
        // content-store parameters too (the real import pipeline task) —
        // they're exercised for real in `commands::table_import_flow_tests`
        // instead of referenced bare here.
        let _route_table_inspection = crate::commands::route_table_inspection;
        let _route_table_validation = crate::commands::route_table_validation;
        let _route_table_analysis = crate::commands::route_table_analysis;
        let _copy_table_id = crate::commands::copy_table_id;
    }
}
