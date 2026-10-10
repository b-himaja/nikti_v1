#[cfg(test)]
mod tests {
    use polars::prelude::*;

    use crate::engines::transformation::formula::FormulaParser;

    fn sample_df() -> DataFrame {
        df![
            "sales" => [100.0, 200.0, 0.0, -50.0],
            "cost" => [10.0, 0.0, 5.0, 25.0],
            "name" => ["Alice", "bob", "CARL", "dee"],
        ]
        .unwrap()
    }

    fn eval(formula: &str) -> DataFrame {
        let df = sample_df();
        let schema = df.schema();
        let expr = FormulaParser::compile(formula, Some(&schema)).expect("formula should compile");
        df.lazy().select([expr.alias("result")]).collect().unwrap()
    }

    #[test]
    fn arithmetic_and_column_refs_happy_path() {
        let out = eval("[sales] - [cost]");
        let col = out.column("result").unwrap().f64().unwrap();
        assert_eq!(col.get(0), Some(90.0));
        assert_eq!(col.get(1), Some(200.0));
    }

    #[test]
    fn division_by_zero_is_null_not_inf_or_error() {
        // Legacy: `pl.when(b != 0).then(a / b).otherwise(pl.lit(None))`
        let out = eval("[sales] / [cost]");
        let col = out.column("result").unwrap().f64().unwrap();
        assert_eq!(col.get(0), Some(10.0));
        assert_eq!(col.get(1), None, "division by zero must be null, not inf");
    }

    #[test]
    fn functions_and_bracket_free_bare_column_ref() {
        let out = eval("ABS(cost - sales)");
        let col = out.column("result").unwrap().f64().unwrap();
        assert_eq!(col.get(0), Some(90.0));
        assert_eq!(col.get(3), Some(75.0));
    }

    #[test]
    fn unknown_column_reference_is_rejected_with_schema() {
        let df = sample_df();
        let schema = df.schema();
        let err = FormulaParser::compile("[nonexistent]", Some(&schema)).unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn mod_function_matches_percent_operator_not_legacy_unimplemented_bug() {
        // Legacy: MOD was listed as supported but had no implementation arm
        // and always raised "Unimplemented function: MOD". We fix this,
        // giving it the same null-safe-modulo semantics as `%`.
        let via_operator = eval("[sales] % 3");
        let via_function = eval("MOD([sales], 3)");
        assert_eq!(
            via_operator.column("result").unwrap().f64().unwrap().to_vec(),
            via_function.column("result").unwrap().f64().unwrap().to_vec()
        );
    }

    #[test]
    fn boolean_logic_and_comparisons() {
        let out = eval("[sales] > 50 AND [cost] < 20");
        let col = out.column("result").unwrap().bool().unwrap();
        assert_eq!(col.get(0), Some(true)); // sales=100>50, cost=10<20
        assert_eq!(col.get(2), Some(false)); // sales=0, fails sales>50
    }

    #[test]
    fn round_requires_literal_second_argument() {
        // Matches legacy: ROUND's decimals arg is read via .meta["value"],
        // which only ever worked for a literal number, never an expression.
        let df = sample_df();
        let schema = df.schema();
        let err = FormulaParser::compile("ROUND([sales], [cost])", Some(&schema)).unwrap_err();
        assert!(err.to_string().contains("literal"));
    }
}
