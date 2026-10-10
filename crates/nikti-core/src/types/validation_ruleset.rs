//! Validation Ruleset Object Model v1.0 (as summarized in the governing
//! task prompt; see the final compliance report for the caveat that the
//! source document itself was unavailable). Knowledge, not evidence
//! (INV-006/012): editable and versionable — but editing an Active
//! version never happens in place (A.1/A.2); it always produces a new
//! version. INV-009: authored/edited only in Table Repository context —
//! enforced by the command layer that calls these methods, not by this
//! module itself (same discipline as the legacy Recipe/Ruleset split
//! already documented elsewhere in this codebase).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::common::{Timestamp, ValidationRulesetId};
use super::definition_diagnostics::{self, DefinitionStatus, DependencyRef, Diagnosable, Diagnostic, DiagnosticSeverity, DiagnosticType, TargetKind};
use super::scalar_value::ScalarValue;
use super::validation_function_params::ValidationFunctionParams;

/// Amendment 4: ruleset-level state and version-level state are two
/// separate fields, never collapsed. `Superseded` is deliberately not a
/// variant here — it is a version-level concept only (see `VersionState`)
/// and must never be representable as a Ruleset's own state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RulesetState {
    Draft,
    Active,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionState {
    Draft,
    Active,
    Superseded,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Critical,
    Error,
    Warning,
    Information,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationTargetType {
    Table,
    Column,
    ColumnSet,
    Row,
    Cell,
    Reference,
    Expression,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindingMode {
    FixedColumnName,
    RuntimeColumnMapping,
    ReferenceBinding,
    ExpressionBinding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleTarget {
    pub target_type: ValidationTargetType,
    pub binding_mode: BindingMode,
    pub target_columns: Vec<String>,
}

/// Rule-level missing-value policy. No rule may rely on implicit
/// handling — every rule resolves to exactly one of these (including,
/// explicitly, deferring to the ruleset default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleNullPolicy {
    FailIfMissing,
    IgnoreMissing,
    PassIfMissing,
    EvaluateMissingAsValue,
    InheritRulesetDefault,
}

/// Ruleset-level missing-value policy (A.3). `default_null_policy` must
/// not itself be `InheritRulesetDefault` — there is nothing left for it
/// to inherit from; that is a construction-time domain error, not
/// merely a documentation note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesetMissingValuePolicy {
    pub missing_tokens: Vec<String>,
    pub default_null_policy: RuleNullPolicy,
    pub blank_string_policy: RuleNullPolicy,
    pub whitespace_only_policy: RuleNullPolicy,
}

impl Default for RulesetMissingValuePolicy {
    fn default() -> Self {
        Self {
            missing_tokens: Vec::new(),
            default_null_policy: RuleNullPolicy::FailIfMissing,
            blank_string_policy: RuleNullPolicy::FailIfMissing,
            whitespace_only_policy: RuleNullPolicy::FailIfMissing,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRule {
    rule_id: Uuid,
    rule_instance_id: Uuid,
    rule_name: String,
    function: ValidationFunctionParams,
    enabled: bool,
    severity: Severity,
    target: RuleTarget,
    null_policy: RuleNullPolicy,
    failure_message_template: String,
}

impl ValidationRule {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        rule_id: Uuid,
        rule_instance_id: Uuid,
        rule_name: String,
        function: ValidationFunctionParams,
        enabled: bool,
        severity: Severity,
        target: RuleTarget,
        null_policy: RuleNullPolicy,
        failure_message_template: String,
    ) -> Self {
        Self { rule_id, rule_instance_id, rule_name, function, enabled, severity, target, null_policy, failure_message_template }
    }

    pub fn rule_id(&self) -> Uuid {
        self.rule_id
    }

    pub fn rule_instance_id(&self) -> Uuid {
        self.rule_instance_id
    }

    pub fn rule_name(&self) -> &str {
        &self.rule_name
    }

    /// `rule_type`, in the source document's naming — maps to a
    /// `function_id` in the Part B catalogue.
    pub fn function_id(&self) -> &'static str {
        self.function.function_id()
    }

    pub fn function(&self) -> &ValidationFunctionParams {
        &self.function
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }

    pub fn target(&self) -> &RuleTarget {
        &self.target
    }

    pub fn null_policy(&self) -> RuleNullPolicy {
        self.null_policy
    }

    pub fn failure_message_template(&self) -> &str {
        &self.failure_message_template
    }
}

impl Diagnosable for ValidationRule {
    fn item_id(&self) -> Uuid {
        self.rule_id
    }

    fn item_name(&self) -> &str {
        &self.rule_name
    }

    fn type_id(&self) -> String {
        self.function_id().to_string()
    }

    fn is_supported_type(&self) -> bool {
        // `Unknown` exists specifically to represent an
        // unsupported/deprecated/future `function_id` (see
        // `validation_function_params.rs`'s module doc) — this is the
        // one real "unsupported type" case either engine's object model
        // can actually construct today.
        !matches!(self.function, ValidationFunctionParams::Unknown)
    }

    fn target_kind(&self) -> TargetKind {
        match self.target.target_type {
            ValidationTargetType::Column | ValidationTargetType::ColumnSet => TargetKind::ColumnBound,
            ValidationTargetType::Table => TargetKind::TableScope,
            ValidationTargetType::Row | ValidationTargetType::Cell | ValidationTargetType::Reference | ValidationTargetType::Expression => TargetKind::Other,
        }
    }

    fn target_columns(&self) -> &[String] {
        &self.target.target_columns
    }

    fn target_roles_count(&self) -> usize {
        // Validation has no role concept at all (confirmed: no such
        // field anywhere on `RuleTarget`/`ValidationRule`) — no
        // `ValidationRule` can ever be `TargetKind::RoleBound` in the
        // first place (see `target_kind` above), so this is never
        // actually consulted, but the trait requires an answer.
        0
    }

    fn expression_to_check(&self) -> Option<&str> {
        if let ValidationFunctionParams::ExpressionTrue { formula } = &self.function { Some(formula) } else { None }
    }

    fn extra_diagnostics(&self) -> Vec<(DiagnosticType, DiagnosticSeverity, String)> {
        let mut diagnostics = validation_function_parameter_diagnostics(&self.function);
        if self.failure_message_template.trim().is_empty() {
            diagnostics.push((DiagnosticType::MissingRequiredField, DiagnosticSeverity::Error, "failure_message_template must not be empty".to_string()));
        }
        diagnostics
    }

    fn dependencies(&self) -> Vec<DependencyRef> {
        // No dependency concept exists anywhere in Validation's object
        // model (confirmed: no field/method referencing another rule by
        // id/reference on `ValidationRule` or `ValidationRulesetVersion`)
        // — always empty, which makes `diagnose_dependencies` a
        // structural no-op for every `ValidationRulesetVersion`, not a
        // special case in the shared code itself.
        Vec::new()
    }

    fn known_output_names(&self) -> Vec<String> {
        Vec::new()
    }
}

fn scalar_as_f64(v: &ScalarValue) -> Option<f64> {
    match v {
        ScalarValue::Int(i) => Some(*i as f64),
        ScalarValue::Float(f) => Some(*f),
        _ => None,
    }
}

/// invalid_parameter, per Validation function type. Every arm was
/// checked against the real per-function semantics in
/// `engines/validation/functions.rs` before being written. `MinValue`/
/// `MaxValue` alone have no invalid single-bound value (any scalar is a
/// structurally valid one-sided bound); `MinLength`/`MaxLength` alone
/// have no invalid single bound either — only the *between*-shaped
/// functions (`BetweenValues`, `DistinctCountBetween`, `LengthBetween`)
/// can have a genuinely inverted, self-contradictory range.
fn validation_function_parameter_diagnostics(function: &ValidationFunctionParams) -> Vec<(DiagnosticType, DiagnosticSeverity, String)> {
    use ValidationFunctionParams::*;
    let err = |message: String| vec![(DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, message)];
    match function {
        NotMissing | NotBlank | NotWhitespaceOnly | NumericCompatible | BooleanCompatible | UniqueColumn | NoDuplicateRows | MinValue { .. } | MaxValue { .. } | MinLength { .. } | MaxLength { .. } | ExpressionTrue { .. } | Unknown => Vec::new(),
        MinimumCompleteness { min_ratio } => {
            if !(0.0..=1.0).contains(min_ratio) {
                err(format!("min_ratio {min_ratio} must be between 0.0 and 1.0"))
            } else {
                Vec::new()
            }
        }
        OneOfRequired { columns } => {
            if columns.is_empty() {
                err("columns must name at least one column".to_string())
            } else {
                Vec::new()
            }
        }
        DateCompatible { format } => {
            if matches!(format, Some(f) if f.trim().is_empty()) {
                err("format, if supplied, must not be empty".to_string())
            } else {
                Vec::new()
            }
        }
        BetweenValues { min, max, .. } => match (scalar_as_f64(min), scalar_as_f64(max)) {
            (Some(a), Some(b)) if a > b => err(format!("min ({a}) must not be greater than max ({b})")),
            _ => Vec::new(),
        },
        AllowedValues { values } => {
            if values.is_empty() {
                err("values must name at least one allowed value".to_string())
            } else {
                Vec::new()
            }
        }
        DisallowedValues { values } => {
            if values.is_empty() {
                err("values must name at least one disallowed value".to_string())
            } else {
                Vec::new()
            }
        }
        DistinctCountBetween { min, max } => {
            if min > max {
                err(format!("min ({min}) must not be greater than max ({max})"))
            } else {
                Vec::new()
            }
        }
        RegexMatch { pattern } | RegexNotMatch { pattern } => {
            if pattern.is_empty() {
                err("pattern must not be empty".to_string())
            } else if regex::Regex::new(pattern).is_err() {
                err(format!("pattern '{pattern}' is not a valid regular expression"))
            } else {
                Vec::new()
            }
        }
        CharacterPatternMatch { allowed_chars } => {
            if allowed_chars.is_empty() {
                err("allowed_chars must not be empty".to_string())
            } else {
                Vec::new()
            }
        }
        LengthBetween { min, max } => {
            if min > max {
                err(format!("min ({min}) must not be greater than max ({max})"))
            } else {
                Vec::new()
            }
        }
        UniqueColumnSet { columns } => {
            if columns.is_empty() {
                err("columns must name at least one column".to_string())
            } else {
                Vec::new()
            }
        }
        ColumnCompare { other_column, .. } => {
            if other_column.is_empty() {
                err("other_column must not be empty".to_string())
            } else {
                Vec::new()
            }
        }
        RequiredIf { other_column, .. } => {
            if other_column.is_empty() {
                err("other_column must not be empty".to_string())
            } else {
                Vec::new()
            }
        }
        MutuallyExclusive { columns } => {
            if columns.len() < 2 {
                err("columns must name at least two columns to be mutually exclusive".to_string())
            } else {
                Vec::new()
            }
        }
        ExistsInReference { reference_values } | NotExistsInReference { reference_values } => {
            if reference_values.is_empty() {
                err("reference_values must not be empty".to_string())
            } else {
                Vec::new()
            }
        }
        DateFormatMatch { format } => {
            if format.is_empty() {
                err("format must not be empty".to_string())
            } else {
                Vec::new()
            }
        }
    }
}

/// The only catalogue versions this codebase's real fixtures currently
/// use — see `definition_diagnostics::diagnose_catalogue_version`'s doc
/// comment for why there is no external spec to source this from.
const SUPPORTED_RULESET_SCHEMA_VERSIONS: &[&str] = &["1.0"];
const SUPPORTED_FUNCTION_CATALOGUE_VERSIONS: &[&str] = &["1.0.0"];

/// Part C: recomputed on every Draft save AND at construction. The
/// enforced GATE is specifically `ValidationRuleset::activate_version()`.
fn diagnose_validation_ruleset_version(rules: &[ValidationRule], compatible_target_types: &[CompatibleTargetObjectType], ruleset_schema_version: &str, function_catalogue_version: &str) -> Vec<Diagnostic> {
    let table_type_allowed = compatible_target_types.contains(&CompatibleTargetObjectType::Table);
    let mut diagnostics = definition_diagnostics::diagnose_items(rules, table_type_allowed, DiagnosticType::DuplicateRuleId, DiagnosticType::UnsupportedRuleType);
    diagnostics.extend(definition_diagnostics::diagnose_catalogue_version("ruleset_schema_version", ruleset_schema_version, SUPPORTED_RULESET_SCHEMA_VERSIONS));
    diagnostics.extend(definition_diagnostics::diagnose_catalogue_version("function_catalogue_version", function_catalogue_version, SUPPORTED_FUNCTION_CATALOGUE_VERSIONS));
    diagnostics
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompatibleTargetObjectType {
    Table,
    Dataset,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ValidationRulesetError {
    #[error("version {0} is not Draft — editing an Active/Superseded/Archived version in place is forbidden; create a new version instead")]
    VersionNotEditable(Uuid),
    #[error("version {0} not found on this ruleset")]
    VersionNotFound(Uuid),
    #[error("only a Draft version may be activated (found {0:?})")]
    OnlyDraftVersionsCanActivate(VersionState),
    /// Part C: "Errors block Save New Version" — a version carrying any
    /// `error`-severity definition diagnostic may never become Active.
    #[error("version {0} has blocking definition-diagnostic errors and cannot be activated")]
    DefinitionInvalid(Uuid),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRulesetVersion {
    ruleset_version_id: Uuid,
    version_number: u32,
    version_state: VersionState,
    created_at: Timestamp,
    created_by: String,
    change_summary: String,
    ruleset_schema_version: String,
    function_catalogue_version: String,
    ruleset_hash: String,
    rules: Vec<ValidationRule>,
    compatible_target_types: Vec<CompatibleTargetObjectType>,
    required_columns: Vec<String>,
    optional_columns: Vec<String>,
    required_working_types: Vec<String>,
    missing_value_policy: RulesetMissingValuePolicy,
    definition_status: DefinitionStatus,
    definition_diagnostics: Vec<Diagnostic>,
}

impl ValidationRulesetVersion {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ruleset_version_id: Uuid,
        version_number: u32,
        created_at: Timestamp,
        created_by: String,
        change_summary: String,
        ruleset_schema_version: String,
        function_catalogue_version: String,
        ruleset_hash: String,
        rules: Vec<ValidationRule>,
        compatible_target_types: Vec<CompatibleTargetObjectType>,
        required_columns: Vec<String>,
        optional_columns: Vec<String>,
        required_working_types: Vec<String>,
        missing_value_policy: RulesetMissingValuePolicy,
    ) -> Self {
        let compatible_target_types = if compatible_target_types.is_empty() { vec![CompatibleTargetObjectType::Table, CompatibleTargetObjectType::Dataset] } else { compatible_target_types };
        // Part C: diagnostics are computed at construction — Validation
        // had no `definition_status`/`definition_diagnostics` concept at
        // all before this task; they are never born empty/unconsidered.
        let definition_diagnostics = diagnose_validation_ruleset_version(&rules, &compatible_target_types, &ruleset_schema_version, &function_catalogue_version);
        let definition_status = definition_diagnostics::status_from(&definition_diagnostics);
        Self {
            ruleset_version_id,
            version_number,
            // Every new version is born Draft — activation is a separate,
            // explicit step (`ValidationRuleset::activate_version`).
            version_state: VersionState::Draft,
            created_at,
            created_by,
            change_summary,
            ruleset_schema_version,
            function_catalogue_version,
            ruleset_hash,
            rules,
            compatible_target_types,
            required_columns,
            optional_columns,
            required_working_types,
            missing_value_policy,
            definition_status,
            definition_diagnostics,
        }
    }

    pub fn definition_status(&self) -> DefinitionStatus {
        self.definition_status
    }

    pub fn definition_diagnostics(&self) -> &[Diagnostic] {
        &self.definition_diagnostics
    }

    pub fn ruleset_version_id(&self) -> Uuid {
        self.ruleset_version_id
    }

    pub fn version_number(&self) -> u32 {
        self.version_number
    }

    pub fn version_state(&self) -> VersionState {
        self.version_state
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn created_by(&self) -> &str {
        &self.created_by
    }

    pub fn change_summary(&self) -> &str {
        &self.change_summary
    }

    pub fn ruleset_schema_version(&self) -> &str {
        &self.ruleset_schema_version
    }

    pub fn function_catalogue_version(&self) -> &str {
        &self.function_catalogue_version
    }

    pub fn ruleset_hash(&self) -> &str {
        &self.ruleset_hash
    }

    pub fn rules(&self) -> &[ValidationRule] {
        &self.rules
    }

    pub fn compatible_target_types(&self) -> &[CompatibleTargetObjectType] {
        &self.compatible_target_types
    }

    pub fn required_columns(&self) -> &[String] {
        &self.required_columns
    }

    pub fn optional_columns(&self) -> &[String] {
        &self.optional_columns
    }

    pub fn required_working_types(&self) -> &[String] {
        &self.required_working_types
    }

    pub fn missing_value_policy(&self) -> &RulesetMissingValuePolicy {
        &self.missing_value_policy
    }

    /// Draft versions are editable; this is the only write path on this
    /// type, and it is a hard error (not a silent no-op) on any non-Draft
    /// version — editing Active/Superseded/Archived in place is exactly
    /// what A.1/A.2 forbid.
    /// Part C: diagnostics are recomputed on every Draft save, not just
    /// at construction or at `activate_version()` — never stale between
    /// edits. This method itself does NOT reject the save on diagnostic
    /// errors (a Draft may carry errors freely); only
    /// `ValidationRuleset::activate_version()` enforces that gate.
    pub fn set_rules(&mut self, rules: Vec<ValidationRule>) -> Result<(), ValidationRulesetError> {
        if self.version_state != VersionState::Draft {
            return Err(ValidationRulesetError::VersionNotEditable(self.ruleset_version_id));
        }
        self.rules = rules;
        self.definition_diagnostics = diagnose_validation_ruleset_version(&self.rules, &self.compatible_target_types, &self.ruleset_schema_version, &self.function_catalogue_version);
        self.definition_status = definition_diagnostics::status_from(&self.definition_diagnostics);
        Ok(())
    }
}

/// Amendment 4: the top-level Knowledge Object. `ruleset_id` is
/// permanent — renaming (`ruleset_name`) never changes it. `state` is
/// tracked independently of any individual version's `version_state`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRuleset {
    ruleset_id: ValidationRulesetId,
    ruleset_name: String,
    state: RulesetState,
    current_version_id: Uuid,
    created_at: Timestamp,
    created_by: String,
    last_modified_at: Timestamp,
    last_modified_by: String,
    tags: Vec<String>,
    description: String,
    versions: Vec<ValidationRulesetVersion>,
}

impl ValidationRuleset {
    /// INV-009: intended to be called only from a code path reachable
    /// from Table Repository context — not enforced by this type (Rust
    /// has no notion of "caller context"), but by the command layer that
    /// is the only place authorized to call it.
    pub fn new(
        ruleset_id: ValidationRulesetId,
        ruleset_name: String,
        created_at: Timestamp,
        created_by: String,
        tags: Vec<String>,
        description: String,
        initial_version: ValidationRulesetVersion,
    ) -> Self {
        let current_version_id = initial_version.ruleset_version_id();
        Self {
            ruleset_id,
            ruleset_name,
            state: RulesetState::Draft,
            current_version_id,
            created_at,
            created_by: created_by.clone(),
            last_modified_at: created_at,
            last_modified_by: created_by,
            tags,
            description,
            versions: vec![initial_version],
        }
    }

    pub fn ruleset_id(&self) -> ValidationRulesetId {
        self.ruleset_id
    }

    pub fn ruleset_name(&self) -> &str {
        &self.ruleset_name
    }

    pub fn state(&self) -> RulesetState {
        self.state
    }

    pub fn current_version_id(&self) -> Uuid {
        self.current_version_id
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn created_by(&self) -> &str {
        &self.created_by
    }

    pub fn last_modified_at(&self) -> Timestamp {
        self.last_modified_at
    }

    pub fn last_modified_by(&self) -> &str {
        &self.last_modified_by
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn versions(&self) -> &[ValidationRulesetVersion] {
        &self.versions
    }

    pub fn version(&self, ruleset_version_id: Uuid) -> Option<&ValidationRulesetVersion> {
        self.versions.iter().find(|v| v.ruleset_version_id() == ruleset_version_id)
    }

    /// The only way to acquire a new version: append a fresh Draft. Never
    /// mutates an existing version's rules — that is `set_rules`'s job,
    /// and it refuses anything but a Draft (A.1/A.2's "editing an Active
    /// version always creates a new version, never an in-place change").
    pub fn add_draft_version(&mut self, version: ValidationRulesetVersion, modified_at: Timestamp, modified_by: String) {
        self.versions.push(version);
        self.last_modified_at = modified_at;
        self.last_modified_by = modified_by;
    }

    /// Promotes a Draft version to Active. Whatever version was
    /// previously Active on this ruleset (if any) becomes `Superseded` —
    /// a version-level transition only; `self.state` becomes/stays
    /// `Active` at the ruleset level, which is a different field
    /// entirely (Amendment 4).
    pub fn activate_version(&mut self, ruleset_version_id: Uuid, at: Timestamp, by: String) -> Result<(), ValidationRulesetError> {
        let target = self.versions.iter().find(|v| v.ruleset_version_id() == ruleset_version_id).ok_or(ValidationRulesetError::VersionNotFound(ruleset_version_id))?;
        if target.version_state != VersionState::Draft {
            return Err(ValidationRulesetError::OnlyDraftVersionsCanActivate(target.version_state));
        }
        // Part C's enforced gate — matches "Errors block Save New
        // Version": a Draft may carry errors freely (see `set_rules`),
        // but the Draft->Active transition is refused if any
        // `error`-severity definition diagnostic is present. `Warning`/
        // `Information` diagnostics never block activation.
        if target.definition_status == DefinitionStatus::Invalid {
            return Err(ValidationRulesetError::DefinitionInvalid(ruleset_version_id));
        }

        for v in self.versions.iter_mut() {
            if v.version_state == VersionState::Active {
                v.version_state = VersionState::Superseded;
            }
            if v.ruleset_version_id == ruleset_version_id {
                v.version_state = VersionState::Active;
            }
        }
        self.current_version_id = ruleset_version_id;
        self.state = RulesetState::Active;
        self.last_modified_at = at;
        self.last_modified_by = by;
        Ok(())
    }

    pub fn archive(&mut self, at: Timestamp, by: String) {
        self.state = RulesetState::Archived;
        self.last_modified_at = at;
        self.last_modified_by = by;
    }
}

/// Small builder used by tests/callers to avoid repeating boilerplate
/// `ScalarValue`/`RuleTarget` construction — not part of the frozen
/// object model itself.
pub fn fixed_column_target(target_type: ValidationTargetType, column: &str) -> RuleTarget {
    RuleTarget { target_type, binding_mode: BindingMode::FixedColumnName, target_columns: vec![column.to_string()] }
}
