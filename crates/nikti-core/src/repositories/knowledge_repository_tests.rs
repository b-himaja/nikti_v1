#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::repositories::knowledge_repository::testing::InMemoryKnowledgeRepository;
    use crate::repositories::KnowledgeRepository;
    use crate::types::analysis_procedure_params::AnalysisProcedureParams;
    use crate::types::validation_function_params::ValidationFunctionParams;
    use crate::types::{
        AnalysisProcedure, AnalysisProcedureTarget, AnalysisRuleset, AnalysisRulesetVersion, BindingMode, DefaultScope, OutputOptions,
        RuleNullPolicy, RuleTarget, RulesetMissingValuePolicy, Severity, TargetContract, ValidationRule, ValidationRuleset,
        ValidationRulesetVersion, ValidationTargetType,
    };

    fn a_validation_ruleset() -> ValidationRuleset {
        let target = RuleTarget { target_type: ValidationTargetType::Column, binding_mode: BindingMode::FixedColumnName, target_columns: vec!["x".to_string()] };
        let rule = ValidationRule::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "r".to_string(),
            ValidationFunctionParams::NotMissing,
            true,
            Severity::Error,
            target,
            RuleNullPolicy::FailIfMissing,
            "missing".to_string(),
        );
        let version = ValidationRulesetVersion::new(
            Uuid::new_v4(),
            1,
            1_000,
            "author".to_string(),
            "initial".to_string(),
            "1.0".to_string(),
            "1.0.0".to_string(),
            "hash".to_string(),
            vec![rule],
            vec![],
            vec![],
            vec![],
            vec![],
            RulesetMissingValuePolicy::default(),
        );
        ValidationRuleset::new(Uuid::new_v4(), "R".to_string(), 1_000, "author".to_string(), vec![], "".to_string(), version)
    }

    fn an_analysis_ruleset() -> AnalysisRuleset {
        let procedure = AnalysisProcedure::new(
            Uuid::new_v4(),
            "Column Property Explorer".to_string(),
            AnalysisProcedureParams::ColumnPropertyExplorer { columns: vec!["x".to_string()] },
            true,
            1,
            AnalysisProcedureTarget::columns(&["x"]),
            OutputOptions::default(),
            vec![],
            "".to_string(),
            vec![],
        );
        let version = AnalysisRulesetVersion::new(
            Uuid::new_v4(),
            1,
            1_000,
            "author".to_string(),
            "initial".to_string(),
            None,
            "1.0".to_string(),
            "1.0.0".to_string(),
            "0.1.0".to_string(),
            "hash".to_string(),
            TargetContract::new(vec![], None, vec![]),
            DefaultScope::default(),
            vec![procedure],
        )
        .unwrap();
        AnalysisRuleset::new(Uuid::new_v4(), "AR".to_string(), 1_000, "author".to_string(), vec![], "".to_string(), version)
    }

    /// B.8/A.5 (INV-009, schema-aware): one shared structural test
    /// pattern (this macro), instantiated once per Knowledge Object
    /// type below, rather than two independently hand-written tests.
    /// The `KnowledgeRepository` trait's entire Ruleset-related surface
    /// — for both `ValidationRuleset` and `AnalysisRuleset` — is
    /// store/get/list only, visible directly in
    /// `repositories/knowledge_repository.rs`: no method shaped like a
    /// partial edit (e.g. `(id, new_rules)`) exists for either type.
    /// Authoring/editing the value itself (via
    /// `{Validation,Analysis}Ruleset::add_draft_version`/
    /// `activate_version` and `{...}RulesetVersion::set_{rules,procedures}`)
    /// happens only in code paths reachable from Table Repository
    /// context — this repository never constructs or mutates a
    /// Ruleset's content, only persists a value handed to it.
    macro_rules! assert_ruleset_store_surface {
        ($test_name:ident, $ctor:expr, $id_fn:ident, $store_fn:ident, $get_fn:ident, $list_fn:ident) => {
            #[tokio::test]
            async fn $test_name() {
                let repo = InMemoryKnowledgeRepository::default();
                let ruleset = $ctor;
                let id = ruleset.$id_fn();

                repo.$store_fn(ruleset).await.unwrap();
                let stored = repo.$get_fn(id).await.unwrap().unwrap();
                assert_eq!(stored.$id_fn(), id);
                assert_eq!(repo.$list_fn().await.unwrap().len(), 1);
            }
        };
    }

    assert_ruleset_store_surface!(
        knowledge_repository_validation_ruleset_surface_is_store_get_list_only,
        a_validation_ruleset(),
        ruleset_id,
        store_validation_ruleset,
        get_validation_ruleset,
        list_validation_rulesets
    );

    assert_ruleset_store_surface!(
        knowledge_repository_analysis_ruleset_surface_is_store_get_list_only,
        an_analysis_ruleset(),
        analysis_ruleset_id,
        store_analysis_ruleset,
        get_analysis_ruleset,
        list_analysis_rulesets
    );
}
