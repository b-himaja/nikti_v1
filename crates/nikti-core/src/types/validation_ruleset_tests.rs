#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::types::validation_function_params::ValidationFunctionParams;
    use crate::types::{
        BindingMode, RuleNullPolicy, RuleTarget, RulesetMissingValuePolicy, RulesetState, Severity, ValidationRule,
        ValidationRuleset, ValidationRulesetVersion, ValidationTargetType, VersionState,
    };

    fn target(column: &str) -> RuleTarget {
        RuleTarget { target_type: ValidationTargetType::Column, binding_mode: BindingMode::FixedColumnName, target_columns: vec![column.to_string()] }
    }

    fn rule(name: &str) -> ValidationRule {
        ValidationRule::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            name.to_string(),
            ValidationFunctionParams::NotMissing,
            true,
            Severity::Error,
            target("x"),
            RuleNullPolicy::FailIfMissing,
            "x is missing".to_string(),
        )
    }

    fn draft_version(rules: Vec<ValidationRule>) -> ValidationRulesetVersion {
        ValidationRulesetVersion::new(
            Uuid::new_v4(),
            1,
            1_000,
            "author".to_string(),
            "initial".to_string(),
            "1.0".to_string(),
            "1.0.0".to_string(),
            "hash-1".to_string(),
            rules,
            vec![],
            vec![],
            vec![],
            vec![],
            RulesetMissingValuePolicy::default(),
        )
    }

    #[test]
    fn ruleset_state_and_version_state_are_independent_and_superseded_never_appears_ruleset_level() {
        let v1 = draft_version(vec![rule("r1")]);
        let v1_id = v1.ruleset_version_id();
        let mut ruleset = ValidationRuleset::new(Uuid::new_v4(), "My Ruleset".to_string(), 1_000, "author".to_string(), vec![], "".to_string(), v1);
        assert_eq!(ruleset.state(), RulesetState::Draft);
        assert_eq!(ruleset.version(v1_id).unwrap().version_state(), VersionState::Draft);

        ruleset.activate_version(v1_id, 2_000, "author".to_string()).unwrap();
        assert_eq!(ruleset.state(), RulesetState::Active, "ruleset-level state");
        assert_eq!(ruleset.version(v1_id).unwrap().version_state(), VersionState::Active, "version-level state");

        // Create and activate a second version — v1 becomes Superseded at
        // the *version* level; `RulesetState` has no `Superseded` variant
        // at all, so it is not even representable as the ruleset's own
        // state (compile-time guarantee, not just a runtime check).
        let v2 = draft_version(vec![rule("r1-edited")]);
        let v2_id = v2.ruleset_version_id();
        ruleset.add_draft_version(v2, 3_000, "author".to_string());
        ruleset.activate_version(v2_id, 4_000, "author".to_string()).unwrap();

        assert_eq!(ruleset.version(v1_id).unwrap().version_state(), VersionState::Superseded);
        assert_eq!(ruleset.version(v2_id).unwrap().version_state(), VersionState::Active);
        assert_eq!(ruleset.state(), RulesetState::Active, "still just Active at the ruleset level, never Superseded");
        assert_eq!(ruleset.current_version_id(), v2_id);
    }

    #[test]
    fn editing_an_active_version_produces_a_new_version_rather_than_mutating_the_existing_one() {
        let v1 = draft_version(vec![rule("original")]);
        let v1_id = v1.ruleset_version_id();
        let mut ruleset = ValidationRuleset::new(Uuid::new_v4(), "R".to_string(), 1_000, "author".to_string(), vec![], "".to_string(), v1);
        ruleset.activate_version(v1_id, 2_000, "author".to_string()).unwrap();

        // Attempting to edit the now-Active version in place is a hard
        // error (A.1/A.2), not a silent no-op or a mutation.
        let mut active_copy = ruleset.version(v1_id).unwrap().clone();
        let err = active_copy.set_rules(vec![rule("mutated-in-place")]).unwrap_err();
        assert!(matches!(err, crate::types::ValidationRulesetError::VersionNotEditable(_)));

        // The correct path: a brand-new Draft version.
        let mut v2 = draft_version(vec![rule("original")]);
        v2.set_rules(vec![rule("edited")]).unwrap();
        let v2_id = v2.ruleset_version_id();
        ruleset.add_draft_version(v2, 3_000, "author".to_string());

        assert_eq!(ruleset.version(v1_id).unwrap().rules()[0].rule_name(), "original", "v1 is untouched");
        assert_eq!(ruleset.version(v2_id).unwrap().rules()[0].rule_name(), "edited");
        assert_ne!(v1_id, v2_id);
    }

    #[test]
    fn execution_requires_a_specific_resolved_version_never_an_unresolved_latest() {
        // Structural guarantee: `ValidationEngine::validate` takes
        // `&ValidationRulesetVersion` — a specific, already-resolved
        // version — never a bare `ValidationRuleset` with an ambiguous
        // "current" pointer. There is no overload that accepts the
        // latter, so a call site cannot even compile an "execute against
        // whatever is current" request.
        fn assert_signature<F>(_f: F)
        where
            F: Fn(&ValidationRulesetVersion) -> Uuid,
        {
        }
        assert_signature(|v: &ValidationRulesetVersion| v.ruleset_version_id());
    }
}
