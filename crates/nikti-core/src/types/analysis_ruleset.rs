//! Analysis Ruleset Object Model v1.0 (as summarized in the governing
//! task prompt; see the final compliance report for the source-document
//! caveat). Same Knowledge Object discipline as
//! `validation_ruleset.rs` (editable/versionable, INV-006/012;
//! authored/edited only in Table Repository context, INV-009) — but
//! Analysis's own vocabulary throughout: **Analysis Procedure**, never
//! "Analysis Rule" (Part A); no `severity` field anywhere (B.6);
//! `role_based_selection` binding mode, not Validation's
//! `reference_binding` (B.5); AP-001 mandatory and un-disableable (B.7,
//! Amendment 5).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::analysis_procedure_params::AnalysisProcedureParams;
use super::common::{AnalysisRulesetId, Timestamp};
use super::definition_diagnostics::{self, DefinitionStatus, DependencyRef, Diagnosable, Diagnostic, DiagnosticSeverity, DiagnosticType, TargetKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalysisRulesetState {
    Draft,
    Active,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalysisVersionState {
    Draft,
    Active,
    Superseded,
    Archived,
}

/// B.5: Analysis's own role vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VariableRole {
    Identifier,
    Measure,
    Dimension,
    DateTime,
    Text,
    Target,
    Weight,
    ReferenceKey,
    Ignore,
}

/// B.5: **not** Validation's binding-mode set — `RoleBasedSelection`
/// here where Validation has `ReferenceBinding`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalysisBindingMode {
    FixedColumnName,
    RuntimeColumnMapping,
    RoleBasedSelection,
    ExpressionBinding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalysisTargetType {
    Column,
    ColumnSet,
    RoleSet,
    Expression,
    TableScope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisProcedureTarget {
    pub target_type: AnalysisTargetType,
    pub binding_mode: AnalysisBindingMode,
    pub target_columns: Vec<String>,
    pub target_roles: Vec<VariableRole>,
    pub target_expression: Option<String>,
}

impl AnalysisProcedureTarget {
    pub fn columns(columns: &[&str]) -> Self {
        Self {
            target_type: AnalysisTargetType::ColumnSet,
            binding_mode: AnalysisBindingMode::FixedColumnName,
            target_columns: columns.iter().map(|s| s.to_string()).collect(),
            target_roles: Vec::new(),
            target_expression: None,
        }
    }

    pub fn table_scope() -> Self {
        Self {
            target_type: AnalysisTargetType::TableScope,
            binding_mode: AnalysisBindingMode::FixedColumnName,
            target_columns: Vec::new(),
            target_roles: Vec::new(),
            target_expression: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetailLevel {
    Summary,
    Standard,
    Detailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputOptions {
    pub detail_level: DetailLevel,
    pub include_visual_outputs: bool,
    pub include_detail_refs: bool,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self { detail_level: DetailLevel::Standard, include_visual_outputs: true, include_detail_refs: false }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DependencyType {
    RequiresRole,
    RequiresColumn,
    RequiresDerivedMeasure,
    RequiresOutput,
    RequiresScope,
    RequiresTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnMissingDependency {
    NotExecutable,
    SkipWithWarning,
    ExecutionError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcedureDependency {
    pub dependency_type: DependencyType,
    pub required_output: Option<String>,
    pub on_missing: OnMissingDependency,
}

/// B.6: no `severity` field, deliberately — Analysis measures, it does
/// not judge conformance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisProcedure {
    procedure_id: Uuid,
    procedure_name: String,
    params: AnalysisProcedureParams,
    enabled: bool,
    display_order: u32,
    target: AnalysisProcedureTarget,
    output_options: OutputOptions,
    dependency_refs: Vec<ProcedureDependency>,
    description: String,
    tags: Vec<String>,
}

impl AnalysisProcedure {
    pub fn new(
        procedure_id: Uuid,
        procedure_name: String,
        params: AnalysisProcedureParams,
        enabled: bool,
        display_order: u32,
        target: AnalysisProcedureTarget,
        output_options: OutputOptions,
        dependency_refs: Vec<ProcedureDependency>,
        description: String,
        tags: Vec<String>,
    ) -> Self {
        Self { procedure_id, procedure_name, params, enabled, display_order, target, output_options, dependency_refs, description, tags }
    }

    /// AP-001's fixed, always-present, always-enabled entry — the only
    /// way this codebase constructs one. Its `description`/`target` may
    /// be customized by the caller (B.7: "It may be edited... its
    /// parameters"); `enabled` is not a parameter here at all — there is
    /// no way to construct a disabled AP-001 through this function.
    pub fn mandatory_scope_setup(procedure_id: Uuid, description: String, target: AnalysisProcedureTarget) -> Self {
        Self {
            procedure_id,
            procedure_name: "Scope & Variable Role Setup".to_string(),
            params: AnalysisProcedureParams::ScopeAndVariableRoleSetup,
            enabled: true,
            display_order: 0,
            target,
            output_options: OutputOptions::default(),
            dependency_refs: Vec::new(),
            description,
            tags: Vec::new(),
        }
    }

    pub fn procedure_id(&self) -> Uuid {
        self.procedure_id
    }

    pub fn procedure_name(&self) -> &str {
        &self.procedure_name
    }

    pub fn procedure_type(&self) -> &'static str {
        self.params.procedure_type()
    }

    pub fn params(&self) -> &AnalysisProcedureParams {
        &self.params
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn display_order(&self) -> u32 {
        self.display_order
    }

    pub fn target(&self) -> &AnalysisProcedureTarget {
        &self.target
    }

    pub fn output_options(&self) -> &OutputOptions {
        &self.output_options
    }

    pub fn dependency_refs(&self) -> &[ProcedureDependency] {
        &self.dependency_refs
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }
}

impl Diagnosable for AnalysisProcedure {
    fn item_id(&self) -> Uuid {
        self.procedure_id
    }

    fn item_name(&self) -> &str {
        &self.procedure_name
    }

    fn type_id(&self) -> String {
        self.procedure_type().to_string()
    }

    fn is_supported_type(&self) -> bool {
        // `AnalysisProcedureParams` is an exhaustive, closed enum with no
        // catch-all "unsupported" variant (unlike Validation's `Unknown`)
        // — every constructible value is, by definition, one of the 19
        // frozen procedure types. Always `true` today; kept as a real
        // check (not hardcoded `true` inline at the call site) so this
        // stays correct automatically if a catch-all is ever added.
        true
    }

    fn target_kind(&self) -> TargetKind {
        match self.target.target_type {
            AnalysisTargetType::Column | AnalysisTargetType::ColumnSet => TargetKind::ColumnBound,
            AnalysisTargetType::RoleSet => TargetKind::RoleBound,
            AnalysisTargetType::TableScope => TargetKind::TableScope,
            AnalysisTargetType::Expression => TargetKind::Other,
        }
    }

    fn target_columns(&self) -> &[String] {
        &self.target.target_columns
    }

    fn target_roles_count(&self) -> usize {
        self.target.target_roles.len()
    }

    fn expression_to_check(&self) -> Option<&str> {
        if let AnalysisProcedureParams::DerivedMeasureDefinition { formula, .. } = &self.params { Some(formula) } else { None }
    }

    fn extra_diagnostics(&self) -> Vec<(DiagnosticType, DiagnosticSeverity, String)> {
        analysis_procedure_parameter_diagnostics(&self.params)
    }

    fn dependencies(&self) -> Vec<DependencyRef> {
        self.dependency_refs.iter().map(|d| DependencyRef { required_output: d.required_output.clone() }).collect()
    }

    fn known_output_names(&self) -> Vec<String> {
        known_static_outputs(self.procedure_type()).iter().map(|s| s.to_string()).collect()
    }
}

/// The UNCONDITIONAL output table names each procedure type is known to
/// produce, per the real implementations in `engines/analysis/procedures.rs`
/// — kept manually in sync with that file (see `Diagnosable::known_output_names`'s
/// doc comment on the shared trait for why this can only ever be a
/// best-effort list, and why `unresolved_dependency` is a Warning, not an
/// Error, as a direct consequence). Deliberately excludes every
/// conditionally-produced table: AP-012's `regex_measure_table`,
/// AP-013's `duration_summary_table`, AP-016's `target_by_segment_table`.
fn known_static_outputs(procedure_type: &str) -> &'static [&'static str] {
    match procedure_type {
        "AP-001" => &["scope_summary_table"],
        "AP-002" => &["derived_measure_summary_table"],
        "AP-003" => &["column_property_table"],
        "AP-004" => &["group_summary_table", "group_rank_table"],
        "AP-005" => &["segment_comparison_table", "difference_table"],
        "AP-006" => &["crosstab_count_table", "crosstab_percentage_table", "association_summary_table"],
        "AP-007" => &["category_numeric_summary_table", "category_distribution_table"],
        "AP-008" => &["pairwise_numeric_coefficient_table", "top_numeric_pairs_table"],
        "AP-009" => &["distribution_summary_table", "histogram_table", "boundary_candidate_table"],
        "AP-010" => &["missingness_summary_table", "missingness_pattern_table", "missingness_cooccurrence_table"],
        "AP-011" => &["duplicate_summary_table", "duplicate_group_table"],
        "AP-012" => &["pattern_frequency_table"],
        "AP-013" => &["date_summary_table", "time_bucket_table"],
        "AP-014" => &["pareto_table", "concentration_summary_table"],
        "AP-015" => &["text_metric_table", "length_distribution_table", "keyword_frequency_table"],
        "AP-016" => &["target_profile_table", "target_coverage_table"],
        "AP-017" => &["association_ranking_table", "association_detail_table"],
        "AP-018" => &["dependency_candidate_table", "composite_uniqueness_table"],
        "AP-019" => &["feature_retention_evidence_table", "row_attention_table"],
        _ => &[],
    }
}

/// invalid_parameter, per Analysis procedure type. Every arm was checked
/// against that procedure's real implementation in
/// `engines/analysis/procedures.rs` before being written — several
/// "empty list" cases are deliberately NOT flagged here because the real
/// implementation treats an empty list as "use all columns" (a genuine
/// feature, not an omission): `ColumnPropertyExplorer`,
/// `MissingnessStructureExplorer`, `RepetitionDuplicateStructureExplorer`,
/// and `DependencyCandidateExplorer` all do this; `TextMeasurementExplorer`'s
/// empty `keywords` is likewise a legitimate "no keyword search" case,
/// confirmed against its own real tests.
fn analysis_procedure_parameter_diagnostics(params: &AnalysisProcedureParams) -> Vec<(DiagnosticType, DiagnosticSeverity, String)> {
    use AnalysisProcedureParams::*;
    let err = |message: String| vec![(DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, message)];
    match params {
        ScopeAndVariableRoleSetup | FeatureAndRowAttentionScreen => Vec::new(),
        ColumnPropertyExplorer { .. } | MissingnessStructureExplorer { .. } | RepetitionDuplicateStructureExplorer { .. } | DependencyCandidateExplorer { .. } => Vec::new(),
        DerivedMeasureDefinition { measure_name, .. } => {
            if measure_name.trim().is_empty() {
                err("measure_name must not be empty".to_string())
            } else {
                Vec::new()
            }
        }
        GroupedSummary { group_by, measure_column, .. } => {
            let mut v = Vec::new();
            if group_by.is_empty() {
                v.push((DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, "group_by must name at least one column".to_string()));
            }
            if measure_column.trim().is_empty() {
                v.push((DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, "measure_column must not be empty".to_string()));
            }
            v
        }
        SegmentSubsetComparison { segment_column, measure_column, .. } => {
            let mut v = Vec::new();
            if segment_column.trim().is_empty() {
                v.push((DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, "segment_column must not be empty".to_string()));
            }
            if measure_column.trim().is_empty() {
                v.push((DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, "measure_column must not be empty".to_string()));
            }
            v
        }
        CrossTabulation { row_column, column_column } => {
            let mut v = Vec::new();
            if row_column.trim().is_empty() {
                v.push((DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, "row_column must not be empty".to_string()));
            }
            if column_column.trim().is_empty() {
                v.push((DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, "column_column must not be empty".to_string()));
            }
            v
        }
        CategoryToNumericComparison { category_column, numeric_column } => {
            let mut v = Vec::new();
            if category_column.trim().is_empty() {
                v.push((DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, "category_column must not be empty".to_string()));
            }
            if numeric_column.trim().is_empty() {
                v.push((DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, "numeric_column must not be empty".to_string()));
            }
            v
        }
        NumericRelationshipAssociation { columns, .. } => {
            if columns.len() < 2 {
                err("columns must name at least two columns to compute pairwise coefficients".to_string())
            } else {
                Vec::new()
            }
        }
        DistributionBoundaryExplorer { column, .. } => {
            if column.trim().is_empty() {
                err("column must not be empty".to_string())
            } else {
                Vec::new()
            }
        }
        PatternFormatStructureExplorer { column, regex_list } => {
            let mut v = Vec::new();
            if column.trim().is_empty() {
                v.push((DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, "column must not be empty".to_string()));
            }
            if let Some(list) = regex_list {
                if list.is_empty() {
                    v.push((DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, "regex_list, if supplied, must not be empty — omit it (None) instead if there is no regex to configure".to_string()));
                }
                for pattern in list {
                    if regex::Regex::new(pattern).is_err() {
                        v.push((DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, format!("regex_list entry '{pattern}' is not a valid regular expression")));
                    }
                }
            }
            v
        }
        DateTimeTrendExplorer { date_column, duration_start_column, duration_end_column, .. } => {
            let mut v = Vec::new();
            if date_column.trim().is_empty() {
                v.push((DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, "date_column must not be empty".to_string()));
            }
            if duration_start_column.is_some() != duration_end_column.is_some() {
                v.push((DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, "duration_start_column and duration_end_column must be supplied together or not at all".to_string()));
            }
            v
        }
        ParetoConcentrationExplorer { category_column, measure_column } => {
            let mut v = Vec::new();
            if category_column.trim().is_empty() {
                v.push((DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, "category_column must not be empty".to_string()));
            }
            if measure_column.trim().is_empty() {
                v.push((DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, "measure_column must not be empty".to_string()));
            }
            v
        }
        TextMeasurementExplorer { column, .. } => {
            if column.trim().is_empty() {
                err("column must not be empty".to_string())
            } else {
                Vec::new()
            }
        }
        TargetLens { target_column, segment_columns } => {
            let mut v = Vec::new();
            if target_column.trim().is_empty() {
                v.push((DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, "target_column must not be empty".to_string()));
            }
            if let Some(list) = segment_columns {
                if list.is_empty() {
                    v.push((DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, "segment_columns, if supplied, must not be empty — omit it (None) instead if there is no segment to configure".to_string()));
                }
            }
            v
        }
        AssociationStrengthRanking { target_column, candidate_columns } => {
            let mut v = Vec::new();
            if target_column.trim().is_empty() {
                v.push((DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, "target_column must not be empty".to_string()));
            }
            if candidate_columns.is_empty() {
                v.push((DiagnosticType::InvalidParameter, DiagnosticSeverity::Error, "candidate_columns must name at least one column".to_string()));
            }
            v
        }
    }
}

/// The only catalogue versions this codebase's real fixtures currently
/// use — there being no external spec to source a "supported" list from
/// otherwise (see `definition_diagnostics::diagnose_catalogue_version`'s
/// doc comment).
const SUPPORTED_RULESET_SCHEMA_VERSIONS: &[&str] = &["1.0"];
const SUPPORTED_ANALYSIS_PROCEDURE_CATALOGUE_VERSIONS: &[&str] = &["1.0.0"];
const SUPPORTED_ANALYSIS_ENGINE_MIN_VERSIONS: &[&str] = &["0.1.0"];

/// Part C: recomputed on every Draft save AND at construction — never
/// only at `activate_version()` time, so `definition_status`/
/// `definition_diagnostics` are never stale between edits. The enforced
/// GATE is specifically in `AnalysisRuleset::activate_version()` below,
/// not here.
fn diagnose_analysis_ruleset_version(procedures: &[AnalysisProcedure], compatible_target_types: &[AnalysisCompatibleTargetType], ruleset_schema_version: &str, analysis_procedure_catalogue_version: &str, analysis_engine_min_version: &str) -> Vec<Diagnostic> {
    let table_type_allowed = compatible_target_types.contains(&AnalysisCompatibleTargetType::Table);
    let mut diagnostics = definition_diagnostics::diagnose_items(procedures, table_type_allowed, DiagnosticType::DuplicateProcedureId, DiagnosticType::UnsupportedProcedureType);
    diagnostics.extend(definition_diagnostics::diagnose_catalogue_version("ruleset_schema_version", ruleset_schema_version, SUPPORTED_RULESET_SCHEMA_VERSIONS));
    diagnostics.extend(definition_diagnostics::diagnose_catalogue_version("analysis_procedure_catalogue_version", analysis_procedure_catalogue_version, SUPPORTED_ANALYSIS_PROCEDURE_CATALOGUE_VERSIONS));
    diagnostics.extend(definition_diagnostics::diagnose_catalogue_version("analysis_engine_min_version", analysis_engine_min_version, SUPPORTED_ANALYSIS_ENGINE_MIN_VERSIONS));
    diagnostics
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalysisCompatibleTargetType {
    Table,
    Dataset,
}

/// B.3: single-target only. `requires_single_target_object` is not
/// caller-settable — it is always `true`, structurally, via
/// `TargetContract::new` (there is no other constructor). Cross-object
/// analysis has no representation anywhere in this type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetContract {
    pub allowed_target_types: Vec<AnalysisCompatibleTargetType>,
    requires_single_target_object: bool,
    pub minimum_row_count: Option<u64>,
    pub required_columns: Vec<String>,
}

impl TargetContract {
    pub fn new(allowed_target_types: Vec<AnalysisCompatibleTargetType>, minimum_row_count: Option<u64>, required_columns: Vec<String>) -> Self {
        let allowed_target_types = if allowed_target_types.is_empty() {
            vec![AnalysisCompatibleTargetType::Table, AnalysisCompatibleTargetType::Dataset]
        } else {
            allowed_target_types
        };
        Self { allowed_target_types, requires_single_target_object: true, minimum_row_count, required_columns }
    }

    pub fn requires_single_target_object(&self) -> bool {
        self.requires_single_target_object
    }
}

/// B.4. Amendment (same sampling-disclosure discipline as Inspection).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SampleMode {
    FullData,
    RowLimit,
    RandomSample,
    StratifiedSample,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplePolicy {
    pub mode: SampleMode,
    pub sample_size: Option<u64>,
    pub random_seed: Option<u64>,
}

impl Default for SamplePolicy {
    fn default() -> Self {
        Self { mode: SampleMode::FullData, sample_size: None, random_seed: None }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DefaultScope {
    pub included_columns: Vec<String>,
    pub excluded_columns: Vec<String>,
    pub row_filter_expression: Option<String>,
    pub sample_policy: SamplePolicy,
    pub allow_runtime_scope_override: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AnalysisRulesetError {
    #[error("version {0} is not Draft — editing an Active/Superseded/Archived version in place is forbidden; create a new version instead")]
    VersionNotEditable(Uuid),
    #[error("version {0} not found on this ruleset")]
    VersionNotFound(Uuid),
    #[error("only a Draft version may be activated (found {0:?})")]
    OnlyDraftVersionsCanActivate(AnalysisVersionState),
    /// B.7/Amendment 5: AP-001 cannot be disabled, structurally enforced
    /// at construction/edit time, not left to a runtime convention.
    #[error("AP-001 (Scope & Variable Role Setup) cannot be disabled")]
    Ap001CannotBeDisabled,
    #[error("a ruleset version may contain at most one AP-001 entry")]
    DuplicateAp001,
    /// Part C: "Errors block Save New Version" — a version carrying any
    /// `error`-severity definition diagnostic may never become Active.
    #[error("version {0} has blocking definition-diagnostic errors and cannot be activated")]
    DefinitionInvalid(Uuid),
}

fn normalize_procedures(mut procedures: Vec<AnalysisProcedure>) -> Result<Vec<AnalysisProcedure>, AnalysisRulesetError> {
    let ap001_count = procedures.iter().filter(|p| p.procedure_type() == "AP-001").count();
    if ap001_count > 1 {
        return Err(AnalysisRulesetError::DuplicateAp001);
    }
    if ap001_count == 0 {
        procedures.insert(0, AnalysisProcedure::mandatory_scope_setup(Uuid::new_v4(), "Scope & Variable Role Setup".to_string(), AnalysisProcedureTarget::table_scope()));
    } else {
        let ap001_enabled = procedures.iter().find(|p| p.procedure_type() == "AP-001").map(|p| p.enabled()).unwrap_or(false);
        if !ap001_enabled {
            return Err(AnalysisRulesetError::Ap001CannotBeDisabled);
        }
        // B.7: AP-001 always executes first — move it to the front
        // regardless of caller-supplied ordering.
        let idx = procedures.iter().position(|p| p.procedure_type() == "AP-001").unwrap();
        let ap001 = procedures.remove(idx);
        procedures.insert(0, ap001);
    }
    Ok(procedures)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRulesetVersion {
    analysis_ruleset_version_id: Uuid,
    version_number: u32,
    version_state: AnalysisVersionState,
    created_at: Timestamp,
    created_by: String,
    change_summary: String,
    based_on_version_id: Option<Uuid>,
    ruleset_schema_version: String,
    analysis_procedure_catalogue_version: String,
    analysis_engine_min_version: String,
    ruleset_hash: String,
    compatible_target_types: Vec<AnalysisCompatibleTargetType>,
    target_contract: TargetContract,
    default_scope: DefaultScope,
    procedures: Vec<AnalysisProcedure>,
    definition_status: DefinitionStatus,
    definition_diagnostics: Vec<Diagnostic>,
}

impl AnalysisRulesetVersion {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        analysis_ruleset_version_id: Uuid,
        version_number: u32,
        created_at: Timestamp,
        created_by: String,
        change_summary: String,
        based_on_version_id: Option<Uuid>,
        ruleset_schema_version: String,
        analysis_procedure_catalogue_version: String,
        analysis_engine_min_version: String,
        ruleset_hash: String,
        target_contract: TargetContract,
        default_scope: DefaultScope,
        procedures: Vec<AnalysisProcedure>,
    ) -> Result<Self, AnalysisRulesetError> {
        let procedures = normalize_procedures(procedures)?;
        let compatible_target_types = target_contract.allowed_target_types.clone();
        // Part C: diagnostics are computed at construction — never born
        // `Valid`/empty by default the way this field used to be
        // hardcoded before this task.
        let definition_diagnostics = diagnose_analysis_ruleset_version(&procedures, &compatible_target_types, &ruleset_schema_version, &analysis_procedure_catalogue_version, &analysis_engine_min_version);
        let definition_status = definition_diagnostics::status_from(&definition_diagnostics);
        Ok(Self {
            analysis_ruleset_version_id,
            version_number,
            version_state: AnalysisVersionState::Draft,
            created_at,
            created_by,
            change_summary,
            based_on_version_id,
            ruleset_schema_version,
            analysis_procedure_catalogue_version,
            analysis_engine_min_version,
            ruleset_hash,
            compatible_target_types,
            target_contract,
            default_scope,
            procedures,
            definition_status,
            definition_diagnostics,
        })
    }

    pub fn analysis_ruleset_version_id(&self) -> Uuid {
        self.analysis_ruleset_version_id
    }

    pub fn version_number(&self) -> u32 {
        self.version_number
    }

    pub fn version_state(&self) -> AnalysisVersionState {
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

    pub fn based_on_version_id(&self) -> Option<Uuid> {
        self.based_on_version_id
    }

    pub fn ruleset_schema_version(&self) -> &str {
        &self.ruleset_schema_version
    }

    pub fn analysis_procedure_catalogue_version(&self) -> &str {
        &self.analysis_procedure_catalogue_version
    }

    pub fn analysis_engine_min_version(&self) -> &str {
        &self.analysis_engine_min_version
    }

    pub fn ruleset_hash(&self) -> &str {
        &self.ruleset_hash
    }

    pub fn compatible_target_types(&self) -> &[AnalysisCompatibleTargetType] {
        &self.compatible_target_types
    }

    pub fn target_contract(&self) -> &TargetContract {
        &self.target_contract
    }

    pub fn default_scope(&self) -> &DefaultScope {
        &self.default_scope
    }

    pub fn procedures(&self) -> &[AnalysisProcedure] {
        &self.procedures
    }

    pub fn definition_status(&self) -> DefinitionStatus {
        self.definition_status
    }

    pub fn definition_diagnostics(&self) -> &[Diagnostic] {
        &self.definition_diagnostics
    }

    /// Draft-only edit (A.1/A.2's discipline, identical to Validation's
    /// `set_rules`) — and B.7's mandatory-AP-001 normalization applies
    /// here exactly as it does at construction: a caller cannot delete
    /// or disable AP-001 through this method either. Part C: diagnostics
    /// are recomputed on every Draft save, not just at construction or at
    /// `activate_version()` — `definition_status`/`definition_diagnostics`
    /// are never stale between edits. This method itself does NOT reject
    /// the save on diagnostic errors (a Draft may carry errors freely) —
    /// only `AnalysisRuleset::activate_version()` enforces that gate.
    pub fn set_procedures(&mut self, procedures: Vec<AnalysisProcedure>) -> Result<(), AnalysisRulesetError> {
        if self.version_state != AnalysisVersionState::Draft {
            return Err(AnalysisRulesetError::VersionNotEditable(self.analysis_ruleset_version_id));
        }
        self.procedures = normalize_procedures(procedures)?;
        self.definition_diagnostics = diagnose_analysis_ruleset_version(&self.procedures, &self.compatible_target_types, &self.ruleset_schema_version, &self.analysis_procedure_catalogue_version, &self.analysis_engine_min_version);
        self.definition_status = definition_diagnostics::status_from(&self.definition_diagnostics);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRuleset {
    analysis_ruleset_id: AnalysisRulesetId,
    analysis_ruleset_name: String,
    state: AnalysisRulesetState,
    current_version_id: Uuid,
    created_at: Timestamp,
    created_by: String,
    last_modified_at: Timestamp,
    last_modified_by: String,
    tags: Vec<String>,
    description: String,
    versions: Vec<AnalysisRulesetVersion>,
}

impl AnalysisRuleset {
    pub fn new(
        analysis_ruleset_id: AnalysisRulesetId,
        analysis_ruleset_name: String,
        created_at: Timestamp,
        created_by: String,
        tags: Vec<String>,
        description: String,
        initial_version: AnalysisRulesetVersion,
    ) -> Self {
        let current_version_id = initial_version.analysis_ruleset_version_id();
        Self {
            analysis_ruleset_id,
            analysis_ruleset_name,
            state: AnalysisRulesetState::Draft,
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

    pub fn analysis_ruleset_id(&self) -> AnalysisRulesetId {
        self.analysis_ruleset_id
    }

    pub fn analysis_ruleset_name(&self) -> &str {
        &self.analysis_ruleset_name
    }

    pub fn state(&self) -> AnalysisRulesetState {
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

    pub fn versions(&self) -> &[AnalysisRulesetVersion] {
        &self.versions
    }

    pub fn version(&self, id: Uuid) -> Option<&AnalysisRulesetVersion> {
        self.versions.iter().find(|v| v.analysis_ruleset_version_id() == id)
    }

    pub fn add_draft_version(&mut self, version: AnalysisRulesetVersion, modified_at: Timestamp, modified_by: String) {
        self.versions.push(version);
        self.last_modified_at = modified_at;
        self.last_modified_by = modified_by;
    }

    pub fn activate_version(&mut self, analysis_ruleset_version_id: Uuid, at: Timestamp, by: String) -> Result<(), AnalysisRulesetError> {
        let target = self.versions.iter().find(|v| v.analysis_ruleset_version_id() == analysis_ruleset_version_id).ok_or(AnalysisRulesetError::VersionNotFound(analysis_ruleset_version_id))?;
        if target.version_state != AnalysisVersionState::Draft {
            return Err(AnalysisRulesetError::OnlyDraftVersionsCanActivate(target.version_state));
        }
        // Part C's enforced gate — matches "Errors block Save New
        // Version": a Draft may carry errors freely (see `set_procedures`),
        // but the Draft->Active transition is refused if any
        // `error`-severity definition diagnostic is present. `Warning`/
        // `Information` diagnostics never block activation.
        if target.definition_status == DefinitionStatus::Invalid {
            return Err(AnalysisRulesetError::DefinitionInvalid(analysis_ruleset_version_id));
        }
        for v in self.versions.iter_mut() {
            if v.version_state == AnalysisVersionState::Active {
                v.version_state = AnalysisVersionState::Superseded;
            }
            if v.analysis_ruleset_version_id == analysis_ruleset_version_id {
                v.version_state = AnalysisVersionState::Active;
            }
        }
        self.current_version_id = analysis_ruleset_version_id;
        self.state = AnalysisRulesetState::Active;
        self.last_modified_at = at;
        self.last_modified_by = by;
        Ok(())
    }

    pub fn archive(&mut self, at: Timestamp, by: String) {
        self.state = AnalysisRulesetState::Archived;
        self.last_modified_at = at;
        self.last_modified_by = by;
    }
}
