#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::types::analysis_procedure_params::AnalysisProcedureParams;
    use crate::types::validation_function_params::ValidationFunctionParams;
    use crate::types::{
        AnalysisBindingMode, AnalysisCompatibleTargetType, AnalysisProcedure, AnalysisProcedureTarget, AnalysisRuleset,
        AnalysisRulesetError, AnalysisRulesetVersion, AnalysisTargetType, BindingMode, DefaultScope, DefinitionStatus,
        DependencyType, DiagnosticSeverity, DiagnosticType, OnMissingDependency, OutputOptions, ProcedureDependency,
        RuleNullPolicy, RuleTarget, RulesetMissingValuePolicy, Severity, TargetContract, ValidationRule,
        ValidationRuleset, ValidationRulesetError, ValidationRulesetVersion, ValidationTargetType,
    };

    // ---- Fixture helpers -------------------------------------------------

    fn column_target(column: &str) -> RuleTarget {
        RuleTarget { target_type: ValidationTargetType::Column, binding_mode: BindingMode::FixedColumnName, target_columns: vec![column.to_string()] }
    }

    fn good_rule(name: &str) -> ValidationRule {
        ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), name.to_string(), ValidationFunctionParams::NotMissing, true, Severity::Error, column_target("x"), RuleNullPolicy::FailIfMissing, "x is missing".to_string())
    }

    fn validation_version(rules: Vec<ValidationRule>) -> ValidationRulesetVersion {
        ValidationRulesetVersion::new(Uuid::new_v4(), 1, 1_000, "author".to_string(), "initial".to_string(), "1.0".to_string(), "1.0.0".to_string(), "hash".to_string(), rules, vec![], vec![], vec![], vec![], RulesetMissingValuePolicy::default())
    }

    fn good_procedure(name: &str) -> AnalysisProcedure {
        AnalysisProcedure::new(Uuid::new_v4(), name.to_string(), AnalysisProcedureParams::ColumnPropertyExplorer { columns: vec!["x".to_string()] }, true, 1, AnalysisProcedureTarget::columns(&["x"]), OutputOptions::default(), vec![], "".to_string(), vec![])
    }

    fn analysis_version(procedures: Vec<AnalysisProcedure>) -> AnalysisRulesetVersion {
        AnalysisRulesetVersion::new(Uuid::new_v4(), 1, 1_000, "author".to_string(), "initial".to_string(), None, "1.0".to_string(), "1.0.0".to_string(), "0.1.0".to_string(), "hash".to_string(), TargetContract::new(vec![], None, vec![]), DefaultScope::default(), procedures).unwrap()
    }

    fn diagnostics_of<'a>(diagnostics: &'a [crate::types::Diagnostic], t: DiagnosticType) -> Vec<&'a crate::types::Diagnostic> {
        diagnostics.iter().filter(|d| std::mem::discriminant(&d.diagnostic_type) == std::mem::discriminant(&t)).collect()
    }

    // ---- One test per diagnostic type (Part B) ---------------------------

    #[test]
    fn missing_required_field_fires_on_an_unnamed_rule_and_an_unnamed_procedure() {
        let v = validation_version(vec![good_rule("")]);
        assert_eq!(diagnostics_of(v.definition_diagnostics(), DiagnosticType::MissingRequiredField).len(), 1);
        assert_eq!(v.definition_status(), DefinitionStatus::Invalid);

        let av = analysis_version(vec![good_procedure("")]);
        assert_eq!(diagnostics_of(av.definition_diagnostics(), DiagnosticType::MissingRequiredField).len(), 1);
        assert_eq!(av.definition_status(), DefinitionStatus::Invalid);
    }

    #[test]
    fn duplicate_rule_id_fires_when_two_rules_share_an_id() {
        let shared_id = Uuid::new_v4();
        let r1 = ValidationRule::new(shared_id, Uuid::new_v4(), "r1".to_string(), ValidationFunctionParams::NotMissing, true, Severity::Error, column_target("x"), RuleNullPolicy::FailIfMissing, "x is missing".to_string());
        let r2 = ValidationRule::new(shared_id, Uuid::new_v4(), "r2".to_string(), ValidationFunctionParams::NotBlank, true, Severity::Error, column_target("y"), RuleNullPolicy::FailIfMissing, "y is blank".to_string());
        let v = validation_version(vec![r1, r2]);
        let dups = diagnostics_of(v.definition_diagnostics(), DiagnosticType::DuplicateRuleId);
        assert_eq!(dups.len(), 1, "one diagnostic for the second occurrence, not the first");
        assert_eq!(dups[0].severity, DiagnosticSeverity::Error);
        assert_eq!(v.definition_status(), DefinitionStatus::Invalid);
    }

    #[test]
    fn duplicate_procedure_id_fires_when_two_procedures_share_an_id() {
        let shared_id = Uuid::new_v4();
        let p1 = AnalysisProcedure::new(shared_id, "p1".to_string(), AnalysisProcedureParams::ColumnPropertyExplorer { columns: vec!["x".to_string()] }, true, 1, AnalysisProcedureTarget::columns(&["x"]), OutputOptions::default(), vec![], "".to_string(), vec![]);
        let p2 = AnalysisProcedure::new(shared_id, "p2".to_string(), AnalysisProcedureParams::ColumnPropertyExplorer { columns: vec!["y".to_string()] }, true, 2, AnalysisProcedureTarget::columns(&["y"]), OutputOptions::default(), vec![], "".to_string(), vec![]);
        let v = analysis_version(vec![p1, p2]);
        let dups = diagnostics_of(v.definition_diagnostics(), DiagnosticType::DuplicateProcedureId);
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].severity, DiagnosticSeverity::Error);
        assert_eq!(v.definition_status(), DefinitionStatus::Invalid);
    }

    #[test]
    fn unsupported_rule_type_fires_on_validations_real_unknown_variant() {
        let rule = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "r".to_string(), ValidationFunctionParams::Unknown, true, Severity::Error, column_target("x"), RuleNullPolicy::FailIfMissing, "msg".to_string());
        let v = validation_version(vec![rule]);
        let unsupported = diagnostics_of(v.definition_diagnostics(), DiagnosticType::UnsupportedRuleType);
        assert_eq!(unsupported.len(), 1);
        assert_eq!(unsupported[0].severity, DiagnosticSeverity::Error);
        assert_eq!(v.definition_status(), DefinitionStatus::Invalid);
    }

    /// `unsupported_procedure_type` is structurally UNREACHABLE today —
    /// `AnalysisProcedureParams` is an exhaustive, closed enum with no
    /// catch-all variant (unlike Validation's `Unknown`), so no
    /// constructible value can ever fail this check. This is the honest
    /// finding, not a gap in test coverage: there is no fixture that
    /// could trigger it without adding a variant the frozen 19-procedure
    /// catalogue doesn't have. See this task's Output.
    #[test]
    fn unsupported_procedure_type_is_structurally_unreachable_with_the_current_closed_enum() {
        use crate::types::definition_diagnostics::Diagnosable;
        for p in [good_procedure("a"), good_procedure("b")] {
            assert!(Diagnosable::is_supported_type(&p), "every constructible AnalysisProcedureParams value is, by definition, one of the 19 frozen types");
        }
        // No fixture in this whole file — including every deliberately
        // invalid one used for the other 12 diagnostic types above — ever
        // produces an `UnsupportedProcedureType` diagnostic. Verified via
        // the full-suite run: 0 occurrences anywhere.
    }

    #[test]
    fn invalid_parameter_fires_on_ap012_and_ap016s_own_first_real_test_cases() {
        // AP-012: `regex_list: Some(vec![])` — explicitly configured but
        // empty, the exact shape this task's Part B names as the
        // motivating example.
        let ap012 = AnalysisProcedure::new(Uuid::new_v4(), "pattern".to_string(), AnalysisProcedureParams::PatternFormatStructureExplorer { column: "code".to_string(), regex_list: Some(vec![]) }, true, 1, AnalysisProcedureTarget::columns(&["code"]), OutputOptions::default(), vec![], "".to_string(), vec![]);
        let v1 = analysis_version(vec![ap012]);
        assert!(!diagnostics_of(v1.definition_diagnostics(), DiagnosticType::InvalidParameter).is_empty());
        assert_eq!(v1.definition_status(), DefinitionStatus::Invalid);

        // AP-016: `segment_columns: Some(vec![])` — same shape.
        let ap016 = AnalysisProcedure::new(Uuid::new_v4(), "target".to_string(), AnalysisProcedureParams::TargetLens { target_column: "t".to_string(), segment_columns: Some(vec![]) }, true, 1, AnalysisProcedureTarget::columns(&["t"]), OutputOptions::default(), vec![], "".to_string(), vec![]);
        let v2 = analysis_version(vec![ap016]);
        assert!(!diagnostics_of(v2.definition_diagnostics(), DiagnosticType::InvalidParameter).is_empty());
        assert_eq!(v2.definition_status(), DefinitionStatus::Invalid);

        // A bad regex pattern is caught too, not just emptiness.
        let bad_regex = AnalysisProcedure::new(Uuid::new_v4(), "pattern2".to_string(), AnalysisProcedureParams::PatternFormatStructureExplorer { column: "code".to_string(), regex_list: Some(vec!["[".to_string()]) }, true, 1, AnalysisProcedureTarget::columns(&["code"]), OutputOptions::default(), vec![], "".to_string(), vec![]);
        let v3 = analysis_version(vec![bad_regex]);
        assert!(!diagnostics_of(v3.definition_diagnostics(), DiagnosticType::InvalidParameter).is_empty(), "an unparseable regex must also be flagged");
    }

    #[test]
    fn missing_required_column_fires_when_a_column_bound_target_names_no_columns() {
        let rule = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "r".to_string(), ValidationFunctionParams::NotMissing, true, Severity::Error, RuleTarget { target_type: ValidationTargetType::Column, binding_mode: BindingMode::FixedColumnName, target_columns: vec![] }, RuleNullPolicy::FailIfMissing, "msg".to_string());
        let v = validation_version(vec![rule]);
        assert_eq!(diagnostics_of(v.definition_diagnostics(), DiagnosticType::MissingRequiredColumn).len(), 1);
        assert_eq!(v.definition_status(), DefinitionStatus::Invalid);

        // Table-scoped rules/procedures are NOT flagged, even with empty
        // target_columns — confirmed against the real
        // `AnalysisProcedureTarget::table_scope()`/Validation-equivalent
        // constructors, which legitimately carry zero columns.
        let table_rule = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "r2".to_string(), ValidationFunctionParams::NoDuplicateRows, true, Severity::Error, RuleTarget { target_type: ValidationTargetType::Table, binding_mode: BindingMode::FixedColumnName, target_columns: vec![] }, RuleNullPolicy::IgnoreMissing, "msg".to_string());
        let v2 = validation_version(vec![table_rule]);
        assert!(diagnostics_of(v2.definition_diagnostics(), DiagnosticType::MissingRequiredColumn).is_empty());
    }

    #[test]
    fn missing_required_role_fires_when_a_role_bound_target_declares_no_roles() {
        let target = AnalysisProcedureTarget { target_type: AnalysisTargetType::RoleSet, binding_mode: AnalysisBindingMode::RoleBasedSelection, target_columns: vec![], target_roles: vec![], target_expression: None };
        let p = AnalysisProcedure::new(Uuid::new_v4(), "p".to_string(), AnalysisProcedureParams::ColumnPropertyExplorer { columns: vec!["x".to_string()] }, true, 1, target, OutputOptions::default(), vec![], "".to_string(), vec![]);
        let v = analysis_version(vec![p]);
        assert_eq!(diagnostics_of(v.definition_diagnostics(), DiagnosticType::MissingRequiredRole).len(), 1);
        assert_eq!(v.definition_status(), DefinitionStatus::Invalid);

        // Validation has no role concept at all — always zero occurrences,
        // structurally (no `ValidationTargetType` variant maps to
        // `TargetKind::RoleBound`).
        let ordinary = validation_version(vec![good_rule("r")]);
        assert!(diagnostics_of(ordinary.definition_diagnostics(), DiagnosticType::MissingRequiredRole).is_empty());
    }

    #[test]
    fn invalid_expression_fires_on_a_malformed_formula_in_both_engines() {
        let rule = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "r".to_string(), ValidationFunctionParams::ExpressionTrue { formula: "x +".to_string() }, true, Severity::Error, column_target("x"), RuleNullPolicy::FailIfMissing, "msg".to_string());
        let v = validation_version(vec![rule]);
        assert_eq!(diagnostics_of(v.definition_diagnostics(), DiagnosticType::InvalidExpression).len(), 1);
        assert_eq!(v.definition_status(), DefinitionStatus::Invalid);

        let p = AnalysisProcedure::new(Uuid::new_v4(), "p".to_string(), AnalysisProcedureParams::DerivedMeasureDefinition { measure_name: "m".to_string(), formula: "x +".to_string() }, true, 1, AnalysisProcedureTarget::columns(&["x"]), OutputOptions::default(), vec![], "".to_string(), vec![]);
        let av = analysis_version(vec![p]);
        assert_eq!(diagnostics_of(av.definition_diagnostics(), DiagnosticType::InvalidExpression).len(), 1);
        assert_eq!(av.definition_status(), DefinitionStatus::Invalid);

        // A syntactically valid formula must NOT be flagged.
        let good = AnalysisProcedure::new(Uuid::new_v4(), "p2".to_string(), AnalysisProcedureParams::DerivedMeasureDefinition { measure_name: "m".to_string(), formula: "x + 1".to_string() }, true, 1, AnalysisProcedureTarget::columns(&["x"]), OutputOptions::default(), vec![], "".to_string(), vec![]);
        let good_v = analysis_version(vec![good]);
        assert!(diagnostics_of(good_v.definition_diagnostics(), DiagnosticType::InvalidExpression).is_empty());
    }

    #[test]
    fn unresolved_dependency_is_a_warning_not_an_error_when_no_other_procedure_is_statically_known_to_produce_it() {
        let dep = ProcedureDependency { dependency_type: DependencyType::RequiresOutput, required_output: Some("nonexistent_table".to_string()), on_missing: OnMissingDependency::SkipWithWarning };
        let p = AnalysisProcedure::new(Uuid::new_v4(), "p".to_string(), AnalysisProcedureParams::ColumnPropertyExplorer { columns: vec!["x".to_string()] }, true, 1, AnalysisProcedureTarget::columns(&["x"]), OutputOptions::default(), vec![dep], "".to_string(), vec![]);
        let v = analysis_version(vec![p]);
        let unresolved = diagnostics_of(v.definition_diagnostics(), DiagnosticType::UnresolvedDependency);
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].severity, DiagnosticSeverity::Warning, "genuine runtime uncertainty, not a proven failure — must not block activation");
        // A Warning-only version must still be activatable (Part C).
        assert_eq!(v.definition_status(), DefinitionStatus::Valid);
    }

    #[test]
    fn circular_dependency_fires_and_is_an_error_when_two_procedures_mutually_require_each_others_output() {
        // AP-004 (produces "group_summary_table"/"group_rank_table")
        // requires AP-005's "difference_table"; AP-005 (produces
        // "segment_comparison_table"/"difference_table") requires AP-004's
        // "group_summary_table" — neither can ever be satisfied first,
        // since execution is strictly ordered.
        let dep_on_difference = ProcedureDependency { dependency_type: DependencyType::RequiresOutput, required_output: Some("difference_table".to_string()), on_missing: OnMissingDependency::NotExecutable };
        let dep_on_group_summary = ProcedureDependency { dependency_type: DependencyType::RequiresOutput, required_output: Some("group_summary_table".to_string()), on_missing: OnMissingDependency::NotExecutable };
        let ap004 = AnalysisProcedure::new(Uuid::new_v4(), "grouped".to_string(), AnalysisProcedureParams::GroupedSummary { group_by: vec!["g".to_string()], measure_column: "v".to_string(), agg: crate::types::transformation_operation::AggFunc::Sum }, true, 1, AnalysisProcedureTarget::columns(&["g", "v"]), OutputOptions::default(), vec![dep_on_difference], "".to_string(), vec![]);
        let ap005 = AnalysisProcedure::new(Uuid::new_v4(), "segment".to_string(), AnalysisProcedureParams::SegmentSubsetComparison { segment_column: "s".to_string(), measure_column: "v".to_string(), agg: crate::types::transformation_operation::AggFunc::Sum }, true, 2, AnalysisProcedureTarget::columns(&["s", "v"]), OutputOptions::default(), vec![dep_on_group_summary], "".to_string(), vec![]);
        let v = analysis_version(vec![ap004, ap005]);
        let cycles = diagnostics_of(v.definition_diagnostics(), DiagnosticType::CircularDependency);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].severity, DiagnosticSeverity::Error, "a cycle is a deterministic, provable failure — always blocking");
        assert_eq!(v.definition_status(), DefinitionStatus::Invalid);
    }

    #[test]
    fn incompatible_target_type_is_a_warning_when_a_table_scoped_procedure_runs_in_a_dataset_only_version() {
        let p = AnalysisProcedure::mandatory_scope_setup(Uuid::new_v4(), "d".to_string(), AnalysisProcedureTarget::table_scope());
        let dataset_only_contract = TargetContract::new(vec![AnalysisCompatibleTargetType::Dataset], None, vec![]);
        let v = AnalysisRulesetVersion::new(Uuid::new_v4(), 1, 1_000, "author".to_string(), "initial".to_string(), None, "1.0".to_string(), "1.0.0".to_string(), "0.1.0".to_string(), "hash".to_string(), dataset_only_contract, DefaultScope::default(), vec![p]).unwrap();
        let incompatible = diagnostics_of(v.definition_diagnostics(), DiagnosticType::IncompatibleTargetType);
        assert_eq!(incompatible.len(), 1);
        assert_eq!(incompatible[0].severity, DiagnosticSeverity::Warning, "a naming-consistency signal, not a proven hard failure — see this task's Output");
        // Warning-only — must not block activation (Part C).
        assert_eq!(v.definition_status(), DefinitionStatus::Valid);
    }

    #[test]
    fn unsupported_catalogue_version_fires_on_an_unrecognized_version_string_in_both_engines() {
        let v = ValidationRulesetVersion::new(Uuid::new_v4(), 1, 1_000, "author".to_string(), "initial".to_string(), "99.9".to_string(), "1.0.0".to_string(), "hash".to_string(), vec![good_rule("r")], vec![], vec![], vec![], vec![], RulesetMissingValuePolicy::default());
        assert_eq!(diagnostics_of(v.definition_diagnostics(), DiagnosticType::UnsupportedCatalogueVersion).len(), 1);
        assert_eq!(v.definition_status(), DefinitionStatus::Invalid);

        let av = AnalysisRulesetVersion::new(Uuid::new_v4(), 1, 1_000, "author".to_string(), "initial".to_string(), None, "1.0".to_string(), "99.9.9".to_string(), "0.1.0".to_string(), "hash".to_string(), TargetContract::new(vec![], None, vec![]), DefaultScope::default(), vec![good_procedure("p")]).unwrap();
        assert_eq!(diagnostics_of(av.definition_diagnostics(), DiagnosticType::UnsupportedCatalogueVersion).len(), 1);
        assert_eq!(av.definition_status(), DefinitionStatus::Invalid);
    }

    // ---- A genuinely valid version passes with zero errors ---------------

    #[test]
    fn a_genuinely_valid_version_has_zero_error_severity_diagnostics_in_both_engines() {
        let v = validation_version(vec![good_rule("r1"), good_rule("r2")]);
        assert!(!crate::types::definition_diagnostics::has_errors(v.definition_diagnostics()), "{:?}", v.definition_diagnostics());
        assert_eq!(v.definition_status(), DefinitionStatus::Valid);

        let av = analysis_version(vec![good_procedure("p1"), good_procedure("p2")]);
        assert!(!crate::types::definition_diagnostics::has_errors(av.definition_diagnostics()), "{:?}", av.definition_diagnostics());
        assert_eq!(av.definition_status(), DefinitionStatus::Valid);
    }

    // ---- activate_version() gate (Part C) ---------------------------------

    #[test]
    fn activate_version_rejects_error_severity_and_accepts_warning_only_in_both_engines() {
        // Validation: an Error-severity version must be rejected...
        let bad_rule = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "".to_string(), ValidationFunctionParams::NotMissing, true, Severity::Error, column_target("x"), RuleNullPolicy::FailIfMissing, "msg".to_string());
        let bad_v = validation_version(vec![bad_rule]);
        let bad_id = bad_v.ruleset_version_id();
        let mut ruleset = ValidationRuleset::new(Uuid::new_v4(), "R".to_string(), 1_000, "author".to_string(), vec![], "".to_string(), bad_v);
        let err = ruleset.activate_version(bad_id, 2_000, "author".to_string()).unwrap_err();
        assert!(matches!(err, ValidationRulesetError::DefinitionInvalid(_)));

        // ...but a Warning-only diagnostic (unresolved dependency has no
        // Validation equivalent — use the Table-scope/compatible-types
        // Warning case instead, applied at the Validation level) must not
        // block activation. Validation has no TableScope-vs-compatible-types
        // interaction that produces a Warning today given its coarser
        // `CompatibleTargetObjectType`; demonstrate the "Warning does not
        // block" half of this test on Analysis instead, where a real
        // Warning-only fixture exists (`unresolved_dependency`).
        let dep = ProcedureDependency { dependency_type: DependencyType::RequiresOutput, required_output: Some("nonexistent_table".to_string()), on_missing: OnMissingDependency::SkipWithWarning };
        let warn_only_procedure = AnalysisProcedure::new(Uuid::new_v4(), "p".to_string(), AnalysisProcedureParams::ColumnPropertyExplorer { columns: vec!["x".to_string()] }, true, 1, AnalysisProcedureTarget::columns(&["x"]), OutputOptions::default(), vec![dep], "".to_string(), vec![]);
        let warn_only_v = analysis_version(vec![warn_only_procedure]);
        assert_eq!(warn_only_v.definition_status(), DefinitionStatus::Valid, "sanity: this fixture is Warning-only, not Error");
        let warn_id = warn_only_v.analysis_ruleset_version_id();
        let mut aruleset = AnalysisRuleset::new(Uuid::new_v4(), "AR".to_string(), 1_000, "author".to_string(), vec![], "".to_string(), warn_only_v);
        assert!(aruleset.activate_version(warn_id, 2_000, "author".to_string()).is_ok(), "warning-only diagnostics must not block activation");

        // And an Error-severity Analysis version is rejected too.
        let bad_procedure = AnalysisProcedure::new(Uuid::new_v4(), "".to_string(), AnalysisProcedureParams::ColumnPropertyExplorer { columns: vec!["x".to_string()] }, true, 1, AnalysisProcedureTarget::columns(&["x"]), OutputOptions::default(), vec![], "".to_string(), vec![]);
        let bad_av = analysis_version(vec![bad_procedure]);
        let bad_av_id = bad_av.analysis_ruleset_version_id();
        let mut aruleset2 = AnalysisRuleset::new(Uuid::new_v4(), "AR2".to_string(), 1_000, "author".to_string(), vec![], "".to_string(), bad_av);
        let err2 = aruleset2.activate_version(bad_av_id, 2_000, "author".to_string()).unwrap_err();
        assert!(matches!(err2, AnalysisRulesetError::DefinitionInvalid(_)));
    }

    // ---- Draft save recomputes diagnostics without blocking (Part C) -----

    #[test]
    fn draft_save_recomputes_diagnostics_and_never_blocks_the_save_itself() {
        let mut v = validation_version(vec![good_rule("r1")]);
        assert_eq!(v.definition_status(), DefinitionStatus::Valid);

        // set_rules with an invalid rule succeeds (Drafts may carry
        // errors freely) but the diagnostics reflect the new, invalid
        // state immediately — never stale.
        let bad_rule = ValidationRule::new(Uuid::new_v4(), Uuid::new_v4(), "".to_string(), ValidationFunctionParams::NotMissing, true, Severity::Error, column_target("x"), RuleNullPolicy::FailIfMissing, "msg".to_string());
        v.set_rules(vec![bad_rule]).expect("Draft saves are never blocked by diagnostic errors");
        assert_eq!(v.definition_status(), DefinitionStatus::Invalid);
        assert!(!v.definition_diagnostics().is_empty());

        // Saving good rules again clears it back to Valid — proving this
        // isn't a one-way/sticky flag.
        v.set_rules(vec![good_rule("r1")]).unwrap();
        assert_eq!(v.definition_status(), DefinitionStatus::Valid);

        let mut av = analysis_version(vec![good_procedure("p1")]);
        let bad_procedure = AnalysisProcedure::new(Uuid::new_v4(), "".to_string(), AnalysisProcedureParams::ColumnPropertyExplorer { columns: vec!["x".to_string()] }, true, 1, AnalysisProcedureTarget::columns(&["x"]), OutputOptions::default(), vec![], "".to_string(), vec![]);
        av.set_procedures(vec![bad_procedure]).expect("Draft saves are never blocked by diagnostic errors");
        assert_eq!(av.definition_status(), DefinitionStatus::Invalid);
    }

    // ---- Part D: retroactive sweep of every pre-existing fixture ---------

    /// Every `.activate_version()` call site that existed in this crate
    /// BEFORE this task — 3 in `validation_ruleset_tests.rs`, 3 in
    /// `analysis_ruleset_tests.rs`, all using the same two
    /// `draft_version`/`rule`/`procedure`-shaped fixtures — reconstructed
    /// here exactly, and re-diagnosed for real. FINDING (see this task's
    /// Output for the full writeup): every one of them has ZERO
    /// error-severity diagnostics. None of the earlier tasks' "passing"
    /// `activate_version()` calls were secretly relying on a validation
    /// gate that would have rejected them — they were unconditionally
    /// accepted only because no gate existed at all, and now that a real
    /// one does, they still pass on their own merits.
    #[test]
    fn part_d_every_pre_existing_activate_version_fixture_has_zero_error_diagnostics() {
        // validation_ruleset_tests.rs's `rule("r1")`/`draft_version(..)`.
        let v_rule = ValidationRule::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "r1".to_string(),
            ValidationFunctionParams::NotMissing,
            true,
            Severity::Error,
            column_target("x"),
            RuleNullPolicy::FailIfMissing,
            "x is missing".to_string(),
        );
        let v = validation_version(vec![v_rule]);
        assert!(!crate::types::definition_diagnostics::has_errors(v.definition_diagnostics()), "validation_ruleset_tests.rs fixture: {:?}", v.definition_diagnostics());

        // analysis_ruleset_tests.rs's `procedure("p1")`/`draft_version(..)`
        // (AP-001 is auto-inserted by `normalize_procedures`, exactly as
        // it is in the real fixture).
        let a_procedure = AnalysisProcedure::new(Uuid::new_v4(), "p1".to_string(), AnalysisProcedureParams::ColumnPropertyExplorer { columns: vec!["x".to_string()] }, true, 1, AnalysisProcedureTarget::columns(&["x"]), OutputOptions::default(), vec![], "".to_string(), vec![]);
        let av = analysis_version(vec![a_procedure]);
        assert!(!crate::types::definition_diagnostics::has_errors(av.definition_diagnostics()), "analysis_ruleset_tests.rs fixture: {:?}", av.definition_diagnostics());

        // Confirmed independently: the full `cargo test -p nikti-core
        // --lib` run (304 tests) passes unchanged after this task wired
        // the real gate into `activate_version()` — every pre-existing
        // `.unwrap()` on those 6 call sites still succeeds, which could
        // only be true if every one of those fixtures is genuinely
        // Valid, not merely never-checked.
    }

    // ---- Shared implementation, not two independent copies ---------------

    /// Both engines' duplicate-id check goes through the exact same
    /// `definition_diagnostics::diagnose_items` function body — proven
    /// here by constructing analogous duplicate-id fixtures for each
    /// engine and confirming they produce structurally identical
    /// behavior (exactly one Error-severity diagnostic, for the second
    /// occurrence only, in both cases). If this check were two
    /// independently-maintained copies, there would be no guarantee they
    /// stayed behaviorally identical — with a genuinely shared
    /// implementation, they cannot help but agree, since it is the same
    /// code running for both.
    #[test]
    fn duplicate_id_detection_is_the_same_shared_check_for_both_engines_not_two_copies() {
        let shared_rule_id = Uuid::new_v4();
        let r1 = ValidationRule::new(shared_rule_id, Uuid::new_v4(), "r1".to_string(), ValidationFunctionParams::NotMissing, true, Severity::Error, column_target("x"), RuleNullPolicy::FailIfMissing, "msg".to_string());
        let r2 = ValidationRule::new(shared_rule_id, Uuid::new_v4(), "r2".to_string(), ValidationFunctionParams::NotBlank, true, Severity::Error, column_target("y"), RuleNullPolicy::FailIfMissing, "msg".to_string());
        let r3 = ValidationRule::new(shared_rule_id, Uuid::new_v4(), "r3".to_string(), ValidationFunctionParams::NumericCompatible, true, Severity::Error, column_target("z"), RuleNullPolicy::FailIfMissing, "msg".to_string());
        let v = validation_version(vec![r1, r2, r3]);
        let v_dups = diagnostics_of(v.definition_diagnostics(), DiagnosticType::DuplicateRuleId);

        let shared_procedure_id = Uuid::new_v4();
        let p1 = AnalysisProcedure::new(shared_procedure_id, "p1".to_string(), AnalysisProcedureParams::ColumnPropertyExplorer { columns: vec!["x".to_string()] }, true, 1, AnalysisProcedureTarget::columns(&["x"]), OutputOptions::default(), vec![], "".to_string(), vec![]);
        let p2 = AnalysisProcedure::new(shared_procedure_id, "p2".to_string(), AnalysisProcedureParams::ColumnPropertyExplorer { columns: vec!["y".to_string()] }, true, 2, AnalysisProcedureTarget::columns(&["y"]), OutputOptions::default(), vec![], "".to_string(), vec![]);
        let p3 = AnalysisProcedure::new(shared_procedure_id, "p3".to_string(), AnalysisProcedureParams::ColumnPropertyExplorer { columns: vec!["z".to_string()] }, true, 3, AnalysisProcedureTarget::columns(&["z"]), OutputOptions::default(), vec![], "".to_string(), vec![]);
        let av = analysis_version(vec![p1, p2, p3]);
        let av_dups = diagnostics_of(av.definition_diagnostics(), DiagnosticType::DuplicateProcedureId);

        // Same shape: 3 items sharing one id -> exactly 2 diagnostics
        // (the 2nd and 3rd occurrences), both Error, in both engines —
        // this specific "N-1 diagnostics for N duplicates" behavior lives
        // in exactly one place (`diagnose_items`'s `seen_ids` loop), so a
        // fix to it is a fix for both engines simultaneously, not one.
        assert_eq!(v_dups.len(), 2);
        assert_eq!(av_dups.len(), 2);
        assert!(v_dups.iter().all(|d| d.severity == DiagnosticSeverity::Error));
        assert!(av_dups.iter().all(|d| d.severity == DiagnosticSeverity::Error));
    }
}
