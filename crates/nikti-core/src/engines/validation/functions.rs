//! Validation Rule Function Catalogue v1.0 — execution logic (Part B).
//! Every function is deterministic and never modifies `df`. Most reduce
//! to one boolean "pass" expression evaluated per row, combined with the
//! rule's resolved null policy via `count_outcomes` — this shared core is
//! what the "same contract, every function" requirement in Part B means
//! in practice, rather than 30 independent ad-hoc implementations.

use polars::prelude::*;

use crate::engines::transformation::formula::FormulaParser;
use crate::types::validation_function_params::ValidationFunctionParams;
use crate::types::{CompareOp, RuleNullPolicy, RuleTarget, ScalarValue};

use super::ValidationError;

pub struct FunctionOutcome {
    pub evaluated_count: u64,
    pub passed_count: u64,
    pub failed_count: u64,
    pub affected_columns: Vec<String>,
    /// The real rows that failed, when "failed rows" is a meaningful
    /// concept for this function — `None` for the two table/column-level
    /// aggregate checks (`MinimumCompleteness`, `DistinctCountBetween`),
    /// where there is no natural per-row subset to point at. Written to
    /// real storage by the command layer, never embedded in evidence
    /// (C.4) — this is the in-memory content behind `AffectedRowsRef`,
    /// not the reference itself.
    pub affected_rows: Option<DataFrame>,
}

/// The shared null-policy core (A.3): every row-shaped function resolves
/// to this. `missing`/`pass` are precomputed per-row so both the
/// Expr-based path (most functions) and the mask-based path (uniqueness
/// functions, which need `DataFrame::is_duplicated`, not an `Expr`) share
/// one counting rule — no function reimplements policy semantics itself.
/// The returned `Vec<bool>` is a per-row "this row counts as failed under
/// this policy" flag (e.g. always `false` for a row `IgnoreMissing` skips
/// entirely) — used to build the real `affected_rows` subset, not just
/// the aggregate `failed` count.
fn count_outcomes(missing: &[bool], pass: &[Option<bool>], policy: RuleNullPolicy) -> (u64, u64, u64, Vec<bool>) {
    let mut evaluated = 0u64;
    let mut passed = 0u64;
    let mut failed = 0u64;
    let mut failed_mask = Vec::with_capacity(missing.len());
    for (is_missing, p) in missing.iter().zip(pass.iter()) {
        let p = p.unwrap_or(false);
        let mut row_failed = false;
        match policy {
            RuleNullPolicy::FailIfMissing => {
                evaluated += 1;
                if *is_missing || !p {
                    failed += 1;
                    row_failed = true;
                } else {
                    passed += 1;
                }
            }
            RuleNullPolicy::IgnoreMissing => {
                if !*is_missing {
                    evaluated += 1;
                    if p {
                        passed += 1;
                    } else {
                        failed += 1;
                        row_failed = true;
                    }
                }
            }
            RuleNullPolicy::PassIfMissing => {
                evaluated += 1;
                if *is_missing || p {
                    passed += 1;
                } else {
                    failed += 1;
                    row_failed = true;
                }
            }
            RuleNullPolicy::EvaluateMissingAsValue => {
                evaluated += 1;
                if p {
                    passed += 1;
                } else {
                    failed += 1;
                    row_failed = true;
                }
            }
            RuleNullPolicy::InheritRulesetDefault => {
                unreachable!("resolved to a concrete policy before function execution")
            }
        }
        failed_mask.push(row_failed);
    }
    (evaluated, passed, failed, failed_mask)
}

fn evaluate_expr_predicate(
    df: &DataFrame,
    is_missing_expr: Expr,
    pass_expr: Expr,
    policy: RuleNullPolicy,
) -> Result<(u64, u64, u64, DataFrame), ValidationError> {
    let out = df.clone().lazy().select([is_missing_expr.alias("__missing"), pass_expr.alias("__pass")]).collect()?;
    let missing: Vec<bool> = out.column("__missing")?.bool()?.into_iter().map(|v| v.unwrap_or(true)).collect();
    let pass: Vec<Option<bool>> = out.column("__pass")?.bool()?.into_iter().collect();
    let (evaluated, passed, failed, failed_mask) = count_outcomes(&missing, &pass, policy);
    let mask_series = BooleanChunked::from_iter_values(PlSmallStr::from_str("__failed"), failed_mask.into_iter());
    let affected_rows = df.filter(&mask_series)?;
    Ok((evaluated, passed, failed, affected_rows))
}

fn values_series(name: &str, values: &[ScalarValue]) -> Series {
    let s: Vec<AnyValue> = values
        .iter()
        .map(|v| match v {
            ScalarValue::Null => AnyValue::Null,
            ScalarValue::Bool(b) => AnyValue::Boolean(*b),
            ScalarValue::Int(i) => AnyValue::Int64(*i),
            ScalarValue::Float(f) => AnyValue::Float64(*f),
            ScalarValue::Str(s) => AnyValue::StringOwned(s.as_str().into()),
        })
        .collect();
    Series::from_any_values(PlSmallStr::from_str(name), &s, false).expect("literal value list must construct")
}

pub fn execute_function(
    df: &DataFrame,
    function: &ValidationFunctionParams,
    target: &RuleTarget,
    policy: RuleNullPolicy,
) -> Result<FunctionOutcome, ValidationError> {
    use ValidationFunctionParams::*;

    let single = target.target_columns.first().cloned().unwrap_or_default();
    let affected_columns = target.target_columns.clone();
    let row_count = df.height() as u64;

    let simple = |is_missing: Expr, pass: Expr| -> Result<FunctionOutcome, ValidationError> {
        let (evaluated_count, passed_count, failed_count, affected_rows) = evaluate_expr_predicate(df, is_missing, pass, policy)?;
        Ok(FunctionOutcome { evaluated_count, passed_count, failed_count, affected_columns: affected_columns.clone(), affected_rows: Some(affected_rows) })
    };

    match function {
        // --- Completeness ---
        NotMissing => simple(col(&single).is_null(), lit(true)),
        NotBlank => {
            let s = col(&single).cast(DataType::String);
            simple(col(&single).is_null().or(s.clone().eq(lit(""))), lit(true))
        }
        NotWhitespaceOnly => {
            let trimmed = col(&single).cast(DataType::String).str().strip_chars(lit(NULL));
            simple(col(&single).is_null().or(trimmed.eq(lit(""))), lit(true))
        }
        MinimumCompleteness { min_ratio } => {
            let non_null = row_count - df.column(&single)?.null_count() as u64;
            let ratio = if row_count == 0 { 1.0 } else { non_null as f64 / row_count as f64 };
            let pass = ratio >= *min_ratio;
            // No natural per-row "affected" subset for a whole-column
            // completeness ratio — never fabricated, left `None`.
            Ok(FunctionOutcome { evaluated_count: 1, passed_count: pass as u64, failed_count: (!pass) as u64, affected_columns, affected_rows: None })
        }
        OneOfRequired { columns } => {
            let pass_expr = columns
                .iter()
                .map(|c| col(c).is_not_null())
                .reduce(|a, b| a.or(b))
                .unwrap_or_else(|| lit(false));
            simple(lit(false), pass_expr)
        }

        // --- Type Validity ---
        NumericCompatible => {
            let s = col(&single).cast(DataType::String);
            simple(col(&single).is_null(), s.str().contains(lit(r"^-?\d+(\.\d+)?$"), false))
        }
        DateCompatible { format } => {
            let fmt = format.clone().unwrap_or_else(|| "%Y-%m-%d".to_string());
            let parsed = col(&single)
                .cast(DataType::String)
                .str()
                .to_date(StrptimeOptions { format: Some(PlSmallStr::from_str(&fmt)), strict: false, ..Default::default() });
            simple(col(&single).is_null(), parsed.is_not_null())
        }
        BooleanCompatible => {
            let s = col(&single).cast(DataType::String).str().to_lowercase();
            let allowed = values_series("allowed", &["true", "false", "1", "0", "yes", "no"].map(|v| ScalarValue::Str(v.to_string())));
            simple(col(&single).is_null(), s.is_in(lit(allowed)))
        }

        // --- Range/Boundary ---
        MinValue { min, inclusive } => {
            let c = col(&single).cast(DataType::Float64);
            let pass = if *inclusive { c.gt_eq(min.to_lit_expr()) } else { c.gt(min.to_lit_expr()) };
            simple(col(&single).is_null(), pass)
        }
        MaxValue { max, inclusive } => {
            let c = col(&single).cast(DataType::Float64);
            let pass = if *inclusive { c.lt_eq(max.to_lit_expr()) } else { c.lt(max.to_lit_expr()) };
            simple(col(&single).is_null(), pass)
        }
        BetweenValues { min, max, inclusive } => {
            let dtype = df.column(&single)?.dtype().clone();
            // Handles both numeric and date bounds (Part B: "do not build
            // a separate date_between") — date bounds are ISO 8601
            // `ScalarValue::Str`s, compared lexicographically against the
            // column cast to its ISO string form, which is
            // order-equivalent to a real date comparison.
            let (lo, hi) = if dtype.is_temporal() {
                let c = col(&single).cast(DataType::String);
                (c.clone().gt_eq(min.to_lit_expr()), c.lt_eq(max.to_lit_expr()))
            } else {
                let c = col(&single).cast(DataType::Float64);
                (c.clone().gt_eq(min.to_lit_expr()), c.lt_eq(max.to_lit_expr()))
            };
            let pass = if *inclusive {
                lo.and(hi)
            } else {
                let c = if dtype.is_temporal() { col(&single).cast(DataType::String) } else { col(&single).cast(DataType::Float64) };
                c.clone().gt(min.to_lit_expr()).and(c.lt(max.to_lit_expr()))
            };
            simple(col(&single).is_null(), pass)
        }

        // --- Value Set ---
        AllowedValues { values } => {
            let s = values_series("allowed", values);
            simple(col(&single).is_null(), col(&single).is_in(lit(s)))
        }
        DisallowedValues { values } => {
            let s = values_series("disallowed", values);
            simple(col(&single).is_null(), col(&single).is_in(lit(s)).not())
        }
        DistinctCountBetween { min, max } => {
            let n = df.column(&single)?.as_materialized_series().n_unique()? as u64;
            let pass = n >= *min && n <= *max;
            // Same reasoning as `MinimumCompleteness` — a distinct-count
            // check has no natural per-row affected subset.
            Ok(FunctionOutcome { evaluated_count: 1, passed_count: pass as u64, failed_count: (!pass) as u64, affected_columns, affected_rows: None })
        }

        // --- Pattern/Format ---
        // `strict: true` — an invalid regex pattern is a technical/
        // configuration failure of the rule itself, not a per-row
        // business outcome; it must surface as `execution_error` (C.8),
        // not be silently absorbed into per-row nulls.
        RegexMatch { pattern } => {
            let s = col(&single).cast(DataType::String);
            simple(col(&single).is_null(), s.str().contains(lit(pattern.clone()), true))
        }
        RegexNotMatch { pattern } => {
            let s = col(&single).cast(DataType::String);
            simple(col(&single).is_null(), s.str().contains(lit(pattern.clone()), true).not())
        }
        CharacterPatternMatch { allowed_chars } => {
            let pattern = format!("^[{allowed_chars}]*$");
            let s = col(&single).cast(DataType::String);
            simple(col(&single).is_null(), s.str().contains(lit(pattern), true))
        }

        // --- Text Length ---
        MinLength { min } => {
            let len = col(&single).cast(DataType::String).str().len_chars();
            simple(col(&single).is_null(), len.gt_eq(lit(*min)))
        }
        MaxLength { max } => {
            let len = col(&single).cast(DataType::String).str().len_chars();
            simple(col(&single).is_null(), len.lt_eq(lit(*max)))
        }
        LengthBetween { min, max } => {
            let len = col(&single).cast(DataType::String).str().len_chars();
            simple(col(&single).is_null(), len.clone().gt_eq(lit(*min)).and(len.lt_eq(lit(*max))))
        }

        // --- Uniqueness/Duplicates ---
        UniqueColumn => simple(col(&single).is_null(), col(&single).is_duplicated().not()),
        UniqueColumnSet { columns } => {
            let sub = df.select(columns.iter().cloned())?;
            let dup = sub.is_duplicated()?;
            let missing = vec![false; df.height()];
            let pass: Vec<Option<bool>> = dup.into_iter().map(|d| d.map(|d| !d)).collect();
            let (evaluated_count, passed_count, failed_count, failed_mask) = count_outcomes(&missing, &pass, policy);
            let mask_series = BooleanChunked::from_iter_values(PlSmallStr::from_str("__failed"), failed_mask.into_iter());
            let affected_rows = df.filter(&mask_series)?;
            Ok(FunctionOutcome { evaluated_count, passed_count, failed_count, affected_columns: columns.clone(), affected_rows: Some(affected_rows) })
        }
        NoDuplicateRows => {
            let scope = if target.target_columns.is_empty() {
                df.clone()
            } else {
                df.select(target.target_columns.iter().cloned())?
            };
            let dup = scope.is_duplicated()?;
            let missing = vec![false; df.height()];
            let pass: Vec<Option<bool>> = dup.into_iter().map(|d| d.map(|d| !d)).collect();
            let (evaluated_count, passed_count, failed_count, failed_mask) = count_outcomes(&missing, &pass, policy);
            let mask_series = BooleanChunked::from_iter_values(PlSmallStr::from_str("__failed"), failed_mask.into_iter());
            let affected_rows = df.filter(&mask_series)?;
            Ok(FunctionOutcome { evaluated_count, passed_count, failed_count, affected_columns, affected_rows: Some(affected_rows) })
        }

        // --- Cross-Column ---
        ColumnCompare { other_column, op } => {
            let a = col(&single);
            let b = col(other_column);
            let pass = compare_expr(a.clone(), b.clone(), op.clone());
            simple(a.is_null().or(b.is_null()), pass)
        }
        RequiredIf { other_column, other_equals } => {
            let condition_met = col(other_column).eq(other_equals.to_lit_expr());
            let pass = condition_met.clone().not().or(col(&single).is_not_null());
            simple(lit(false), pass)
        }
        MutuallyExclusive { columns } => {
            let present_count = columns
                .iter()
                .map(|c| col(c).is_not_null().cast(DataType::UInt32))
                .reduce(|a, b| a + b)
                .unwrap_or_else(|| lit(0u32));
            simple(lit(false), present_count.lt_eq(lit(1u32)))
        }

        // --- Referential ---
        ExistsInReference { reference_values } => {
            let s = values_series("reference", reference_values);
            simple(col(&single).is_null(), col(&single).is_in(lit(s)))
        }
        NotExistsInReference { reference_values } => {
            let s = values_series("reference", reference_values);
            simple(col(&single).is_null(), col(&single).is_in(lit(s)).not())
        }

        // --- Date/Time Format ---
        DateFormatMatch { format } => {
            let parsed = col(&single)
                .cast(DataType::String)
                .str()
                .to_date(StrptimeOptions { format: Some(PlSmallStr::from_str(format)), strict: false, ..Default::default() });
            simple(col(&single).is_null(), parsed.is_not_null())
        }

        // --- Expression ---
        ExpressionTrue { formula } => {
            let schema = df.schema();
            let compiled = FormulaParser::compile(formula, Some(&schema)).map_err(|e| ValidationError::Request(e.to_string()))?;
            simple(compiled.clone().is_null(), compiled.cast(DataType::Boolean))
        }

        Unknown => unreachable!("dispatched to not_executable before reaching execute_function"),
    }
}

fn compare_expr(a: Expr, b: Expr, op: CompareOp) -> Expr {
    match op {
        CompareOp::Eq => a.eq(b),
        CompareOp::Ne => a.neq(b),
        CompareOp::Gt => a.gt(b),
        CompareOp::Lt => a.lt(b),
        CompareOp::Ge => a.gt_eq(b),
        CompareOp::Le => a.lt_eq(b),
        CompareOp::IsNull => a.is_null(),
        CompareOp::IsNotNull => a.is_not_null(),
        CompareOp::Contains => a.cast(DataType::String).str().contains_literal(b.cast(DataType::String)),
        CompareOp::StartsWith => a.cast(DataType::String).str().starts_with(b.cast(DataType::String)),
        CompareOp::EndsWith => a.cast(DataType::String).str().ends_with(b.cast(DataType::String)),
        CompareOp::Matches => a.cast(DataType::String).str().contains(b.cast(DataType::String), false),
    }
}
