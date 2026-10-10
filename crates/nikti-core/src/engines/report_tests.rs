#[cfg(test)]
mod tests {
    use polars::prelude::IntoLazy;
    use uuid::Uuid;

    use crate::engines::inspection::{inspect_standard, ImportSummary, ThresholdRegistry};
    use crate::engines::content::TableContent;
    use crate::engines::report::{NativeReportEngine, ReportEngine};
    use crate::types::common::EngineVersion;

    fn results_for(values: &[&str]) -> crate::engines::inspection::InspectionResults {
        let df = polars::df!["x" => values].unwrap();
        let content = TableContent { table_id: Uuid::new_v4(), content_fingerprint: "test-fp".to_string(), frame: df.lazy() };
        inspect_standard(content, ImportSummary::default(), &ThresholdRegistry::v1(), "0.1.0", 1_000).unwrap()
    }

    #[test]
    fn generate_inspection_report_distinguishes_generated_by_engine_from_source_engine() {
        let engine = NativeReportEngine;
        let generated = engine.generate_inspection_report(
            results_for(&["a", "b"]),
            Uuid::new_v4(),
            Uuid::new_v4(),
            EngineVersion { rust_core: "0.1.0".to_string(), python_env_hash: None },
            "test-user".to_string(),
            1_000,
        );
        // B.1: `generated_by_engine` identifies who built the Report object
        // (always the Report Engine); `source_event_ref.source_engine`
        // identifies who produced the evidence (the Inspection Engine).
        // Never conflate the two.
        assert_eq!(generated.report.generated_by_engine(), "report_engine");
        assert_eq!(generated.report.source_event_ref().source_engine, "inspection_engine");
    }

    #[test]
    fn two_calls_on_different_content_produce_two_distinct_report_ids() {
        let engine = NativeReportEngine;
        let table_id = Uuid::new_v4();
        let a = engine.generate_inspection_report(
            results_for(&["a", "b"]),
            table_id,
            Uuid::new_v4(),
            EngineVersion { rust_core: "0.1.0".to_string(), python_env_hash: None },
            "test-user".to_string(),
            1_000,
        );
        let b = engine.generate_inspection_report(
            results_for(&["c", "d", "e"]),
            table_id,
            Uuid::new_v4(),
            EngineVersion { rust_core: "0.1.0".to_string(), python_env_hash: None },
            "test-user".to_string(),
            1_001,
        );
        assert_ne!(a.report.report_id(), b.report.report_id());
        assert_ne!(a.report.content_hash(), b.report.content_hash(), "different content must hash differently");
    }

    #[test]
    fn generated_report_starts_at_registered_never_available() {
        // The Report Engine never assigns `Available` itself (B.3) — only
        // the repository's registration algorithm does, after content and
        // index validation succeed.
        let engine = NativeReportEngine;
        let generated = engine.generate_inspection_report(
            results_for(&["a"]),
            Uuid::new_v4(),
            Uuid::new_v4(),
            EngineVersion { rust_core: "0.1.0".to_string(), python_env_hash: None },
            "test-user".to_string(),
            1_000,
        );
        assert_eq!(generated.report.report_state(), crate::types::ReportState::Registered);
    }
}
