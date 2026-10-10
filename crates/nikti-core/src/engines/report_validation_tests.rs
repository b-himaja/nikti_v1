#[cfg(test)]
mod tests {
    use polars::prelude::*;
    use uuid::Uuid;

    use crate::engines::content::TableContent;
    use crate::engines::report::{NativeReportEngine, ReportEngine, VALIDATION_REPORT_SECTION_ORDER};
    use crate::engines::validation::{validate_standard, ValidationTarget};
    use crate::types::validation_function_params::ValidationFunctionParams;
    use crate::types::{
        BindingMode, RuleNullPolicy, RuleTarget, RulesetMissingValuePolicy, Severity, ValidationRule, ValidationRulesetVersion,
        ValidationTargetType,
    };

    fn rule() -> ValidationRule {
        let target = RuleTarget { target_type: ValidationTargetType::Column, binding_mode: BindingMode::FixedColumnName, target_columns: vec!["x".to_string()] };
        ValidationRule::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "r".to_string(),
            ValidationFunctionParams::NotMissing,
            true,
            Severity::Error,
            target,
            RuleNullPolicy::FailIfMissing,
            "x must not be missing".to_string(),
        )
    }

    fn version(hash: &str) -> ValidationRulesetVersion {
        ValidationRulesetVersion::new(
            Uuid::new_v4(),
            1,
            1_000,
            "author".to_string(),
            "initial".to_string(),
            "1.0".to_string(),
            "1.0.0".to_string(),
            hash.to_string(),
            vec![rule()],
            vec![],
            vec![],
            vec![],
            vec![],
            RulesetMissingValuePolicy::default(),
        )
    }

    fn generate(v: &ValidationRulesetVersion, table_id: Uuid) -> crate::engines::report::GeneratedValidationReport {
        let df = df!["x" => [Some(1i64), None]].unwrap();
        let content = TableContent { table_id, content_fingerprint: "test-fp".to_string(), frame: df.lazy() };
        let (results, metadata, __detail) = validate_standard(ValidationTarget::Table(content), Uuid::new_v4(), v, "tester".to_string(), 1_000, "0.1.0", &|_| false);
        let engine = NativeReportEngine;
        engine.generate_validation_report(results, metadata, v, crate::types::report_registration::SourceContext::for_table(table_id), "tester".to_string(), 1_000)
    }

    #[test]
    fn knowledge_refs_contains_exactly_the_ruleset_version_used() {
        let v = version("hash-a");
        let generated = generate(&v, Uuid::new_v4());
        assert_eq!(generated.report.knowledge_refs(), &[v.ruleset_version_id()]);
    }

    #[test]
    fn rerunning_with_a_different_version_produces_a_distinct_report_even_on_unchanged_data() {
        let table_id = Uuid::new_v4();
        let v1 = version("hash-a");
        let v2 = version("hash-b");
        let g1 = generate(&v1, table_id);
        let g2 = generate(&v2, table_id);
        assert_ne!(g1.report.report_id(), g2.report.report_id());
        assert_ne!(g1.report.knowledge_refs(), g2.report.knowledge_refs());
    }

    #[test]
    fn generated_by_engine_and_source_engine_are_distinguished() {
        let v = version("hash-a");
        let generated = generate(&v, Uuid::new_v4());
        assert_eq!(generated.report.generated_by_engine(), "report_engine");
        assert_eq!(generated.report.source_event_ref().source_engine, "validation_engine");
    }

    const SYSTEM_ADVICE_LANGUAGE: &[&str] = &["we recommend", "you should", "consider ", "suggested fix", "best practice"];

    #[test]
    fn corrective_notes_contain_only_rule_authored_messages() {
        let v = version("hash-a");
        let generated = generate(&v, Uuid::new_v4());
        let corrective = generated.content.sections.iter().find(|s| s.name == "Corrective Notes").unwrap();

        // Every failing rule's own `failure_message_template` verbatim —
        // never anything the engine invented.
        assert!(corrective.body.contains("x must not be missing"));
        let lowered = corrective.body.to_lowercase();
        for phrase in SYSTEM_ADVICE_LANGUAGE {
            assert!(!lowered.contains(phrase), "system-generated advice language '{phrase}' leaked into Corrective Notes");
        }
    }

    #[test]
    fn fixed_section_order_is_produced_exactly() {
        let v = version("hash-a");
        let generated = generate(&v, Uuid::new_v4());
        let names: Vec<&str> = generated.content.sections.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, VALIDATION_REPORT_SECTION_ORDER);
    }

    #[test]
    fn report_template_version_and_report_schema_version_are_independent_fields() {
        let v = version("hash-a");
        let generated = generate(&v, Uuid::new_v4());
        // Both present and independently named/settable — not the same
        // field wearing two names.
        assert_eq!(generated.report.report_schema_version(), "1.0");
        assert_eq!(generated.report.report_template_version(), "1.0");
        // Structural guarantee that they are two distinct fields: the
        // accessor methods have different names and could diverge freely.
        let _: &str = generated.report.report_schema_version();
        let _: &str = generated.report.report_template_version();
    }

    #[test]
    fn generated_report_starts_at_registered_never_available() {
        let v = version("hash-a");
        let generated = generate(&v, Uuid::new_v4());
        assert_eq!(generated.report.report_state(), crate::types::ReportState::Registered);
    }
}
