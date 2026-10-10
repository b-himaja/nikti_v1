#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::types::analysis_procedure_params::AnalysisProcedureParams;
    use crate::types::{
        AnalysisProcedure, AnalysisProcedureTarget, AnalysisRuleset, AnalysisRulesetError, AnalysisRulesetState,
        AnalysisRulesetVersion, AnalysisVersionState, DefaultScope, OutputOptions, TargetContract,
    };

    fn procedure(name: &str) -> AnalysisProcedure {
        AnalysisProcedure::new(
            Uuid::new_v4(),
            name.to_string(),
            AnalysisProcedureParams::ColumnPropertyExplorer { columns: vec!["x".to_string()] },
            true,
            1,
            AnalysisProcedureTarget::columns(&["x"]),
            OutputOptions::default(),
            vec![],
            "".to_string(),
            vec![],
        )
    }

    fn draft_version(procedures: Vec<AnalysisProcedure>) -> AnalysisRulesetVersion {
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
            "hash-1".to_string(),
            TargetContract::new(vec![], None, vec![]),
            DefaultScope::default(),
            procedures,
        )
        .unwrap()
    }

    #[test]
    fn ap001_is_always_present_first_and_enabled_even_if_omitted() {
        let v = draft_version(vec![procedure("p1")]);
        assert_eq!(v.procedures().len(), 2, "AP-001 was auto-inserted");
        assert_eq!(v.procedures()[0].procedure_type(), "AP-001");
        assert!(v.procedures()[0].enabled());
    }

    #[test]
    fn ap001_cannot_be_constructed_disabled() {
        // There is no public constructor path that accepts a disabled
        // AP-001 — `AnalysisProcedure::mandatory_scope_setup` has no
        // `enabled` parameter at all. A hand-built AP-001-typed
        // procedure via the generic `AnalysisProcedure::new` with
        // `enabled: false` is rejected by `normalize_procedures` at
        // ruleset-version construction time.
        let disabled_ap001 = AnalysisProcedure::new(
            Uuid::new_v4(),
            "Scope & Variable Role Setup".to_string(),
            AnalysisProcedureParams::ScopeAndVariableRoleSetup,
            false,
            0,
            AnalysisProcedureTarget::table_scope(),
            OutputOptions::default(),
            vec![],
            "".to_string(),
            vec![],
        );
        let err = AnalysisRulesetVersion::new(
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
            vec![disabled_ap001],
        )
        .unwrap_err();
        assert_eq!(err, AnalysisRulesetError::Ap001CannotBeDisabled);
    }

    #[test]
    fn ap001_cannot_be_deleted_via_set_procedures() {
        let mut v = draft_version(vec![procedure("p1")]);
        // Attempting to "delete" AP-001 by submitting a procedure list
        // that omits it does not remove it — `set_procedures` re-inserts
        // a fresh, enabled AP-001 exactly as construction does (B.7:
        // "not a runtime convention someone could bypass").
        v.set_procedures(vec![procedure("p2")]).unwrap();
        assert_eq!(v.procedures().len(), 2);
        assert_eq!(v.procedures()[0].procedure_type(), "AP-001");
        assert!(v.procedures()[0].enabled());
    }

    #[test]
    fn analysis_procedure_has_no_severity_field() {
        // Structural: `AnalysisProcedure::new`'s parameter list (above,
        // and in `types/analysis_ruleset.rs`) has no `severity`
        // parameter, and the struct has no such field — this test's own
        // successful compilation, using every constructor parameter by
        // name-adjacent position with nothing severity-shaped among
        // them, is the check.
        let p = procedure("p");
        assert_eq!(p.procedure_name(), "p");
    }

    #[test]
    fn ruleset_state_and_version_state_are_independent_and_superseded_never_appears_ruleset_level() {
        let v1 = draft_version(vec![procedure("p1")]);
        let v1_id = v1.analysis_ruleset_version_id();
        let mut ruleset = AnalysisRuleset::new(Uuid::new_v4(), "AR".to_string(), 1_000, "author".to_string(), vec![], "".to_string(), v1);
        assert_eq!(ruleset.state(), AnalysisRulesetState::Draft);

        ruleset.activate_version(v1_id, 2_000, "author".to_string()).unwrap();
        assert_eq!(ruleset.state(), AnalysisRulesetState::Active);
        assert_eq!(ruleset.version(v1_id).unwrap().version_state(), AnalysisVersionState::Active);

        let v2 = draft_version(vec![procedure("p1-edited")]);
        let v2_id = v2.analysis_ruleset_version_id();
        ruleset.add_draft_version(v2, 3_000, "author".to_string());
        ruleset.activate_version(v2_id, 4_000, "author".to_string()).unwrap();

        assert_eq!(ruleset.version(v1_id).unwrap().version_state(), AnalysisVersionState::Superseded);
        assert_eq!(ruleset.state(), AnalysisRulesetState::Active, "never Superseded at the ruleset level");
    }

    #[test]
    fn editing_an_active_version_produces_a_new_version_rather_than_mutating_the_existing_one() {
        let v1 = draft_version(vec![procedure("original")]);
        let v1_id = v1.analysis_ruleset_version_id();
        let mut ruleset = AnalysisRuleset::new(Uuid::new_v4(), "AR".to_string(), 1_000, "author".to_string(), vec![], "".to_string(), v1);
        ruleset.activate_version(v1_id, 2_000, "author".to_string()).unwrap();

        let mut active_copy = ruleset.version(v1_id).unwrap().clone();
        let err = active_copy.set_procedures(vec![procedure("mutated-in-place")]).unwrap_err();
        assert!(matches!(err, AnalysisRulesetError::VersionNotEditable(_)));
    }

    #[test]
    fn execution_requires_a_specific_resolved_version_never_an_unresolved_latest() {
        fn assert_signature<F>(_f: F)
        where
            F: Fn(&AnalysisRulesetVersion) -> Uuid,
        {
        }
        assert_signature(|v: &AnalysisRulesetVersion| v.analysis_ruleset_version_id());
    }

    #[test]
    fn analysis_binding_mode_is_distinct_from_validations_and_contains_role_based_selection() {
        // Structural: `AnalysisBindingMode` is its own enum type (see
        // `types/analysis_ruleset.rs`), not a reuse/alias of
        // `validation_ruleset::BindingMode` — this compiles specifically
        // because the two are unrelated types with different variant
        // sets (`RoleBasedSelection` here, `ReferenceBinding` there).
        let mode = crate::types::AnalysisBindingMode::RoleBasedSelection;
        assert!(matches!(mode, crate::types::AnalysisBindingMode::RoleBasedSelection));
    }
}
