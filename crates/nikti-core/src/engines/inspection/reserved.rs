//! Reserved / not part of V1 Standard Inspection.
//!
//! The Inspection Report Design Spec (A.1, "Future") explicitly excludes
//! semantic type classification from V1 Inspection scope — INV-028
//! excludes "Future"/"Reserved" items by default without explicit written
//! approval. An earlier task session ported this from the legacy
//! `detect_semantic_type` (reshaped to drop its confidence/verdict
//! framing per Rule 11) before that exclusion was written down this
//! explicitly; the newer, more specific spec supersedes that framing.
//!
//! Rather than deleting already-tested, invariant-compliant code, this
//! module keeps it available as a standalone utility, explicitly outside
//! `NativeInspectionEngine::inspect()`'s Standard V1 output and not wired
//! into any command. It is not "Inspection" in the INV-011 sense (it is
//! not invoked automatically, unconditionally, on every import) — treat
//! it as dormant until a future task explicitly promotes semantic type
//! classification back into scope with written approval.

use std::sync::OnceLock;

use polars::prelude::*;
use regex::Regex;

use crate::engines::content::TableContent;

use super::InspectionError;

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticPatternMatch {
    pub pattern: String,
    pub match_ratio: f64,
}

/// Rule 11: reports each fixed pattern's raw match ratio as descriptive
/// evidence — no "detected" flag, no threshold applied here, no
/// confidence. Interpreting the ratios is left to whoever reads this, a
/// human or a user-authored Validation Ruleset (INV-012) — never this
/// function.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticTypeScanResult {
    pub column: String,
    pub sampled_rows: usize,
    pub matches: Vec<SemanticPatternMatch>,
}

const COUNTRIES: &[&str] = &[
    "usa",
    "us",
    "united states",
    "uk",
    "gb",
    "united kingdom",
    "canada",
    "ca",
    "australia",
    "au",
    "germany",
    "de",
    "france",
    "fr",
    "india",
    "in",
    "china",
    "cn",
    "japan",
    "jp",
    "brazil",
    "br",
];

const PATTERNS: &[(&str, Option<&str>)] = &[
    ("email", Some(r"^[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}$")),
    ("phone", Some(r"^\+?[\d\s\-().]{7,20}$")),
    ("url", Some(r"^https?://[^\s]+$")),
    ("postal_code", Some(r"^\d{4,10}(-\d{4})?$")),
    ("currency", Some(r"^[$€£¥]?\s*[\d,]+(\.\d{1,4})?$")),
    ("country", None),
];

fn compiled_patterns() -> &'static Vec<(&'static str, Option<Regex>)> {
    static CELL: OnceLock<Vec<(&'static str, Option<Regex>)>> = OnceLock::new();
    CELL.get_or_init(|| {
        PATTERNS.iter().map(|(name, pat)| (*name, pat.map(|p| Regex::new(p).expect("static pattern is valid")))).collect()
    })
}

pub fn scan_semantic_types(content: TableContent, column: &str, sample_size: usize) -> Result<SemanticTypeScanResult, InspectionError> {
    let schema = content.frame.clone().collect_schema()?;
    if schema.get(column).is_none() {
        return Err(InspectionError::ColumnNotFound(column.to_string()));
    }

    let df = content
        .frame
        .select([col(column).cast(DataType::String)])
        .filter(col(column).is_not_null())
        .collect()?;

    let sampled = if df.height() > sample_size {
        df.sample_n_literal(sample_size, false, false, Some(0))?
    } else {
        df
    };

    let values: Vec<String> = sampled
        .column(column)?
        .str()?
        .into_no_null_iter()
        .map(|s| s.to_string())
        .collect();
    let total = values.len().max(1);

    let matches: Vec<SemanticPatternMatch> = compiled_patterns()
        .iter()
        .map(|(name, regex)| {
            let hit_count = match regex {
                Some(re) => values.iter().filter(|v| re.is_match(v.trim())).count(),
                None => values.iter().filter(|v| COUNTRIES.contains(&v.to_lowercase().trim())).count(),
            };
            SemanticPatternMatch {
                pattern: name.to_string(),
                match_ratio: (hit_count as f64 / total as f64 * 1000.0).round() / 1000.0,
            }
        })
        .collect();

    Ok(SemanticTypeScanResult { column: column.to_string(), sampled_rows: values.len(), matches })
}
