//! Parameter types for every ported Transformation operation. One variant
//! of `TransformationOperation` per legacy function in
//! `legacy/datawrangler/python/engine/ops/{column,table,record}_ops.py`.
//! `detect_semantic_type` is deliberately absent — it doesn't mutate a
//! Dataset (not Transformation-shaped) and moved to InspectionEngine.

use super::scalar_value::ScalarValue;

// ── Shared sub-structures ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CompareOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    IsNull,
    IsNotNull,
    Contains,
    StartsWith,
    EndsWith,
    Matches,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CaseLogic {
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SubCondition {
    pub column: String,
    pub op: CompareOp,
    pub value: Option<ScalarValue>,
    pub negate: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConditionCase {
    pub sub_conditions: Vec<SubCondition>,
    pub logic: CaseLogic,
    pub then: ScalarValue,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DiscretiseMethod {
    Fixed { bins: Vec<f64> },
    EqualWidth { n_bins: u32 },
    Quantile { n_bins: u32 },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FillStrategy {
    Constant(ScalarValue),
    Mean,
    Median,
    Mode,
    Forward,
    Backward,
    Interpolate,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ScalerMethod {
    MinMax,
    ZScore,
    Robust,
    Log,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PadSide {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum StringOpKind {
    Trim,
    Upper,
    Lower,
    Replace { pattern: String, replacement: String, regex: bool },
    Extract { pattern: String },
    Pad { side: PadSide, width: usize, fill_char: char },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DatetimeOpKind {
    ExtractYear,
    ExtractMonth,
    ExtractDay,
    ExtractWeekday,
    ExtractHour,
    ExtractMinute,
    ToUnix,
    AddDays { days: i64 },
    /// Always returns whole days, matching the legacy implementation's
    /// actual behavior — its `unit` payload field was accepted but never
    /// read (a legacy bug), so there is nothing to port beyond "days".
    Diff { other_column: String },
    Parse { format: Option<String> },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FlagMethod {
    OutlierIqr,
    OutlierZScore { threshold: f64 },
    TopN { n: usize },
    BottomN { n: usize },
    Duplicates,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TargetType {
    Int8,
    Int16,
    Int32,
    Int64,
    UInt32,
    UInt64,
    Float32,
    Float64,
    Utf8,
    Boolean,
    Date,
    Datetime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HashAlgorithm {
    Sha256,
    Md5,
    Sha1,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum JoinKeys {
    On(Vec<String>),
    LeftRight { left_on: Vec<String>, right_on: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum JoinHow {
    Inner,
    Left,
    Right,
    Full,
    Cross,
    Anti,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MatchBy {
    Name,
    Position,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AggFunc {
    Sum,
    Mean,
    Min,
    Max,
    Count,
    First,
    Last,
    Std,
    Var,
    Median,
    NUnique,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AggSpec {
    pub column: String,
    pub func: AggFunc,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RollingFunc {
    Mean,
    Sum,
    Min,
    Max,
    Std,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SampleMethod {
    Random,
    Stratified,
    Systematic,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SortKey {
    pub column: String,
    pub descending: bool,
}

// ── The operation enum ──────────────────────────────────────────────────

/// One variant per ported legacy operation. `Recipe::steps` is a sequence
/// of these; a single ad hoc call from the command layer is also exactly
/// one of these — there is no separate "single op" vs "recipe step" type.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TransformationOperation {
    // -- column_ops.py --
    DeriveColumn { output_col: String, formula: String, overwrite: bool },
    RecodeColumn { column: String, mapping: Vec<(String, String)>, default: Option<String>, output_col: Option<String> },
    DiscretiseColumn { column: String, output_col: Option<String>, method: DiscretiseMethod, labels: Option<Vec<String>> },
    LeadLag { column: String, output_col: Option<String>, periods: i64, group_by: Option<String>, fill_value: Option<ScalarValue> },
    FillMissing { columns: Vec<String>, strategy: FillStrategy },
    ApplyScaler { columns: Vec<String>, method: ScalerMethod, output_suffix: Option<String> },
    RenameColumns { mapping: Vec<(String, String)> },
    ReorderColumns { column_order: Vec<String> },
    StringOp { column: String, output_col: Option<String>, op: StringOpKind },
    DatetimeOp { column: String, output_col: Option<String>, op: DatetimeOpKind },
    Differencing { column: String, output_col: Option<String>, periods: i64, pct_change: bool },
    ConditionalFlag { output_col: String, conditions: Vec<ConditionCase>, default: Option<ScalarValue> },
    FlagValues { column: String, output_col: Option<String>, method: FlagMethod },
    ChangeDtype { column: String, target_type: TargetType, format: Option<String> },
    DropColumns { columns: Vec<String> },
    CopyColumn { column: String, output_col: String },
    CoalesceColumns { columns: Vec<String>, output_col: String },
    GenerateRowNumber { output_col: Option<String>, start: i64 },
    GenerateUuid { output_col: Option<String>, seed: Option<u64> },
    NullStandardise { columns: Vec<String>, null_variants: Option<Vec<String>> },
    TextStandardise { column: String, output_col: Option<String>, mapping: Vec<(String, String)>, case_sensitive: bool },
    HashColumn { column: String, output_col: Option<String>, algorithm: HashAlgorithm },

    // -- table_ops.py (transform_many, 2+ Dataset inputs) --
    JoinDatasets { keys: JoinKeys, how: JoinHow, suffix: Option<String> },
    AppendDatasets { match_by: MatchBy, deduplicate: bool, add_source_col: bool },
    IntersectDatasets,
    SubtractDatasets,
    CompareDatasets { key_columns: Vec<String> },

    // -- table_ops.py (transform, single Dataset) --
    UnpivotDataset { id_vars: Vec<String>, value_vars: Option<Vec<String>>, variable_name: String, value_name: String },
    AggregateDataset { group_by: Vec<String>, aggregations: Vec<AggSpec> },
    MovingCalc { column: String, output_col: Option<String>, func: RollingFunc, window: u32, min_periods: Option<u32> },
    RunningTotal { column: String, output_col: Option<String>, group_by: Option<String> },

    // -- record_ops.py --
    FilterDataset { condition: String, produce_complement: bool },
    SampleDataset { method: SampleMethod, n: Option<u64>, fraction: Option<f64>, seed: u64, group_by: Option<String> },
    SortDataset { sort_by: Vec<SortKey>, nulls_last: bool },
    SliceRows { start: i64, end: i64 },
    DeleteRows { indices: Vec<u64> },
    TagRows { tag_col: Option<String>, indices: Vec<u64>, tag: String },
}
