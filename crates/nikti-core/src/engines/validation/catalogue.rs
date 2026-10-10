//! Validation Rule Function Catalogue v1.0 metadata (Part B). Frozen at
//! 30 functions — this array is not a starting point to extend or trim.
//! The Validation Engine rejects any `function_id` not present here at
//! runtime, resolving the owning rule to `not_executable` rather than
//! crashing or silently skipping it.

use crate::types::RuleNullPolicy;

#[derive(Debug, Clone, Copy)]
pub struct FunctionCatalogueEntry {
    pub function_id: &'static str,
    pub display_name: &'static str,
    pub category: &'static str,
    pub default_null_policy: RuleNullPolicy,
}

pub const CATALOGUE_ID: &str = "nikti-validation-v1";
pub const CATALOGUE_VERSION: &str = "1.0.0";
pub const RULESET_SCHEMA_VERSION: &str = "1.0";

macro_rules! entry {
    ($id:expr, $name:expr, $cat:expr, $policy:expr) => {
        FunctionCatalogueEntry { function_id: $id, display_name: $name, category: $cat, default_null_policy: $policy }
    };
}

use RuleNullPolicy::*;

pub const FUNCTION_CATALOGUE: &[FunctionCatalogueEntry] = &[
    entry!("not_missing", "Not Missing", "Completeness", FailIfMissing),
    entry!("not_blank", "Not Blank", "Completeness", FailIfMissing),
    entry!("not_whitespace_only", "Not Whitespace-Only", "Completeness", FailIfMissing),
    entry!("minimum_completeness", "Minimum Completeness", "Completeness", IgnoreMissing),
    entry!("one_of_required", "One Of Required", "Completeness", FailIfMissing),
    entry!("numeric_compatible", "Numeric Compatible", "Type Validity", IgnoreMissing),
    entry!("date_compatible", "Date Compatible", "Type Validity", IgnoreMissing),
    entry!("boolean_compatible", "Boolean Compatible", "Type Validity", IgnoreMissing),
    entry!("min_value", "Minimum Value", "Range/Boundary", IgnoreMissing),
    entry!("max_value", "Maximum Value", "Range/Boundary", IgnoreMissing),
    entry!("between_values", "Between Values", "Range/Boundary", IgnoreMissing),
    entry!("allowed_values", "Allowed Values", "Value Set", FailIfMissing),
    entry!("disallowed_values", "Disallowed Values", "Value Set", IgnoreMissing),
    entry!("distinct_count_between", "Distinct Count Between", "Value Set", IgnoreMissing),
    entry!("regex_match", "Regex Match", "Pattern/Format", IgnoreMissing),
    entry!("regex_not_match", "Regex Not Match", "Pattern/Format", IgnoreMissing),
    entry!("character_pattern_match", "Character Pattern Match", "Pattern/Format", IgnoreMissing),
    entry!("min_length", "Minimum Length", "Text Length", IgnoreMissing),
    entry!("max_length", "Maximum Length", "Text Length", IgnoreMissing),
    entry!("length_between", "Length Between", "Text Length", IgnoreMissing),
    entry!("unique_column", "Unique Column", "Uniqueness/Duplicates", IgnoreMissing),
    entry!("unique_column_set", "Unique Column Set", "Uniqueness/Duplicates", IgnoreMissing),
    entry!("no_duplicate_rows", "No Duplicate Rows", "Uniqueness/Duplicates", IgnoreMissing),
    entry!("column_compare", "Column Compare", "Cross-Column", IgnoreMissing),
    entry!("required_if", "Required If", "Cross-Column", FailIfMissing),
    entry!("mutually_exclusive", "Mutually Exclusive", "Cross-Column", FailIfMissing),
    entry!("exists_in_reference", "Exists In Reference", "Referential", FailIfMissing),
    entry!("not_exists_in_reference", "Not Exists In Reference", "Referential", IgnoreMissing),
    entry!("date_format_match", "Date Format Match", "Date/Time Format", IgnoreMissing),
    entry!("expression_true", "Expression True", "Expression", FailIfMissing),
];

pub fn is_supported(function_id: &str) -> bool {
    FUNCTION_CATALOGUE.iter().any(|e| e.function_id == function_id)
}

pub fn lookup(function_id: &str) -> Option<&'static FunctionCatalogueEntry> {
    FUNCTION_CATALOGUE.iter().find(|e| e.function_id == function_id)
}
