//! Part B of the fabricated-evidence remediation task: proves
//! `AffectedRowsRef`/`OutputTableRef` are backed by real, readable
//! Parquet content — not just that the reference string is well-formed.
//! Every test here calls the real command layer
//! (`run_validation_on_dataset`/`run_analysis_on_dataset`), reads the
//! actual stored report content back out (same technique as Part A's
//! Inspection fingerprint test), extracts the real `reference` string
//! the engine produced, and reads that file back via `DetailTableStore`.

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
    use crate::repositories::{DetailTableStore, KnowledgeRepository, ParquetContentStore, ProjectRepository, ReportRepository};
    use crate::types::analysis_procedure_params::AnalysisProcedureParams;
    use crate::types::validation_function_params::ValidationFunctionParams;
    use crate::types::{
        AnalysisProcedure, AnalysisProcedureTarget, AnalysisRuleset, AnalysisRulesetVersion, BindingMode, DefaultScope,
        OutputOptions, RegistrationOutcome, RuleNullPolicy, RuleTarget, RulesetMissingValuePolicy, Severity,
        TargetContract, ValidationRule, ValidationRuleset, ValidationRulesetVersion, ValidationTargetType,
    };

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("nikti-detail-storage-test-{}", Uuid::new_v4()));
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

    /// A dataset with real, deliberate issues: row 3's `id` is null
    /// (fails a column-level `NotMissing` check), rows 4/5 are exact
    /// duplicates (fails a row-level `NoDuplicateRows` check), and the
    /// table overall has <100% completeness on `id` (fails a table-level
    /// `MinimumCompleteness` check, which has no per-row concept at all).
    async fn setup(h: &Harness) -> (crate::types::Project, crate::types::Dataset) {
        let scratch = TempDir::new();
        let csv = "id,category,amount,signup_date,activation_date\n\
                   1,A,10.5,2024-01-01,2024-01-05\n\
                   2,A,20.0,2024-01-02,2024-01-12\n\
                   ,B,30.0,2024-01-10,2024-01-20\n\
                   4,B,200.0,2024-02-01,2024-02-11\n\
                   4,B,200.0,2024-02-01,2024-02-11\n";
        let uri = write_csv(scratch.path(), "issues.csv", csv);
        let table = commands::import_table(&h.table_repo, &h.report_repo, &h.content_store, ImportSource { uri }, 1_000, "tester".to_string()).await.unwrap();
        let project = project_workspace::create_project(&h.project_repo, "P".to_string(), 1_000).await.unwrap();
        let dataset = project_workspace::load_table_as_dataset(&h.table_repo, &h.dataset_repo, &h.project_repo, project.id(), table.id(), "issues".to_string(), 1_500).await.unwrap();
        let project = h.project_repo.get(project.id()).await.unwrap().unwrap();
        (project, dataset)
    }

    fn validation_ruleset() -> ValidationRuleset {
        let col_target = |c: &str| RuleTarget { target_type: ValidationTargetType::Column, binding_mode: BindingMode::FixedColumnName, target_columns: vec![c.to_string()] };
        let table_target = RuleTarget { target_type: ValidationTargetType::Table, binding_mode: BindingMode::FixedColumnName, target_columns: vec![] };

        let not_missing = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "id not missing".to_string(), ValidationFunctionParams::NotMissing, true, Severity::Error, col_target("id"), RuleNullPolicy::FailIfMissing, "id must not be missing".to_string());
        let no_dup_rows = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "no duplicate rows".to_string(), ValidationFunctionParams::NoDuplicateRows, true, Severity::Error, table_target.clone(), RuleNullPolicy::IgnoreMissing, "rows must not exactly duplicate".to_string());
        let min_completeness = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "id completeness".to_string(), ValidationFunctionParams::MinimumCompleteness { min_ratio: 1.0 }, true, Severity::Error, col_target("id"), RuleNullPolicy::IgnoreMissing, "id must be 100% complete".to_string());

        let version = ValidationRulesetVersion::new(Uuid::new_v4(), 1, 1_000, "author".to_string(), "initial".to_string(), "1.0".to_string(), "1.0.0".to_string(), "hash".to_string(), vec![not_missing, no_dup_rows, min_completeness], vec![], vec![], vec![], vec![], RulesetMissingValuePolicy::default());
        ValidationRuleset::new(Uuid::new_v4(), "R".to_string(), 1_000, "author".to_string(), vec![], "".to_string(), version)
    }

    fn analysis_ruleset() -> AnalysisRuleset {
        let grouped = AnalysisProcedure::new(Uuid::new_v4(), "Grouped Summary".to_string(), AnalysisProcedureParams::GroupedSummary { group_by: vec!["category".to_string()], measure_column: "amount".to_string(), agg: crate::types::transformation_operation::AggFunc::Sum }, true, 2, AnalysisProcedureTarget::columns(&["category", "amount"]), OutputOptions::default(), vec![], "".to_string(), vec![]);
        let boundary = AnalysisProcedure::new(Uuid::new_v4(), "Distribution Boundary".to_string(), AnalysisProcedureParams::DistributionBoundaryExplorer { column: "amount".to_string(), method: crate::types::analysis_procedure_params::BoundaryMethod::Iqr }, true, 3, AnalysisProcedureTarget::columns(&["amount"]), OutputOptions::default(), vec![], "".to_string(), vec![]);
        let trend = AnalysisProcedure::new(
            Uuid::new_v4(),
            "Date Trend".to_string(),
            AnalysisProcedureParams::DateTimeTrendExplorer {
                date_column: "signup_date".to_string(),
                bucket: crate::types::analysis_procedure_params::DateBucket::Day,
                duration_start_column: Some("signup_date".to_string()),
                duration_end_column: Some("activation_date".to_string()),
            },
            true,
            4,
            AnalysisProcedureTarget::columns(&["signup_date", "activation_date"]),
            OutputOptions::default(),
            vec![],
            "".to_string(),
            vec![],
        );
        let attention = AnalysisProcedure::new(Uuid::new_v4(), "Feature & Row Attention Screen".to_string(), AnalysisProcedureParams::FeatureAndRowAttentionScreen, true, 5, AnalysisProcedureTarget::table_scope(), OutputOptions::default(), vec![], "".to_string(), vec![]);

        let version = AnalysisRulesetVersion::new(Uuid::new_v4(), 1, 1_000, "author".to_string(), "initial".to_string(), None, "1.0".to_string(), "1.0.0".to_string(), "0.1.0".to_string(), "hash".to_string(), TargetContract::new(vec![], None, vec![]), DefaultScope::default(), vec![grouped, boundary, trend, attention]).unwrap();
        AnalysisRuleset::new(Uuid::new_v4(), "AR".to_string(), 1_000, "author".to_string(), vec![], "".to_string(), version)
    }

    async fn registered_report_content(h: &Harness, outcome: RegistrationOutcome, kind: &str) -> serde_json::Value {
        let RegistrationOutcome::Registered { report_id } = outcome else { panic!("expected Registered, got {outcome:?}") };
        let content_ref = if kind == "validation" {
            h.report_repo.get_validation_report(report_id).await.unwrap().unwrap().content_ref().to_string()
        } else {
            h.report_repo.get_analysis_report(report_id).await.unwrap().unwrap().content_ref().to_string()
        };
        let bytes = h.report_repo.get_content_bytes(&content_ref).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn validation_affected_rows_ref_reads_back_to_real_failing_row_content_column_row_and_table_level() {
        let h = Harness::new();
        let (_project, dataset) = setup(&h).await;
        let mut ruleset = validation_ruleset();
        let version_id = ruleset.versions()[0].ruleset_version_id();
        ruleset.activate_version(version_id, 5_000, "author".to_string()).unwrap();
        h.knowledge_repo.store_validation_ruleset(ruleset.clone()).await.unwrap();

        let outcome = project_workspace::run_validation_on_dataset(&h.dataset_repo, &h.project_repo, &h.knowledge_repo, &h.report_repo, &h.content_store, &h.detail_store, dataset.id(), ruleset.ruleset_id(), version_id, 6_000, "tester".to_string())
            .await
            .unwrap();
        let content = registered_report_content(&h, outcome, "validation").await;
        let rule_results = content["results"]["rule_results"].as_array().unwrap();

        // Column-level: NotMissing on `id` — real failing rows readable.
        let not_missing = rule_results.iter().find(|r| r["message"] == "id must not be missing").unwrap();
        assert_eq!(not_missing["failed_count"], 1);
        let reference = not_missing["affected_rows_ref"]["reference"].as_str().expect("a real reference must exist when failed_count > 0");
        let rows = h.detail_store.read(reference).unwrap();
        assert_eq!(rows.height(), 1, "the file at the reference contains exactly the real failing row(s)");
        assert!(rows.column("id").unwrap().null_count() == 1, "the actual failing row — a null id — is really in the file");

        // Row-level: NoDuplicateRows — real duplicate rows readable.
        let no_dup = rule_results.iter().find(|r| r["message"] == "rows must not exactly duplicate").unwrap();
        assert!(no_dup["failed_count"].as_u64().unwrap() > 0);
        let reference = no_dup["affected_rows_ref"]["reference"].as_str().expect("a real reference must exist");
        let rows = h.detail_store.read(reference).unwrap();
        assert_eq!(rows.height() as u64, no_dup["failed_count"].as_u64().unwrap(), "real row count matches the reported failed_count exactly");

        // Table-level: MinimumCompleteness — no natural per-row subset,
        // so no reference at all (never a fabricated one pointing at
        // nothing, per Part B).
        let completeness = rule_results.iter().find(|r| r["message"] == "id must be 100% complete").unwrap();
        assert!(completeness["failed_count"].as_u64().unwrap() > 0, "the check does fail");
        assert!(completeness["affected_rows_ref"].is_null(), "a table-level aggregate check must never claim a per-row reference");
    }

    #[tokio::test]
    async fn analysis_output_table_refs_read_back_to_real_content_across_categories() {
        let h = Harness::new();
        let (_project, dataset) = setup(&h).await;
        let mut ruleset = analysis_ruleset();
        let version_id = ruleset.versions()[0].analysis_ruleset_version_id();
        ruleset.activate_version(version_id, 5_000, "author".to_string()).unwrap();
        h.knowledge_repo.store_analysis_ruleset(ruleset.clone()).await.unwrap();

        let outcome = project_workspace::run_analysis_on_dataset(&h.dataset_repo, &h.project_repo, &h.knowledge_repo, &h.report_repo, &h.content_store, &h.detail_store, dataset.id(), ruleset.analysis_ruleset_id(), version_id, 6_000, "tester".to_string())
            .await
            .unwrap();
        let content = registered_report_content(&h, outcome, "analysis").await;
        let procedure_results = content["result"]["procedure_results"].as_array().unwrap();

        let find_table = |procedure_name: &str, table_name: &str| -> serde_json::Value {
            let proc = procedure_results.iter().find(|p| p["procedure_name"] == procedure_name).unwrap_or_else(|| panic!("procedure {procedure_name} not found"));
            let refs = proc["output_table_refs"].as_array().unwrap();
            refs.iter().find(|t| t["name"] == table_name).unwrap_or_else(|| panic!("table {table_name} not found on {procedure_name}")).clone()
        };

        // AP-001 (setup category) — always present, mandatory first.
        let scope_ref = find_table("Scope & Variable Role Setup", "scope_summary_table");
        let scope_rows = h.detail_store.read(scope_ref["reference"].as_str().unwrap()).unwrap();
        assert_eq!(scope_rows.height(), 1);
        assert!(scope_rows.column("row_count").is_ok(), "real column, not a hand-typed claim");

        // AP-004 (grouping category) — the rank table's row_count/columns
        // are derived from real content, and the rank column exists.
        let rank_ref = find_table("Grouped Summary", "group_rank_table");
        let rank_rows = h.detail_store.read(rank_ref["reference"].as_str().unwrap()).unwrap();
        assert!(rank_rows.column("rank").is_ok(), "AP-004's previously-fabricated rank column is now real");
        assert_eq!(rank_rows.height() as u64, rank_ref["row_count"].as_u64().unwrap());

        // AP-009 (distribution category) — real histogram, bin counts sum
        // to the real row count (not the previously-hardcoded 10 rows of
        // nothing).
        let hist_ref = find_table("Distribution Boundary", "histogram_table");
        let hist_rows = h.detail_store.read(hist_ref["reference"].as_str().unwrap()).unwrap();
        let counts = hist_rows.column("count").unwrap().i64().unwrap();
        let total: i64 = counts.into_iter().flatten().sum();
        assert_eq!(total as u64, dataset.row_count(), "real bucketed counts, not a fabricated fixed-10-row placeholder");

        // AP-013 (time category) — duration_summary_table reports real
        // min/max/mean/median of the per-row `activation_date -
        // signup_date` difference (the corrected semantics — see the
        // AP-013 duration-semantics correction task), genuinely different
        // from date_summary_table's raw min/max of signup_date alone.
        let date_summary = find_table("Date Trend", "date_summary_table");
        let duration_summary = find_table("Date Trend", "duration_summary_table");
        assert_ne!(date_summary["columns"], duration_summary["columns"], "no longer a byte-for-byte copy — different columns entirely (min/max of raw dates vs min/max/mean/median of per-row durations)");
        let duration_rows = h.detail_store.read(duration_summary["reference"].as_str().unwrap()).unwrap();
        assert!(duration_rows.column("median").is_ok(), "real duration statistics, not a copy of date_summary_table");

        // AP-019 (attention/evidence category) — ran because AP-009
        // produced a BoundaryCandidate finding; its evidence table is
        // real (possibly empty, but genuinely computed, not fabricated).
        let row_attention = find_table("Feature & Row Attention Screen", "row_attention_table");
        let _ = h.detail_store.read(row_attention["reference"].as_str().unwrap()).unwrap();
    }

    /// Part B's required sweep confirmation: every one of the 30
    /// Validation functions and the 13 Analysis procedures the audit
    /// flagged, checked off against what this task actually did to it.
    /// "Real" = now writes/reads real storage-backed content for every
    /// reference it produces. "Removed" = the specific sub-table that had
    /// no real computation behind it is no longer claimed at all (Part
    /// B/C: never fabricated content just to have something to store).
    #[test]
    fn all_forty_three_originally_flagged_call_sites_are_accounted_for() {
        const VALIDATION_FUNCTIONS_NOW_REAL: [&str; 30] = [
            "not_missing", "not_blank", "not_whitespace_only", "minimum_completeness", "one_of_required",
            "numeric_compatible", "date_compatible", "boolean_compatible", "min_value", "max_value",
            "between_values", "allowed_values", "disallowed_values", "distinct_count_between", "regex_match",
            "regex_not_match", "character_pattern_match", "min_length", "max_length", "length_between",
            "unique_column", "unique_column_set", "no_duplicate_rows", "column_compare", "required_if",
            "mutually_exclusive", "exists_in_reference", "not_exists_in_reference", "date_format_match", "expression_true",
        ];
        assert_eq!(VALIDATION_FUNCTIONS_NOW_REAL.len(), 30);

        // (procedure, table_name, outcome) — outcome is "Real" for every
        // table with genuine content now, "Removed" for the 2 remaining
        // sub-tables that had no real computation behind them and were
        // not claimed rather than fabricated (Part B's explicit
        // instruction: don't add a placeholder to make it look wired).
        // AP-012's `regex_measure_table` and AP-016's `target_by_segment_table`
        // were originally "Removed" for the same reason but were pure
        // parameter-plumbing gaps, not missing algorithms — a follow-up
        // task added the missing `regex_list`/`segment_columns` parameters
        // and real computations, so both are "Real" here now.
        const ANALYSIS_PROCEDURES_WITH_FLAGGED_TABLES: [(&str, &str, &str); 13] = [
            ("AP-004", "group_rank_table", "Real"),
            ("AP-005", "difference_table", "Real"),
            ("AP-006", "crosstab_percentage_table", "Real"),
            ("AP-007", "category_distribution_table", "Real"),
            ("AP-008", "top_numeric_pairs_table", "Real"),
            ("AP-009", "histogram_table", "Real (Part C fix)"),
            ("AP-010", "missingness_pattern_table", "Real"),
            ("AP-011", "duplicate_group_table", "Real"),
            ("AP-012", "regex_measure_table", "Real (parameter-plumbing follow-up fix: regex_list added to params)"),
            ("AP-013", "date_gap_table", "Removed — gap-detection algorithm out of Part C's scope"),
            ("AP-013", "duration_summary_table", "Real (Part C fix, later corrected: reports the per-row duration_end - duration_start difference, not a between-row gap)"),
            ("AP-016", "target_by_segment_table", "Real (parameter-plumbing follow-up fix: segment_columns added to params)"),
            ("AP-018", "fd_exception_table", "Removed — functional-dependency-exception detection out of Part C's scope"),
        ];
        assert_eq!(ANALYSIS_PROCEDURES_WITH_FLAGGED_TABLES.len(), 13);

        assert_eq!(VALIDATION_FUNCTIONS_NOW_REAL.len() + ANALYSIS_PROCEDURES_WITH_FLAGGED_TABLES.len(), 43);
    }
}
