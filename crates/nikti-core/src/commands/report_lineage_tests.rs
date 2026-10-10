//! `commands::generate_lineage_report` — real end-to-end chains, never
//! hand-constructed fixtures standing in for the real traversal (per
//! this task's own "Tests required"). Every assertion here checks a
//! specific node/edge the traversal is supposed to have found from real
//! command-layer state, not just "the call succeeded."

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use uuid::Uuid;

    use crate::commands::{self, project_workspace, ImportSource, LineageQueryObject};
    use crate::repositories::dataset_repository::testing::InMemoryDatasetRepository;
    use crate::repositories::project_repository::testing::InMemoryProjectRepository;
    use crate::repositories::report_repository::testing::InMemoryReportRepository;
    use crate::repositories::table_repository::testing::InMemoryTableRepository;
    use crate::repositories::{AnyReport, ParquetContentStore, ReportRepository};
    use crate::types::{LineageEdge, LineageNode, RegistrationOutcome, TableState, TransformationOperation};

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("nikti-lineage-test-{}", Uuid::new_v4()));
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
        content_store: ParquetContentStore,
    }

    impl Harness {
        fn new() -> Self {
            let dir = TempDir::new();
            let content_store = ParquetContentStore::new(dir.path().join("content")).unwrap();
            Self {
                _dir: dir,
                table_repo: InMemoryTableRepository::new(),
                dataset_repo: InMemoryDatasetRepository::new(),
                project_repo: InMemoryProjectRepository::new(),
                report_repo: InMemoryReportRepository::new(),
                content_store,
            }
        }
    }

    fn write_csv(dir: &std::path::Path, name: &str, content: &str) -> String {
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        path.display().to_string()
    }

    fn report_id_of(report: &AnyReport) -> Uuid {
        match report {
            AnyReport::Inspection(r) => r.report_id(),
            AnyReport::Validation(r) => r.report_id(),
            AnyReport::Analysis(r) => r.report_id(),
            AnyReport::Lineage(r) => r.report_id(),
            AnyReport::Comparison(r) => r.report_id(),
            // Real as of the Audit Report task.
            AnyReport::Audit(r) => r.report_id(),
        }
    }

    async fn extract_content(h: &Harness, outcome: RegistrationOutcome) -> crate::types::LineageContent {
        let RegistrationOutcome::Registered { report_id } = outcome else { panic!("expected Registered, got {outcome:?}") };
        let report = h.report_repo.get_lineage_report(report_id).await.unwrap().unwrap();
        let bytes = h.report_repo.get_content_bytes(report.content_ref()).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// Real chain: import → load as Dataset → transform → export as
    /// Table. `generate_lineage_report` queried from the EXPORTED Table
    /// must walk all the way back through the real chain.
    #[tokio::test]
    async fn real_end_to_end_chain_produces_the_true_nodes_and_edges() {
        let h = Harness::new();
        let scratch = TempDir::new();
        let uri = write_csv(scratch.path(), "orders.csv", "id,amount\n1,10.5\n2,20.0\n");
        let table1 = commands::import_table(&h.table_repo, &h.report_repo, &h.content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();

        let project = project_workspace::create_project(&h.project_repo, "P".to_string(), 1_000).await.unwrap();
        let dataset = project_workspace::load_table_as_dataset(&h.table_repo, &h.dataset_repo, &h.project_repo, project.id(), table1.id(), "orders".to_string(), 1_500).await.unwrap();

        project_workspace::run_transformation(&h.dataset_repo, &h.project_repo, &h.report_repo, &h.content_store, dataset.id(), vec![TransformationOperation::DropColumns { columns: vec!["amount".to_string()] }], 2_000, "tester".to_string())
            .await
            .unwrap();

        let table2 = project_workspace::export_dataset_as_table(&h.dataset_repo, &h.project_repo, &h.table_repo, &h.report_repo, &h.content_store, dataset.id(), "orders_export".to_string(), 3_000, "tester".to_string())
            .await
            .unwrap();

        let outcome = commands::generate_lineage_report(&h.table_repo, &h.dataset_repo, &h.project_repo, &h.report_repo, LineageQueryObject::Table(table2.id()), 4_000, "tester".to_string())
            .await
            .unwrap();
        let content = extract_content(&h, outcome).await;

        assert!(content.nodes.contains(&LineageNode::ImportedTable(table2.id())));
        assert!(content.nodes.contains(&LineageNode::Dataset(dataset.id())));
        assert!(content.nodes.contains(&LineageNode::ImportedTable(table1.id())));

        assert!(content.edges.contains(&LineageEdge::ExportAsTable { dataset_id: dataset.id(), table_id: table2.id() }), "real export_dataset_as_table back-reference");
        assert!(content.edges.contains(&LineageEdge::LoadIntoProject { table_id: table1.id(), dataset_id: dataset.id() }));
        assert!(
            content.edges.iter().any(|e| matches!(e, LineageEdge::TransformationRun { dataset_id, operation, .. } if *dataset_id == dataset.id() && operation.starts_with("DropColumns"))),
            "the real transformation step, not a fabricated one: {:?}",
            content.edges
        );

        // Every import auto-generates an Inspection report — real
        // generated_report edges for both Tables.
        assert!(content.edges.iter().any(|e| matches!(e, LineageEdge::GeneratedReport { source, .. } if *source == LineageNode::ImportedTable(table1.id()))));
        assert!(content.edges.iter().any(|e| matches!(e, LineageEdge::GeneratedReport { source, .. } if *source == LineageNode::ImportedTable(table2.id()))));

        assert!(content.upstream_count >= 3, "real edges, not an estimate: {}", content.upstream_count);
        assert_eq!(content.downstream_count, None, "forward traversal is explicitly out of scope — never a fabricated 0");
        assert!(content.archived_or_purged.is_empty());
        assert!(content.reproducible, "the whole chain is intact and loadable");
    }

    /// Part C: an archived node stops traversal past it, and
    /// `reproducible` becomes false — never silently continues past a
    /// node whose content may not be trustworthy.
    #[tokio::test]
    async fn an_archived_table_in_the_chain_stops_traversal_and_marks_the_graph_non_reproducible() {
        let h = Harness::new();
        let scratch = TempDir::new();
        let uri = write_csv(scratch.path(), "orders.csv", "id,amount\n1,10.5\n");
        let table1 = commands::import_table(&h.table_repo, &h.report_repo, &h.content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();
        let project = project_workspace::create_project(&h.project_repo, "P".to_string(), 1_000).await.unwrap();
        let dataset1 = project_workspace::load_table_as_dataset(&h.table_repo, &h.dataset_repo, &h.project_repo, project.id(), table1.id(), "orders".to_string(), 1_500).await.unwrap();
        let table2 = project_workspace::export_dataset_as_table(&h.dataset_repo, &h.project_repo, &h.table_repo, &h.report_repo, &h.content_store, dataset1.id(), "exported".to_string(), 2_000, "tester".to_string())
            .await
            .unwrap();

        // Archive the INTERMEDIATE table (table2) — traversal from a
        // Dataset loaded FROM table2 must reach table2, record it, and
        // stop before exploring table2's own backward edge to dataset1.
        commands::archive_table(&h.table_repo, table2.id(), 2_500).await.unwrap();
        let dataset2 = project_workspace::load_table_as_dataset(&h.table_repo, &h.dataset_repo, &h.project_repo, project.id(), table2.id(), "reloaded".to_string(), 3_000).await.unwrap();

        let outcome = commands::generate_lineage_report(&h.table_repo, &h.dataset_repo, &h.project_repo, &h.report_repo, LineageQueryObject::Dataset(dataset2.id()), 4_000, "tester".to_string())
            .await
            .unwrap();
        let content = extract_content(&h, outcome).await;

        assert_eq!(content.archived_or_purged, vec![LineageNode::ImportedTable(table2.id())]);
        assert!(!content.reproducible);
        assert!(content.nodes.contains(&LineageNode::ImportedTable(table2.id())));
        // Stopped BEFORE reaching further back — dataset1/table1 never
        // explored, since table2's own backward edge was never followed.
        assert!(!content.nodes.contains(&LineageNode::Dataset(dataset1.id())), "traversal must not continue past an archived node");
        assert!(!content.nodes.contains(&LineageNode::ImportedTable(table1.id())));
    }

    /// A purged Table (distinct from Archived) triggers the same stop.
    #[tokio::test]
    async fn a_purged_table_also_stops_traversal_and_marks_the_graph_non_reproducible() {
        let h = Harness::new();
        let scratch = TempDir::new();
        let uri = write_csv(scratch.path(), "orders.csv", "id,amount\n1,10.5\n");
        let table = commands::import_table(&h.table_repo, &h.report_repo, &h.content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();
        commands::purge_table(&h.table_repo, table.id(), "retention policy".to_string(), 2_000).await.unwrap();

        let outcome = commands::generate_lineage_report(&h.table_repo, &h.dataset_repo, &h.project_repo, &h.report_repo, LineageQueryObject::Table(table.id()), 3_000, "tester".to_string())
            .await
            .unwrap();
        let content = extract_content(&h, outcome).await;

        assert_eq!(content.archived_or_purged, vec![LineageNode::ImportedTable(table.id())]);
        assert!(!content.reproducible);
    }

    /// The specific historical gap flagged since early in this project:
    /// is `export_dataset_as_table`'s back-reference to its source
    /// Dataset actually populated? Proven here directly against the
    /// real Table object, not asserted only via the lineage graph.
    #[tokio::test]
    async fn export_dataset_as_table_really_does_populate_the_source_dataset_back_reference() {
        let h = Harness::new();
        let scratch = TempDir::new();
        let uri = write_csv(scratch.path(), "orders.csv", "id,amount\n1,10.5\n");
        let table1 = commands::import_table(&h.table_repo, &h.report_repo, &h.content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();
        let project = project_workspace::create_project(&h.project_repo, "P".to_string(), 1_000).await.unwrap();
        let dataset = project_workspace::load_table_as_dataset(&h.table_repo, &h.dataset_repo, &h.project_repo, project.id(), table1.id(), "orders".to_string(), 1_500).await.unwrap();
        let exported = project_workspace::export_dataset_as_table(&h.dataset_repo, &h.project_repo, &h.table_repo, &h.report_repo, &h.content_store, dataset.id(), "exported".to_string(), 2_000, "tester".to_string())
            .await
            .unwrap();

        assert_eq!(exported.source_provenance().source_dataset_id, Some(dataset.id()), "the field this task's Part B asked to check first — real and populated, not a continuing gap");

        // The lineage graph reflects it as a real edge, not a fabricated
        // one on top of an empty field.
        let outcome = commands::generate_lineage_report(&h.table_repo, &h.dataset_repo, &h.project_repo, &h.report_repo, LineageQueryObject::Table(exported.id()), 3_000, "tester".to_string())
            .await
            .unwrap();
        let content = extract_content(&h, outcome).await;
        assert!(content.edges.contains(&LineageEdge::ExportAsTable { dataset_id: dataset.id(), table_id: exported.id() }));
    }

    /// Part D: idempotent registration — regenerating lineage for
    /// unchanged underlying data returns the same `report_id`, never a
    /// duplicate.
    #[tokio::test]
    async fn generating_lineage_twice_for_unchanged_data_is_idempotent() {
        let h = Harness::new();
        let scratch = TempDir::new();
        let uri = write_csv(scratch.path(), "orders.csv", "id,amount\n1,10.5\n");
        let table = commands::import_table(&h.table_repo, &h.report_repo, &h.content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();

        let first = commands::generate_lineage_report(&h.table_repo, &h.dataset_repo, &h.project_repo, &h.report_repo, LineageQueryObject::Table(table.id()), 2_000, "tester".to_string())
            .await
            .unwrap();
        let RegistrationOutcome::Registered { report_id: first_id } = first else { panic!("expected Registered") };

        let second = commands::generate_lineage_report(&h.table_repo, &h.dataset_repo, &h.project_repo, &h.report_repo, LineageQueryObject::Table(table.id()), 5_000, "tester".to_string())
            .await
            .unwrap();
        assert_eq!(second, RegistrationOutcome::Idempotent { report_id: first_id }, "unchanged graph content must hit the idempotent path, never duplicate");
    }

    /// Audit #3 found Lineage had no kind-specific test proving what
    /// happens when its OWN report registration fails content
    /// validation — every other kind (Inspection/Validation/Analysis/
    /// Audit) already has one; this was inherited-by-assumption only.
    /// Mirrors `a_degraded_audit_report_registration_never_undoes_the_
    /// already_committed_transformation`'s exact mechanism
    /// (`InMemoryReportRepository::with_content_validation_failure()`).
    /// Note what this test deliberately does NOT do: it does not use a
    /// nonexistent root Table/Dataset id to trigger this. Read directly,
    /// `build_lineage_content` treats an unresolvable root/ancestor as
    /// `reproducible: false` honest content (the same pattern already
    /// covered by `an_archived_table_in_the_chain_stops_traversal_...`
    /// below) — the traversal still completes and the report still
    /// registers successfully. That's correct, existing behavior, not a
    /// gap: a missing reference is informative content, not a
    /// registration failure. The only real path to `Unavailable` is the
    /// registration layer itself, which is what this test exercises.
    #[tokio::test]
    async fn a_degraded_lineage_report_registration_is_surfaced_as_unavailable_never_silently_available() {
        let dir = TempDir::new();
        let content_store = ParquetContentStore::new(dir.path().join("content")).unwrap();
        let table_repo = InMemoryTableRepository::new();
        let report_repo = InMemoryReportRepository::with_content_validation_failure();
        let dataset_repo = InMemoryDatasetRepository::new();
        let project_repo = InMemoryProjectRepository::new();

        let uri = write_csv(dir.path(), "orders.csv", "id,amount\n1,10.5\n2,20.0\n");
        let table = commands::import_table(&table_repo, &report_repo, &content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();

        let outcome = commands::generate_lineage_report(&table_repo, &dataset_repo, &project_repo, &report_repo, LineageQueryObject::Table(table.id()), 2_000, "tester".to_string())
            .await
            .unwrap();
        let RegistrationOutcome::RegisteredButUnavailable { reason, .. } = outcome else { panic!("expected RegisteredButUnavailable, got {outcome:?}") };
        assert!(!reason.is_empty(), "a real reason, not a silent degradation");

        // Confirmed at the metadata level, not via `extract_content` —
        // content is genuinely never written to the content store on
        // this path (registration's own content-write step never runs).
        let reports = report_repo.list_by_kind(crate::types::common::ReportKind::Lineage).await.unwrap();
        assert_eq!(reports.len(), 1);
        let AnyReport::Lineage(lineage) = &reports[0] else { panic!("expected Lineage") };
        assert_eq!(lineage.report_state(), crate::types::report_registration::ReportState::Unavailable, "content validation failure must surface as degraded, never as success");
    }

    /// A real, different graph (a genuine transformation happened
    /// between calls) must NOT be treated as idempotent with the first.
    #[tokio::test]
    async fn generating_lineage_after_a_real_change_produces_a_different_report() {
        let h = Harness::new();
        let scratch = TempDir::new();
        let uri = write_csv(scratch.path(), "orders.csv", "id,amount\n1,10.5\n2,20.0\n");
        let table1 = commands::import_table(&h.table_repo, &h.report_repo, &h.content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();
        let project = project_workspace::create_project(&h.project_repo, "P".to_string(), 1_000).await.unwrap();
        let dataset = project_workspace::load_table_as_dataset(&h.table_repo, &h.dataset_repo, &h.project_repo, project.id(), table1.id(), "orders".to_string(), 1_500).await.unwrap();

        let first = commands::generate_lineage_report(&h.table_repo, &h.dataset_repo, &h.project_repo, &h.report_repo, LineageQueryObject::Dataset(dataset.id()), 2_000, "tester".to_string())
            .await
            .unwrap();
        let RegistrationOutcome::Registered { report_id: first_id } = first else { panic!("expected Registered") };

        // A real transformation changes the graph (a new TransformationRun edge).
        project_workspace::run_transformation(&h.dataset_repo, &h.project_repo, &h.report_repo, &h.content_store, dataset.id(), vec![TransformationOperation::DropColumns { columns: vec!["amount".to_string()] }], 3_000, "tester".to_string())
            .await
            .unwrap();

        let second = commands::generate_lineage_report(&h.table_repo, &h.dataset_repo, &h.project_repo, &h.report_repo, LineageQueryObject::Dataset(dataset.id()), 4_000, "tester".to_string())
            .await
            .unwrap();
        let RegistrationOutcome::Registered { report_id: second_id } = second else { panic!("expected a fresh Registered outcome for genuinely different content") };
        assert_ne!(first_id, second_id);
    }

    /// `LineageEdge::RecipeApplied` is kept in the type (matching the
    /// requested edge vocabulary) but is structurally unreachable from
    /// the real traversal: `apply_recipe_to_dataset` forwards straight
    /// into `run_transformation`, and no `recipe_id` is ever recorded on
    /// the resulting log entry — so applying a Recipe produces the exact
    /// same trace as a manually-run transformation, never a
    /// `RecipeApplied` edge.
    #[tokio::test]
    async fn applying_a_recipe_never_produces_a_recipe_applied_edge_only_transformation_run() {
        let h = Harness::new();
        let scratch = TempDir::new();
        let uri = write_csv(scratch.path(), "orders.csv", "id,amount\n1,10.5\n2,20.0\n");
        let table1 = commands::import_table(&h.table_repo, &h.report_repo, &h.content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();
        let project = project_workspace::create_project(&h.project_repo, "P".to_string(), 1_000).await.unwrap();
        let dataset = project_workspace::load_table_as_dataset(&h.table_repo, &h.dataset_repo, &h.project_repo, project.id(), table1.id(), "orders".to_string(), 1_500).await.unwrap();

        let context = project_workspace::RecipeContext { recipe_id: Uuid::new_v4(), version: 1, steps: vec![TransformationOperation::DropColumns { columns: vec!["amount".to_string()] }] };
        project_workspace::apply_recipe_to_dataset(&h.dataset_repo, &h.project_repo, &h.report_repo, &h.content_store, dataset.id(), context, 2_000, "tester".to_string()).await.unwrap();

        let outcome = commands::generate_lineage_report(&h.table_repo, &h.dataset_repo, &h.project_repo, &h.report_repo, LineageQueryObject::Dataset(dataset.id()), 3_000, "tester".to_string())
            .await
            .unwrap();
        let content = extract_content(&h, outcome).await;

        assert!(!content.edges.iter().any(|e| matches!(e, LineageEdge::RecipeApplied { .. })), "no data exists to honestly construct this edge — must never be fabricated");
        assert!(content.edges.iter().any(|e| matches!(e, LineageEdge::TransformationRun { dataset_id, .. } if *dataset_id == dataset.id())), "the real, honest classification of what happened");
    }

    /// INV-015: `LineageReport::new` is constructed in exactly the two
    /// places every OTHER report kind's own object is also legitimately
    /// constructed in — `engines/report.rs` (the Report Engine, which
    /// builds the pre-registration draft from already-queried content)
    /// and `repositories/report_repository.rs` (the registration
    /// algorithm, which alone re-constructs the final, state-transitioned
    /// object — `Registered`/`Available`/`Unavailable` — per B.3; see
    /// `register_inspection_report`'s identical shape). No workspace or
    /// command assembles a `LineageReport` independently of that
    /// established two-step pattern.
    #[test]
    fn lineage_report_is_only_ever_constructed_by_the_report_engine_and_its_registration_algorithm() {
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
                if name.ends_with("_tests.rs") || name == "report_lineage.rs" {
                    // The type's own constructor definition, and this
                    // test file itself, are not call sites.
                    continue;
                }
                let source = std::fs::read_to_string(&path).unwrap();
                if source.contains("LineageReport::new(") {
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
        assert_eq!(hits, expected, "exactly these two legitimate call sites (Report Engine + its registration algorithm) — got {hits:?}");
    }
}
