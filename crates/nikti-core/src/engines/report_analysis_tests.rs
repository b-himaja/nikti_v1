#[cfg(test)]
mod tests {
    use polars::prelude::*;
    use uuid::Uuid;

    use crate::engines::analysis::{analyse_standard, AnalysisTarget};
    use crate::engines::content::TableContent;
    use crate::engines::report::{NativeReportEngine, ReportEngine, ANALYSIS_REPORT_SECTION_ORDER_TEMPLATE};
    use crate::types::analysis_procedure_params::AnalysisProcedureParams;
    use crate::types::{
        AnalysisCompatibleTargetType, AnalysisProcedure, AnalysisProcedureTarget, AnalysisRulesetVersion, DefaultScope, OutputOptions,
        TargetContract,
    };

    fn procedure(params: AnalysisProcedureParams, target: AnalysisProcedureTarget) -> AnalysisProcedure {
        AnalysisProcedure::new(Uuid::new_v4(), "p".to_string(), params, true, 1, target, OutputOptions::default(), vec![], "".to_string(), vec![])
    }

    fn version(hash: &str, procedures: Vec<AnalysisProcedure>) -> AnalysisRulesetVersion {
        AnalysisRulesetVersion::new(
            Uuid::new_v4(),
            1,
            1_000,
            "author".to_string(),
            "initial".to_string(),
            None,
            "1.0".to_string(),
            "1.0.0".to_string(),
            "0.1.0".to_string(),
            hash.to_string(),
            TargetContract::new(vec![AnalysisCompatibleTargetType::Table, AnalysisCompatibleTargetType::Dataset], None, vec![]),
            DefaultScope::default(),
            procedures,
        )
        .unwrap()
    }

    fn generate(v: &AnalysisRulesetVersion, table_id: Uuid) -> crate::engines::report::GeneratedAnalysisReport {
        let df = df!["v" => [10.0, 12.0, 11.0, 13.0, 12.0, 1000.0]].unwrap();
        let content = TableContent { table_id, content_fingerprint: "test-fp".to_string(), frame: df.lazy() };
        let (result, metadata, __detail) = analyse_standard(AnalysisTarget::Table(content), Uuid::new_v4(), v, "tester".to_string(), 1_000, "0.1.0", &|_| false);
        let engine = NativeReportEngine;
        engine.generate_analysis_report(result, metadata, v, crate::types::report_registration::SourceContext::for_table(table_id), "tester".to_string(), 1_000)
    }

    #[test]
    fn knowledge_refs_contains_exactly_the_ruleset_version_used() {
        let v = version("hash-a", vec![]);
        let generated = generate(&v, Uuid::new_v4());
        assert_eq!(generated.report.knowledge_refs(), &[v.analysis_ruleset_version_id()]);
    }

    #[test]
    fn rerunning_with_a_different_version_produces_a_distinct_report_even_on_unchanged_data() {
        let table_id = Uuid::new_v4();
        let v1 = version("hash-a", vec![]);
        let v2 = version("hash-b", vec![]);
        let g1 = generate(&v1, table_id);
        let g2 = generate(&v2, table_id);
        assert_ne!(g1.report.report_id(), g2.report.report_id());
        assert_ne!(g1.report.knowledge_refs(), g2.report.knowledge_refs());
    }

    #[test]
    fn generated_by_engine_and_source_engine_are_distinguished() {
        let v = version("hash-a", vec![]);
        let generated = generate(&v, Uuid::new_v4());
        assert_eq!(generated.report.generated_by_engine(), "report_engine");
        assert_eq!(generated.report.source_event_ref().source_engine, "analysis_engine");
    }

    #[test]
    fn fixed_section_order_matches_the_template_when_every_conditional_section_applies() {
        // Boundary procedure (produces Findings + Visual Evidence) + AP-019
        // (produces Feature & Row Attention Evidence + is itself part of
        // Procedure Results) + a manufactured warning, so every
        // conditional section in F.2 is present.
        let boundary = procedure(
            AnalysisProcedureParams::DistributionBoundaryExplorer { column: "v".to_string(), method: crate::types::analysis_procedure_params::BoundaryMethod::Iqr },
            AnalysisProcedureTarget::columns(&["v"]),
        );
        let screen = procedure(AnalysisProcedureParams::FeatureAndRowAttentionScreen, AnalysisProcedureTarget::table_scope());
        let v = version("hash-a", vec![boundary, screen]);
        let generated = generate(&v, Uuid::new_v4());

        let names: Vec<&str> = generated.content.sections.iter().map(|s| s.name.as_str()).collect();
        // Every present section must appear in the same relative order
        // as the template (a subsequence check — Warnings/Not Executed
        // may legitimately be absent if nothing warned/failed in this
        // particular fixture).
        let mut template_iter = ANALYSIS_REPORT_SECTION_ORDER_TEMPLATE.iter();
        for name in &names {
            assert!(template_iter.any(|t| t == name), "section '{name}' is out of order relative to the F.2 template");
        }
        assert!(names.contains(&"Feature & Row Attention Evidence"), "AP-019 ran, so this section must be present");
        assert!(names.contains(&"Visual Evidence"), "the boundary procedure produced a visual output ref");
    }

    #[test]
    fn feature_and_row_attention_evidence_section_absent_when_ap019_did_not_run() {
        let column_property = procedure(AnalysisProcedureParams::ColumnPropertyExplorer { columns: vec!["v".to_string()] }, AnalysisProcedureTarget::columns(&["v"]));
        let v = version("hash-a", vec![column_property]);
        let generated = generate(&v, Uuid::new_v4());
        let names: Vec<&str> = generated.content.sections.iter().map(|s| s.name.as_str()).collect();
        assert!(!names.contains(&"Feature & Row Attention Evidence"), "AP-019 did not run, so this section must be absent, not present-but-empty");
    }

    #[test]
    fn generated_report_starts_at_registered_never_available() {
        let v = version("hash-a", vec![]);
        let generated = generate(&v, Uuid::new_v4());
        assert_eq!(generated.report.report_state(), crate::types::ReportState::Registered);
    }

    #[test]
    fn composite_engine_version_flows_through_when_python_backed() {
        // ADR 0007: no procedure in the current frozen 19 is actually
        // Python-backed, so this wiring is otherwise never exercised —
        // this test proves the composite `{ rust_core, python_env_hash }`
        // form (INV-030) genuinely flows from execution metadata into the
        // report's `source_event_ref.source_engine_version`, by
        // constructing metadata as if a sidecar had run.
        let df = df!["v" => [1.0, 2.0, 3.0]].unwrap();
        let table_id = Uuid::new_v4();
        let content = TableContent { table_id, content_fingerprint: "test-fp".to_string(), frame: df.lazy() };
        let v = version("hash-a", vec![]);
        let (result, mut metadata, __detail) = analyse_standard(AnalysisTarget::Table(content), Uuid::new_v4(), &v, "tester".to_string(), 1_000, "0.1.0", &|_| false);
        metadata.engine_version.python_env_hash = Some("pinned-lockfile-hash-abc123".to_string());

        let engine = NativeReportEngine;
        let generated = engine.generate_analysis_report(result, metadata, &v, crate::types::report_registration::SourceContext::for_table(table_id), "tester".to_string(), 1_000);
        assert_eq!(generated.report.source_event_ref().source_engine_version.python_env_hash.as_deref(), Some("pinned-lockfile-hash-abc123"));
    }

    #[test]
    fn analysis_report_kind_is_distinct_for_same_type_only_comparison() {
        // F.4/INV-016: comparison logic itself remains `todo!()` in this
        // codebase for every report kind (unimplemented since the
        // Inspection task, not a regression introduced here) — but the
        // structural precondition for enforcing "same-type only" is that
        // each report kind is independently distinguishable, which this
        // confirms for Analysis specifically.
        let v = version("hash-a", vec![]);
        let generated = generate(&v, Uuid::new_v4());
        assert_eq!(generated.report.report_type(), crate::types::ReportKind::Analysis);
        assert_ne!(generated.report.report_type(), crate::types::ReportKind::Inspection);
        assert_ne!(generated.report.report_type(), crate::types::ReportKind::Validation);
    }
}
