//! Validation Rule Function Catalogue v1.0 (as summarized in the
//! governing task prompt — the full source document was unavailable in
//! this repository at implementation time; see the final compliance
//! report). Frozen at 30 functions for V1 — this is not a starting point
//! to extend or trim.
//!
//! Each known function gets its own enum variant carrying exactly its
//! parameters, typed — this mirrors `TransformationOperation`'s
//! established pattern in this codebase rather than a stringly-typed
//! parameter bag, so a rule's parameters can never mismatch its
//! function's expected shape at construction time.
//!
//! `Unknown` exists specifically so an unsupported/deprecated/future
//! `function_id` can still be *represented* (e.g. an old Ruleset
//! referencing a function since removed from the catalogue) — the engine
//! resolves it to the `not_executable` outcome rather than failing to
//! construct the rule at all (Part B: "the Validation Engine rejects
//! unsupported function_ids at runtime").

use serde::{Deserialize, Serialize};

use super::scalar_value::ScalarValue;
use super::transformation_operation::CompareOp;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationFunctionParams {
    // --- Completeness (5) ---
    NotMissing,
    NotBlank,
    NotWhitespaceOnly,
    MinimumCompleteness { min_ratio: f64 },
    OneOfRequired { columns: Vec<String> },

    // --- Type Validity (3) ---
    NumericCompatible,
    DateCompatible { format: Option<String> },
    BooleanCompatible,

    // --- Range/Boundary (3) ---
    MinValue { min: ScalarValue, inclusive: bool },
    MaxValue { max: ScalarValue, inclusive: bool },
    BetweenValues { min: ScalarValue, max: ScalarValue, inclusive: bool },

    // --- Value Set (3) ---
    AllowedValues { values: Vec<ScalarValue> },
    DisallowedValues { values: Vec<ScalarValue> },
    DistinctCountBetween { min: u64, max: u64 },

    // --- Pattern/Format (3) ---
    RegexMatch { pattern: String },
    RegexNotMatch { pattern: String },
    CharacterPatternMatch { allowed_chars: String },

    // --- Text Length (3) ---
    MinLength { min: u64 },
    MaxLength { max: u64 },
    LengthBetween { min: u64, max: u64 },

    // --- Uniqueness/Duplicates (3) ---
    UniqueColumn,
    UniqueColumnSet { columns: Vec<String> },
    NoDuplicateRows,

    // --- Cross-Column (3) ---
    ColumnCompare { other_column: String, op: CompareOp },
    RequiredIf { other_column: String, other_equals: ScalarValue },
    MutuallyExclusive { columns: Vec<String> },

    // --- Referential (2) ---
    // No repository-backed reference-object loading exists anywhere in
    // this codebase yet (see `content.rs`'s own doc comments on
    // `TableContent`/`DatasetContent`) — the reference set is supplied
    // directly as a resolved value list, matching the "Resolve
    // References" pipeline stage's *output* shape, not its resolution
    // mechanism (which is later, repository-backed work).
    ExistsInReference { reference_values: Vec<ScalarValue> },
    NotExistsInReference { reference_values: Vec<ScalarValue> },

    // --- Date/Time Format (1) ---
    DateFormatMatch { format: String },

    // --- Expression (1) ---
    ExpressionTrue { formula: String },

    /// Not part of the frozen 30 — see the module doc comment.
    Unknown,
}

impl ValidationFunctionParams {
    pub fn function_id(&self) -> &'static str {
        match self {
            Self::NotMissing => "not_missing",
            Self::NotBlank => "not_blank",
            Self::NotWhitespaceOnly => "not_whitespace_only",
            Self::MinimumCompleteness { .. } => "minimum_completeness",
            Self::OneOfRequired { .. } => "one_of_required",
            Self::NumericCompatible => "numeric_compatible",
            Self::DateCompatible { .. } => "date_compatible",
            Self::BooleanCompatible => "boolean_compatible",
            Self::MinValue { .. } => "min_value",
            Self::MaxValue { .. } => "max_value",
            Self::BetweenValues { .. } => "between_values",
            Self::AllowedValues { .. } => "allowed_values",
            Self::DisallowedValues { .. } => "disallowed_values",
            Self::DistinctCountBetween { .. } => "distinct_count_between",
            Self::RegexMatch { .. } => "regex_match",
            Self::RegexNotMatch { .. } => "regex_not_match",
            Self::CharacterPatternMatch { .. } => "character_pattern_match",
            Self::MinLength { .. } => "min_length",
            Self::MaxLength { .. } => "max_length",
            Self::LengthBetween { .. } => "length_between",
            Self::UniqueColumn => "unique_column",
            Self::UniqueColumnSet { .. } => "unique_column_set",
            Self::NoDuplicateRows => "no_duplicate_rows",
            Self::ColumnCompare { .. } => "column_compare",
            Self::RequiredIf { .. } => "required_if",
            Self::MutuallyExclusive { .. } => "mutually_exclusive",
            Self::ExistsInReference { .. } => "exists_in_reference",
            Self::NotExistsInReference { .. } => "not_exists_in_reference",
            Self::DateFormatMatch { .. } => "date_format_match",
            Self::ExpressionTrue { .. } => "expression_true",
            Self::Unknown => "unknown",
        }
    }
}
