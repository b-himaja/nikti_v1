//! Amendment 2 (Inspection Report Design Spec): no metric may appear in
//! engine output unless it has a corresponding entry here. Built alongside
//! the engine — every new computed value gets an entry in the same change
//! that introduces it, not after the fact.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exactness {
    Exact,
    Sampled,
}

#[derive(Debug, Clone, Copy)]
pub struct MetricDictionaryEntry {
    pub key: &'static str,
    pub description: &'static str,
    pub input_scope: &'static str,
    pub formula: &'static str,
    pub applicable_types: &'static str,
    pub exactness: Exactness,
    pub threshold_ref: Option<&'static str>,
    pub output_format: &'static str,
    pub section: &'static str,
}

macro_rules! metric {
    ($key:expr, $desc:expr, $scope:expr, $formula:expr, $types:expr, $exactness:expr, $threshold:expr, $format:expr, $section:expr) => {
        MetricDictionaryEntry {
            key: $key,
            description: $desc,
            input_scope: $scope,
            formula: $formula,
            applicable_types: $types,
            exactness: $exactness,
            threshold_ref: $threshold,
            output_format: $format,
            section: $section,
        }
    };
}

pub const METRIC_DICTIONARY: &[MetricDictionaryEntry] = &[
    metric!("identity.table_id", "The Table's identity.", "table metadata", "n/a", "any", Exactness::Exact, None, "uuid", "Dataset Identity"),
    metric!("identity.row_count", "Total row count.", "full table", "count(rows)", "any", Exactness::Exact, None, "integer", "Dataset Identity"),
    metric!("identity.column_count", "Total column count.", "table schema", "count(columns)", "any", Exactness::Exact, None, "integer", "Dataset Identity"),
    metric!("identity.content_fingerprint", "Content-addressed fingerprint of the Table.", "table metadata", "n/a", "any", Exactness::Exact, None, "string", "Dataset Identity"),
    metric!("import_health.rows_read", "Rows successfully read during import.", "import event record", "n/a", "any", Exactness::Exact, None, "integer", "Import Health"),
    metric!("import_health.rows_rejected", "Rows rejected during import.", "import event record", "n/a", "any", Exactness::Exact, None, "integer", "Import Health"),
    metric!("import_health.parse_error_count", "Parse errors encountered during import.", "import event record", "n/a", "any", Exactness::Exact, None, "integer", "Import Health"),
    metric!("import_health.encoding", "Detected/declared source encoding.", "import event record", "n/a", "any", Exactness::Exact, None, "string", "Import Health"),
    metric!("shape.row_count", "Row count.", "full table", "count(rows)", "any", Exactness::Exact, None, "integer", "Dataset Shape"),
    metric!("shape.column_count", "Column count.", "table schema", "count(columns)", "any", Exactness::Exact, None, "integer", "Dataset Shape"),
    metric!("shape.estimated_memory_bytes", "Estimated in-memory size.", "table schema + row count", "sum(per-column estimated width) * row_count", "any", Exactness::Exact, None, "integer (bytes)", "Dataset Shape"),
    metric!("completeness.overall_missing_ratio", "Share of all cells that are null.", "full table", "null_cells / total_cells", "any", Exactness::Exact, None, "ratio [0,1]", "Completeness"),
    metric!("completeness.column_missing_ratio", "Per-column share of null values.", "full table, per column", "null_count(col) / row_count", "any", Exactness::Exact, Some("high_missingness_min_ratio"), "ratio [0,1]", "Completeness"),
    metric!("data_types.column_dtype", "Declared/inferred dtype per column.", "table schema", "n/a", "any", Exactness::Exact, None, "string", "Data Types"),
    metric!("cardinality.distinct_count", "Distinct non-null value count per column.", "full table, per column", "n_unique(col)", "any", Exactness::Exact, None, "integer", "Values & Cardinality"),
    metric!("cardinality.distinct_ratio", "Distinct value count over row count.", "full table, per column", "n_unique(col) / row_count", "any", Exactness::Exact, Some("high_cardinality_min_ratio"), "ratio [0,1]", "Values & Cardinality"),
    metric!("cardinality.columns_with_100pct_uniqueness", "Columns whose values are all distinct.", "full table", "n_unique(col) == row_count", "any", Exactness::Exact, None, "list of column names", "Values & Cardinality"),
    metric!("duplicates.exact_duplicate_row_count", "Fully duplicated rows.", "full table", "count(rows) - count(distinct rows)", "any", Exactness::Exact, None, "integer", "Duplicates & Matches"),
    metric!("duplicates.duplicate_row_ratio", "Duplicate rows over total rows.", "full table", "exact_duplicate_row_count / row_count", "any", Exactness::Exact, None, "ratio [0,1]", "Duplicates & Matches"),
    metric!("number_profile.min", "Minimum value.", "full table, per numeric column", "min(col)", "numeric", Exactness::Exact, None, "number", "Number Profile"),
    metric!("number_profile.max", "Maximum value.", "full table, per numeric column", "max(col)", "numeric", Exactness::Exact, None, "number", "Number Profile"),
    metric!("number_profile.mean", "Arithmetic mean.", "full table, per numeric column", "mean(col)", "numeric", Exactness::Exact, None, "number", "Number Profile"),
    metric!("number_profile.median", "Median (50th percentile).", "full table, per numeric column", "quantile(col, 0.5)", "numeric", Exactness::Exact, None, "number", "Number Profile"),
    metric!("number_profile.std_dev", "Sample standard deviation.", "full table, per numeric column", "std(col)", "numeric", Exactness::Exact, None, "number", "Number Profile"),
    metric!("number_profile.quartile_iqr_summary", "Q1/Q2/Q3 and interquartile range.", "full table, per numeric column", "quantile(col, 0.25/0.5/0.75); iqr = q3 - q1", "numeric", Exactness::Exact, None, "{q1,q2,q3,iqr}", "Number Profile"),
    metric!("category_profile.top_values", "Most frequent values and their counts.", "full/sampled table, per categorical column", "value_counts(col) top-N", "string/categorical", Exactness::Sampled, None, "list of {value,count}", "Category Profile"),
    metric!("category_profile.rare_value_count", "Values occurring below the rare-value threshold.", "full/sampled table, per categorical column", "count(values where freq_ratio <= rare_value_max_ratio)", "string/categorical", Exactness::Sampled, Some("rare_value_max_ratio"), "integer", "Category Profile"),
    metric!("datetime_profile.min", "Earliest value.", "full table, per datetime column", "min(col)", "datetime", Exactness::Exact, None, "timestamp", "Date & Time Profile"),
    metric!("datetime_profile.max", "Latest value.", "full table, per datetime column", "max(col)", "datetime", Exactness::Exact, None, "timestamp", "Date & Time Profile"),
    metric!("datetime_profile.distinct_count", "Distinct value count.", "full table, per datetime column", "n_unique(col)", "datetime", Exactness::Exact, None, "integer", "Date & Time Profile"),
    metric!("text_profile.min_length", "Shortest string length.", "full/sampled table, per text column", "min(len(col))", "string", Exactness::Sampled, None, "integer", "Text Profile"),
    metric!("text_profile.max_length", "Longest string length.", "full/sampled table, per text column", "max(len(col))", "string", Exactness::Sampled, None, "integer", "Text Profile"),
    metric!("text_profile.mean_length", "Mean string length.", "full/sampled table, per text column", "mean(len(col))", "string", Exactness::Sampled, None, "number", "Text Profile"),
    metric!("text_profile.long_text_count", "Values at/above the long-text length threshold.", "full/sampled table, per text column", "count(len(col) >= long_text_min_chars)", "string", Exactness::Sampled, Some("long_text_min_chars"), "integer", "Text Profile"),
    metric!("format_patterns.distinct_format_count", "Distinct structural shapes (digits/letters reduced).", "sampled table, per text column", "n_unique(shape(col))", "string", Exactness::Sampled, None, "integer", "Format Patterns"),
    metric!("format_patterns.dominant_format_ratio", "Share held by the most common structural shape.", "sampled table, per text column", "max(shape frequency) / sampled_count", "string", Exactness::Sampled, Some("pattern_dominance_min_ratio"), "ratio [0,1]", "Format Patterns"),
    metric!("whitespace.leading_trailing_count", "Values with leading/trailing whitespace.", "sampled table, per text column", "count(trim(v) != v)", "string", Exactness::Sampled, None, "integer", "Whitespace & Hidden Characters"),
    metric!("whitespace.control_char_count", "Values containing control characters.", "sampled table, per text column", "count(v contains control char)", "string", Exactness::Sampled, None, "integer", "Whitespace & Hidden Characters"),
    metric!("character_condition_table.non_ascii_count", "Values containing non-ASCII characters.", "sampled table, per text column", "count(v contains non-ascii char)", "string", Exactness::Sampled, None, "integer", "Whitespace & Hidden Characters"),
    metric!("row_profile.row_condition_index", "Per-row missing-field count for flagged rows.", "full table", "count(nulls in row)", "any", Exactness::Exact, Some("row_high_missingness_min_ratio"), "list of {row_index,missing_field_count}", "Row Profile"),
    metric!("row_profile.rows_flagged_high_missingness", "Rows at/above the row-level missingness threshold.", "full table", "count(rows where missing_ratio >= row_high_missingness_min_ratio)", "any", Exactness::Exact, Some("row_high_missingness_min_ratio"), "integer", "Row Profile"),
    metric!("column_cards.summary", "Per-column composite summary (dtype, missing ratio, distinct count, sample values).", "full/sampled table, per column", "n/a (composite of other metrics)", "any", Exactness::Sampled, None, "object", "Column Cards"),
    metric!("column_relationships.pairwise_numeric_coefficient", "Pearson coefficient between two numeric columns.", "full/sampled table, per numeric column pair", "pearson(col_a, col_b)", "numeric", Exactness::Sampled, None, "number [-1,1]", "Column Relationships"),
    metric!("distribution_views.histogram_bin", "Bin range and count underlying a distribution chart.", "full/sampled table, per numeric column", "fixed-width bin count(col)", "numeric", Exactness::Sampled, None, "list of {range_start,range_end,count}", "Distribution Views"),
    metric!("data_checks.detected_condition_count", "Count of rows/values meeting a fixed structural condition.", "full/sampled table", "n/a (per-check formula)", "any", Exactness::Sampled, None, "integer", "Data Checks"),
    metric!("metadata_coverage.columns_with_type_metadata", "Columns carrying a declared/inferred dtype.", "table schema", "count(columns where dtype is known)", "any", Exactness::Exact, None, "integer", "Metadata Coverage"),
    metric!("evidence_trail.inspected_at", "Timestamp this inspection ran.", "engine execution context", "n/a", "any", Exactness::Exact, None, "timestamp", "Evidence Trail"),
    metric!("evidence_trail.engine_version", "Engine version that produced this inspection.", "engine execution context", "n/a", "any", Exactness::Exact, None, "string", "Evidence Trail"),
];

pub fn lookup(key: &str) -> Option<&'static MetricDictionaryEntry> {
    METRIC_DICTIONARY.iter().find(|e| e.key == key)
}

pub fn contains(key: &str) -> bool {
    lookup(key).is_some()
}
