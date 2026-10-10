//! Shared Definition Diagnostics engine (Part A of the task that added
//! this module): ONE implementation used by both Validation and
//! Analysis to check a ruleset version's definition for correctness
//! before it may be activated. Neither engine gets its own independently
//! -written copy of these checks — each implements `Diagnosable` for its
//! own rule/procedure type (`ValidationRule` in `validation_ruleset.rs`,
//! `AnalysisProcedure` in `analysis_ruleset.rs`) and calls
//! `diagnose_items` here, which is written exactly once.
//!
//! No "Manager spec" document defining a diagnostic-type vocabulary or
//! severity model exists anywhere in this repository — confirmed by an
//! exhaustive repo-wide search before writing this module. The
//! diagnostic type names below are taken verbatim from the task prompt
//! that commissioned this module, since that is the only source for
//! them. The severity assigned to each check — and the full reasoning
//! behind it, including several checks' exact semantics against this
//! codebase's actual (not spec-described) object model — is recorded in
//! full in `docs/adr/0008-definition-diagnostic-severities.md`, which is
//! the source of truth for "why this severity"; the short comments at
//! each check site below point to it rather than restating it, so there
//! is exactly one place to update if a severity or its reasoning ever
//! changes.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
}

/// Shared by both engines — Validation had no equivalent field at all
/// before this task; Analysis already had this exact enum (unused,
/// hardcoded to `Valid` at construction) and now gets a real producer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DefinitionStatus {
    Valid,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticType {
    MissingRequiredField,
    DuplicateRuleId,
    DuplicateProcedureId,
    UnsupportedRuleType,
    UnsupportedProcedureType,
    InvalidParameter,
    MissingRequiredColumn,
    MissingRequiredRole,
    InvalidExpression,
    UnresolvedDependency,
    CircularDependency,
    IncompatibleTargetType,
    UnsupportedCatalogueVersion,
}

impl DiagnosticType {
    /// The exact snake_case identifiers this task's prompt specifies.
    pub fn id(&self) -> &'static str {
        match self {
            Self::MissingRequiredField => "missing_required_field",
            Self::DuplicateRuleId => "duplicate_rule_id",
            Self::DuplicateProcedureId => "duplicate_procedure_id",
            Self::UnsupportedRuleType => "unsupported_rule_type",
            Self::UnsupportedProcedureType => "unsupported_procedure_type",
            Self::InvalidParameter => "invalid_parameter",
            Self::MissingRequiredColumn => "missing_required_column",
            Self::MissingRequiredRole => "missing_required_role",
            Self::InvalidExpression => "invalid_expression",
            Self::UnresolvedDependency => "unresolved_dependency",
            Self::CircularDependency => "circular_dependency",
            Self::IncompatibleTargetType => "incompatible_target_type",
            Self::UnsupportedCatalogueVersion => "unsupported_catalogue_version",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub diagnostic_type: DiagnosticType,
    pub severity: DiagnosticSeverity,
    /// The rule_id/procedure_id this diagnostic is about, if it's about
    /// one specific item rather than the version as a whole (e.g.
    /// `circular_dependency` names no single item — the cycle involves
    /// several).
    pub subject_id: Option<Uuid>,
    pub message: String,
}

impl Diagnostic {
    fn new(diagnostic_type: DiagnosticType, severity: DiagnosticSeverity, subject_id: Option<Uuid>, message: impl Into<String>) -> Self {
        Self { diagnostic_type, severity, subject_id, message: message.into() }
    }
}

pub fn has_errors(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(|d| d.severity == DiagnosticSeverity::Error)
}

pub fn status_from(diagnostics: &[Diagnostic]) -> DefinitionStatus {
    if has_errors(diagnostics) {
        DefinitionStatus::Invalid
    } else {
        DefinitionStatus::Valid
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    /// Binds to specific named column(s) — a column list must be
    /// non-empty (Validation's `Column`/`ColumnSet`, Analysis's
    /// `Column`/`ColumnSet`).
    ColumnBound,
    /// Operates on the whole object, no specific columns needed
    /// (Validation's `Table`, Analysis's `TableScope`) — deliberately
    /// NOT checked for empty `target_columns` (AP-001/AP-019's
    /// `AnalysisProcedureTarget::table_scope()` and Validation's
    /// equivalent legitimately carry zero named columns).
    TableScope,
    /// Analysis-only: binds by variable role rather than column name
    /// (`AnalysisTargetType::RoleSet`) — a role list must be non-empty.
    /// Validation has no role concept at all, so no `ValidationRule`
    /// ever reports this kind.
    RoleBound,
    /// Row/Cell/Reference/Expression (Validation) or Expression
    /// (Analysis) — no column/role completeness check applies.
    Other,
}

/// A single declared dependency, engine-agnostic. `required_output` is a
/// free-form name — the only dependency concept either engine's object
/// model has today (`ProcedureDependency::required_output`, Analysis
/// only; Validation has no dependency concept and always reports zero
/// of these).
#[derive(Debug, Clone)]
pub struct DependencyRef {
    pub required_output: Option<String>,
}

/// Part A's shared interface. `ValidationRule` and `AnalysisProcedure`
/// each implement this once, against their own real field shapes — the
/// checks in `diagnose_items`/`diagnose_dependencies` below are written
/// exactly once and run identically for both engines.
pub trait Diagnosable {
    fn item_id(&self) -> Uuid;
    fn item_name(&self) -> &str;
    fn type_id(&self) -> String;
    fn is_supported_type(&self) -> bool;
    fn target_kind(&self) -> TargetKind;
    fn target_columns(&self) -> &[String];
    fn target_roles_count(&self) -> usize;
    /// `Some(formula)` for the one function/procedure type per engine
    /// that carries a raw expression string (Validation's
    /// `ExpressionTrue`, Analysis's `DerivedMeasureDefinition`); `None`
    /// for every other type.
    fn expression_to_check(&self) -> Option<&str>;
    /// Per-item checks beyond the generic ones above — each engine
    /// supplies its own (see `validation_function_parameter_diagnostics`/
    /// `analysis_procedure_parameter_diagnostics`), covering both
    /// `invalid_parameter` (a function/procedure's own payload fields
    /// are semantically empty/degenerate) and any additional
    /// `missing_required_field` cases specific to the container type
    /// (e.g. Validation's `failure_message_template`).
    fn extra_diagnostics(&self) -> Vec<(DiagnosticType, DiagnosticSeverity, String)>;
    fn dependencies(&self) -> Vec<DependencyRef>;
    /// The output table names this item is UNCONDITIONALLY known to
    /// produce, independent of its own configured parameters — used only
    /// for dependency resolution. Deliberately excludes any
    /// conditionally-produced table (e.g. AP-012's `regex_measure_table`,
    /// which only exists if `regex_list` is configured) since whether
    /// those exist can't be determined without re-running that
    /// procedure's own parameter logic — see `unresolved_dependency`'s
    /// documented Warning-not-Error severity below for why this matters.
    fn known_output_names(&self) -> Vec<String>;
}

/// Runs every shared check against `items` (all rules in a
/// `ValidationRulesetVersion`, or all procedures in an
/// `AnalysisRulesetVersion`) plus the dependency/cycle checks. Each
/// engine's own `diagnose_*_ruleset_version` wrapper supplies
/// `table_type_allowed` (does this version's own compatible-target-types
/// list include the "whole table" object kind?), `duplicate_id_type`,
/// and `unsupported_type_type` (which of the two engine-specific
/// diagnostic-type names applies).
pub fn diagnose_items<T: Diagnosable>(items: &[T], table_type_allowed: bool, duplicate_id_type: DiagnosticType, unsupported_type_type: DiagnosticType) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen_ids: HashSet<Uuid> = HashSet::new();

    for item in items {
        let id = item.item_id();

        // duplicate_rule_id / duplicate_procedure_id — Error. See ADR 0008.
        if !seen_ids.insert(id) {
            diagnostics.push(Diagnostic::new(duplicate_id_type, DiagnosticSeverity::Error, Some(id), format!("id {id} is shared by more than one item in this version — every id must be unique within a version")));
        }

        // missing_required_field — Error. See ADR 0008.
        if item.item_name().trim().is_empty() {
            diagnostics.push(Diagnostic::new(DiagnosticType::MissingRequiredField, DiagnosticSeverity::Error, Some(id), "name must not be empty"));
        }

        // unsupported_rule_type / unsupported_procedure_type — Error, but
        // structurally unreachable for Analysis today (its params enum
        // is exhaustive/closed with no catch-all variant). See ADR 0008.
        if !item.is_supported_type() {
            diagnostics.push(Diagnostic::new(unsupported_type_type, DiagnosticSeverity::Error, Some(id), format!("type '{}' is not part of the frozen catalogue for this engine", item.type_id())));
        }

        // missing_required_column (Error) / missing_required_role (Error)
        // / incompatible_target_type (Warning) — severities and reasoning
        // in ADR 0008. Keyed off `target_kind()`, NOT off binding_mode:
        // both engines' `table_scope()`-style constructors leave
        // `binding_mode` at its `FixedColumnName` default even though no
        // column is bound, so binding_mode alone is not a reliable signal
        // here — verified against real code before relying on it
        // (AP-001/AP-019 legitimately construct
        // `AnalysisProcedureTarget::table_scope()` with empty
        // `target_columns` and `FixedColumnName` binding_mode).
        match item.target_kind() {
            TargetKind::ColumnBound if item.target_columns().is_empty() => {
                diagnostics.push(Diagnostic::new(DiagnosticType::MissingRequiredColumn, DiagnosticSeverity::Error, Some(id), "a column-bound target must name at least one column"));
            }
            TargetKind::RoleBound if item.target_roles_count() == 0 => {
                diagnostics.push(Diagnostic::new(DiagnosticType::MissingRequiredRole, DiagnosticSeverity::Error, Some(id), "a role-bound target must declare at least one role"));
            }
            TargetKind::TableScope if !table_type_allowed => {
                diagnostics.push(Diagnostic::new(DiagnosticType::IncompatibleTargetType, DiagnosticSeverity::Warning, Some(id), "this item targets whole-table scope, but the version's compatible target types exclude Table"));
            }
            _ => {}
        }

        // invalid_expression — Error. Schema-blind syntax + function-name
        // check only: no `Schema` exists at authoring time (rulesets are
        // reusable across many Tables/Datasets — INV-006), and
        // `FormulaParser::compile` skips column-existence checking
        // entirely when given `None`, per its own doc comment. See ADR
        // 0008 for why a schema-blind check is still unambiguously
        // blocking.
        if let Some(formula) = item.expression_to_check() {
            if let Err(e) = crate::engines::transformation::formula::FormulaParser::compile(formula, None) {
                diagnostics.push(Diagnostic::new(DiagnosticType::InvalidExpression, DiagnosticSeverity::Error, Some(id), format!("expression does not parse under the safe expression grammar: {e}")));
            }
        }

        for (diagnostic_type, severity, message) in item.extra_diagnostics() {
            diagnostics.push(Diagnostic::new(diagnostic_type, severity, Some(id), message));
        }
    }

    diagnostics.extend(diagnose_dependencies(items));
    diagnostics
}

/// unresolved_dependency (Warning) / circular_dependency (Error) —
/// severities and full reasoning in ADR 0008. Trivially a no-op when
/// every item's `dependencies()` is empty (Validation, always, today).
/// `required_output` is a free-form string naming an output, not a
/// structural procedure-id edge — see `known_output_names`'s doc comment
/// for why a producer can only ever be resolved on a best-effort basis.
fn diagnose_dependencies<T: Diagnosable>(items: &[T]) -> Vec<Diagnostic> {
    let known: HashMap<Uuid, Vec<String>> = items.iter().map(|i| (i.item_id(), i.known_output_names())).collect();
    let mut edges: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    let mut diagnostics = Vec::new();

    for item in items {
        let id = item.item_id();
        for dep in item.dependencies() {
            let Some(name) = dep.required_output else { continue };
            let producers: Vec<Uuid> = known.iter().filter(|(pid, names)| **pid != id && names.contains(&name)).map(|(pid, _)| *pid).collect();
            if producers.is_empty() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticType::UnresolvedDependency,
                    DiagnosticSeverity::Warning,
                    Some(id),
                    format!("no other item in this version is statically known to produce output '{name}' — this may still resolve at runtime depending on how other items are actually configured"),
                ));
            } else {
                edges.entry(id).or_default().extend(producers);
            }
        }
    }

    if has_cycle(&edges) {
        diagnostics.push(Diagnostic::new(
            DiagnosticType::CircularDependency,
            DiagnosticSeverity::Error,
            None,
            "the declared dependencies among items in this version form a cycle — at least one side can never be satisfied, since execution is strictly ordered",
        ));
    }

    diagnostics
}

fn has_cycle(edges: &HashMap<Uuid, Vec<Uuid>>) -> bool {
    #[derive(PartialEq)]
    enum Mark {
        Visiting,
        Done,
    }

    fn visit(node: Uuid, edges: &HashMap<Uuid, Vec<Uuid>>, marks: &mut HashMap<Uuid, Mark>) -> bool {
        match marks.get(&node) {
            Some(Mark::Visiting) => return true,
            Some(Mark::Done) => return false,
            None => {}
        }
        marks.insert(node, Mark::Visiting);
        if let Some(neighbors) = edges.get(&node) {
            for &n in neighbors {
                if visit(n, edges, marks) {
                    return true;
                }
            }
        }
        marks.insert(node, Mark::Done);
        false
    }

    let mut marks: HashMap<Uuid, Mark> = HashMap::new();
    edges.keys().any(|&k| !matches!(marks.get(&k), Some(Mark::Done)) && visit(k, edges, &mut marks))
}

/// unsupported_catalogue_version — Error; see ADR 0008. `supported` is a
/// small, explicit allowlist each engine maintains itself (see
/// `validation_ruleset.rs`/`analysis_ruleset.rs`), matching the literal
/// version strings this codebase's own real fixtures already use.
pub fn diagnose_catalogue_version(field_name: &str, actual: &str, supported: &[&str]) -> Option<Diagnostic> {
    if supported.contains(&actual) {
        None
    } else {
        Some(Diagnostic::new(DiagnosticType::UnsupportedCatalogueVersion, DiagnosticSeverity::Error, None, format!("{field_name} '{actual}' is not among the versions this engine currently recognizes ({supported:?})")))
    }
}
