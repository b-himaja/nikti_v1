//! Native Rust port of `legacy/datawrangler/python/engine/formula_parser.py`.
//!
//! Hand-written recursive-descent parser, not a new grammar-crate
//! dependency — the legacy grammar (arithmetic, comparisons, boolean
//! logic, a fixed function whitelist, column refs, literals) is small
//! enough not to need one. Compiles directly to `polars::prelude::Expr`,
//! matching the legacy `FormulaTransformer`'s behavior term-for-term,
//! including its null-safe division/modulo and literal-only second
//! arguments for ROUND/LOG/PCTCHANGE (the legacy parser reads those via
//! `.meta["value"]` straight off the parse tree, so only a literal number
//! ever worked there — not a general expression).
//!
//! Deliberate deviation from the legacy grammar (flagged, not silent):
//! the legacy `CNAME` terminal allows embedded spaces in a *bare* column
//! reference (`sales tax` without brackets), which collides ambiguously
//! with the `AND`/`OR`/`NOT` keywords and is only resolved by Lark's
//! Earley `ambiguity="resolve"` heuristics — not independently
//! verifiable without Lark itself. This parser requires brackets
//! (`[sales tax]`) for any column name containing a space or matching a
//! reserved word; bare identifiers are restricted to
//! `[a-zA-Z_][a-zA-Z0-9_]*`. Every other construct matches the legacy
//! grammar.
//!
//! Also fixes one legacy bug rather than replicating it: `MOD` is listed
//! in `SUPPORTED_FUNCTIONS` but has no matching arm in the legacy
//! `func_call` if/elif chain, so calling `MOD(a, b)` in the legacy parser
//! always raised "Unimplemented function: MOD" despite being advertised
//! as supported. The `%` operator's null-safe-modulo logic is already
//! implemented and unambiguous, so `MOD(a, b)` here is wired to the same
//! logic instead of reproducing the dead end.

use std::iter::Peekable;
use std::str::Chars;

use polars::prelude::*;

use super::errors::TransformError;

const SUPPORTED_FUNCTIONS: &[&str] = &[
    "ABS", "ROUND", "CEIL", "FLOOR", "SQRT", "LOG", "EXP", "CUMSUM", "CUMPROD", "RANK",
    "PCTCHANGE", "ZSCORE", "NORMALISE", "IF", "MOD", "LEN", "UPPER", "LOWER",
];

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    String(String),
    Ident(String),
    Symbol(&'static str),
    Eof,
}

struct Lexer<'a> {
    chars: Peekable<Chars<'a>>,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self { chars: src.chars().peekable() }
    }

    fn tokenize(mut self) -> Result<Vec<Token>, TransformError> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace();
            let Some(&c) = self.chars.peek() else {
                tokens.push(Token::Eof);
                break;
            };

            if c.is_ascii_digit() || (c == '.' && self.peek_is_digit_after_dot()) {
                tokens.push(self.read_number()?);
            } else if c == '"' || c == '\'' {
                tokens.push(self.read_string(c)?);
            } else if c == '[' {
                self.chars.next();
                let name = self.read_until(']')?;
                self.chars.next(); // consume ']'
                tokens.push(Token::Ident(name));
            } else if c.is_alphabetic() || c == '_' {
                tokens.push(self.read_bare_ident());
            } else {
                tokens.push(self.read_symbol()?);
            }
        }
        Ok(tokens)
    }

    fn peek_is_digit_after_dot(&self) -> bool {
        let mut clone = self.chars.clone();
        clone.next();
        matches!(clone.peek(), Some(c) if c.is_ascii_digit())
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.chars.peek(), Some(c) if c.is_whitespace()) {
            self.chars.next();
        }
    }

    fn read_number(&mut self) -> Result<Token, TransformError> {
        let mut s = String::new();
        while matches!(self.chars.peek(), Some(c) if c.is_ascii_digit() || *c == '.') {
            s.push(self.chars.next().unwrap());
        }
        s.parse::<f64>()
            .map(Token::Number)
            .map_err(|_| TransformError::Formula(format!("invalid number literal: {s}")))
    }

    fn read_string(&mut self, quote: char) -> Result<Token, TransformError> {
        self.chars.next(); // opening quote
        let mut s = String::new();
        loop {
            match self.chars.next() {
                Some(c) if c == quote => break,
                Some(c) => s.push(c),
                None => return Err(TransformError::Formula("unterminated string literal".into())),
            }
        }
        Ok(Token::String(s))
    }

    fn read_until(&mut self, end: char) -> Result<String, TransformError> {
        let mut s = String::new();
        loop {
            match self.chars.peek() {
                Some(&c) if c == end => break,
                Some(&c) => {
                    s.push(c);
                    self.chars.next();
                }
                None => return Err(TransformError::Formula(format!("unterminated '[' — expected '{end}'"))),
            }
        }
        Ok(s.trim().to_string())
    }

    fn read_bare_ident(&mut self) -> Token {
        let mut s = String::new();
        while matches!(self.chars.peek(), Some(c) if c.is_alphanumeric() || *c == '_') {
            s.push(self.chars.next().unwrap());
        }
        Token::Ident(s)
    }

    fn read_symbol(&mut self) -> Result<Token, TransformError> {
        let c = self.chars.next().unwrap();
        let two = |lexer: &mut Self, expect: char, two_sym: &'static str, one_sym: &'static str| {
            if lexer.chars.peek() == Some(&expect) {
                lexer.chars.next();
                two_sym
            } else {
                one_sym
            }
        };
        let sym = match c {
            '+' => "+",
            '-' => "-",
            '*' => two(self, '*', "**", "*"),
            '/' => "/",
            '%' => "%",
            '(' => "(",
            ')' => ")",
            ',' => ",",
            '>' => two(self, '=', ">=", ">"),
            '<' => two(self, '=', "<=", "<"),
            '=' => {
                if self.chars.peek() == Some(&'=') {
                    self.chars.next();
                    "=="
                } else {
                    return Err(TransformError::Formula("unexpected '=' — did you mean '=='?".into()));
                }
            }
            '!' => {
                if self.chars.peek() == Some(&'=') {
                    self.chars.next();
                    "!="
                } else {
                    return Err(TransformError::Formula("unexpected '!' — did you mean '!='?".into()));
                }
            }
            other => return Err(TransformError::Formula(format!("unexpected character '{other}'"))),
        };
        Ok(Token::Symbol(sym))
    }
}

pub struct FormulaParser<'a> {
    tokens: Vec<Token>,
    pos: usize,
    schema: Option<&'a Schema>,
}

impl<'a> FormulaParser<'a> {
    /// Parses `formula` and compiles it to a `polars::prelude::Expr`.
    /// `schema`, if given, is used to reject references to unknown
    /// columns at compile time (matching the legacy parser's behavior
    /// when a schema is supplied).
    pub fn compile(formula: &str, schema: Option<&'a Schema>) -> Result<Expr, TransformError> {
        let tokens = Lexer::new(formula).tokenize()?;
        let mut parser = FormulaParser { tokens, pos: 0, schema };
        let expr = parser.parse_or()?;
        parser.expect_eof()?;
        Ok(expr)
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn advance(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn expect_symbol(&mut self, sym: &str) -> Result<(), TransformError> {
        match self.peek() {
            Token::Symbol(s) if *s == sym => {
                self.advance();
                Ok(())
            }
            other => Err(TransformError::Formula(format!("expected '{sym}', found {other:?}"))),
        }
    }

    fn expect_eof(&self) -> Result<(), TransformError> {
        match self.peek() {
            Token::Eof => Ok(()),
            other => Err(TransformError::Formula(format!("unexpected trailing input: {other:?}"))),
        }
    }

    fn match_ident_keyword(&mut self, kw: &str) -> bool {
        if let Token::Ident(name) = self.peek() {
            if name.eq_ignore_ascii_case(kw) {
                self.advance();
                return true;
            }
        }
        false
    }

    // or_expr: and_expr (OR and_expr)*
    fn parse_or(&mut self) -> Result<Expr, TransformError> {
        let mut lhs = self.parse_and()?;
        while self.match_ident_keyword("OR") {
            let rhs = self.parse_and()?;
            lhs = lhs.or(rhs);
        }
        Ok(lhs)
    }

    // and_expr: not_expr (AND not_expr)*
    fn parse_and(&mut self) -> Result<Expr, TransformError> {
        let mut lhs = self.parse_not()?;
        while self.match_ident_keyword("AND") {
            let rhs = self.parse_not()?;
            lhs = lhs.and(rhs);
        }
        Ok(lhs)
    }

    // not_expr: NOT not_expr | compare
    fn parse_not(&mut self) -> Result<Expr, TransformError> {
        if self.match_ident_keyword("NOT") {
            let inner = self.parse_not()?;
            return Ok(inner.not());
        }
        self.parse_compare()
    }

    // compare: expr ((>= | <= | > | < | == | !=) expr)?
    fn parse_compare(&mut self) -> Result<Expr, TransformError> {
        let lhs = self.parse_expr()?;
        let op = match self.peek() {
            Token::Symbol(">=") => Some(">="),
            Token::Symbol("<=") => Some("<="),
            Token::Symbol(">") => Some(">"),
            Token::Symbol("<") => Some("<"),
            Token::Symbol("==") => Some("=="),
            Token::Symbol("!=") => Some("!="),
            _ => None,
        };
        let Some(op) = op else { return Ok(lhs) };
        self.advance();
        let rhs = self.parse_expr()?;
        Ok(match op {
            ">=" => lhs.gt_eq(rhs),
            "<=" => lhs.lt_eq(rhs),
            ">" => lhs.gt(rhs),
            "<" => lhs.lt(rhs),
            "==" => lhs.eq(rhs),
            "!=" => lhs.neq(rhs),
            _ => unreachable!(),
        })
    }

    // expr: term ((+ | -) term)*
    fn parse_expr(&mut self) -> Result<Expr, TransformError> {
        let mut lhs = self.parse_term()?;
        loop {
            match self.peek() {
                Token::Symbol("+") => {
                    self.advance();
                    lhs = lhs + self.parse_term()?;
                }
                Token::Symbol("-") => {
                    self.advance();
                    lhs = lhs - self.parse_term()?;
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    // term: factor ((* | / | %) factor)*
    fn parse_term(&mut self) -> Result<Expr, TransformError> {
        let mut lhs = self.parse_factor()?;
        loop {
            match self.peek() {
                Token::Symbol("*") => {
                    self.advance();
                    lhs = lhs * self.parse_factor()?;
                }
                Token::Symbol("/") => {
                    self.advance();
                    let rhs = self.parse_factor()?;
                    // Null-safe division: legacy returns null instead of
                    // inf/NaN/error when the denominator is zero or null.
                    lhs = when(rhs.clone().neq(lit(0)))
                        .then(lhs / rhs)
                        .otherwise(lit(NULL));
                }
                Token::Symbol("%") => {
                    self.advance();
                    let rhs = self.parse_factor()?;
                    lhs = null_safe_mod(lhs, rhs);
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    // factor: power
    // power: unary (** power)?   (right-associative, matches legacy)
    fn parse_factor(&mut self) -> Result<Expr, TransformError> {
        let base = self.parse_unary()?;
        if matches!(self.peek(), Token::Symbol("**")) {
            self.advance();
            let exponent = self.parse_factor()?;
            return Ok(base.pow(exponent));
        }
        Ok(base)
    }

    // unary: - atom | atom
    fn parse_unary(&mut self) -> Result<Expr, TransformError> {
        if matches!(self.peek(), Token::Symbol("-")) {
            self.advance();
            let inner = self.parse_atom()?;
            return Ok(-inner);
        }
        self.parse_atom()
    }

    fn parse_atom(&mut self) -> Result<Expr, TransformError> {
        match self.advance() {
            Token::Number(n) => Ok(number_literal(n)),
            Token::String(s) => Ok(lit(s)),
            Token::Symbol("(") => {
                let inner = self.parse_or()?;
                self.expect_symbol(")")?;
                Ok(inner)
            }
            Token::Ident(name) => {
                if matches!(self.peek(), Token::Symbol("(")) {
                    self.parse_func_call(name)
                } else {
                    self.col_ref(&name)
                }
            }
            other => Err(TransformError::Formula(format!("unexpected token: {other:?}"))),
        }
    }

    fn col_ref(&self, name: &str) -> Result<Expr, TransformError> {
        if let Some(schema) = self.schema {
            if schema.get(name).is_none() {
                let available: Vec<&str> = schema.iter_names().map(|s| s.as_str()).collect();
                return Err(TransformError::Formula(format!(
                    "Column '{name}' does not exist. Available columns: {}",
                    available.join(", ")
                )));
            }
        }
        Ok(col(name))
    }

    fn parse_args(&mut self) -> Result<Vec<Expr>, TransformError> {
        self.expect_symbol("(")?;
        let mut args = Vec::new();
        if !matches!(self.peek(), Token::Symbol(")")) {
            args.push(self.parse_or()?);
            while matches!(self.peek(), Token::Symbol(",")) {
                self.advance();
                args.push(self.parse_or()?);
            }
        }
        self.expect_symbol(")")?;
        Ok(args)
    }

    /// Parses a call whose second argument (if present) must be a bare
    /// numeric literal — matches the legacy parser's use of
    /// `.meta["value"]`, which only ever worked for a literal NUMBER
    /// token in the parse tree, never a general expression.
    fn parse_func_call(&mut self, name: String) -> Result<Expr, TransformError> {
        let fname = name.to_uppercase();
        if !SUPPORTED_FUNCTIONS.contains(&fname.as_str()) {
            return Err(TransformError::Formula(format!(
                "Function '{fname}' is not supported. Supported: {}",
                SUPPORTED_FUNCTIONS.join(", ")
            )));
        }
        let args = self.parse_args()?;

        let arg_count_error = |min: usize| {
            TransformError::Formula(format!("{fname} requires at least {min} argument(s)"))
        };

        Ok(match fname.as_str() {
            "ABS" => args.first().ok_or_else(|| arg_count_error(1))?.clone().abs(),
            "ROUND" => {
                let decimals = literal_u32(args.get(1))?.unwrap_or(0);
                args.first().ok_or_else(|| arg_count_error(1))?.clone().round(decimals)
            }
            "CEIL" => args.first().ok_or_else(|| arg_count_error(1))?.clone().ceil(),
            "FLOOR" => args.first().ok_or_else(|| arg_count_error(1))?.clone().floor(),
            "SQRT" => args.first().ok_or_else(|| arg_count_error(1))?.clone().sqrt(),
            "LOG" => {
                let base = literal_f64(args.get(1))?.unwrap_or(10.0);
                args.first().ok_or_else(|| arg_count_error(1))?.clone().log(base)
            }
            "EXP" => args.first().ok_or_else(|| arg_count_error(1))?.clone().exp(),
            "CUMSUM" => args.first().ok_or_else(|| arg_count_error(1))?.clone().cum_sum(false),
            "CUMPROD" => args.first().ok_or_else(|| arg_count_error(1))?.clone().cum_prod(false),
            "RANK" => args
                .first()
                .ok_or_else(|| arg_count_error(1))?
                .clone()
                .rank(RankOptions::default(), None),
            "PCTCHANGE" => {
                let n = literal_i64(args.get(1))?.unwrap_or(1);
                args.first().ok_or_else(|| arg_count_error(1))?.clone().pct_change(lit(n))
            }
            "ZSCORE" => {
                let c = args.first().ok_or_else(|| arg_count_error(1))?.clone();
                (c.clone() - c.clone().mean()) / c.std(1)
            }
            "NORMALISE" => {
                let c = args.first().ok_or_else(|| arg_count_error(1))?.clone();
                (c.clone() - c.clone().min()) / (c.clone().max() - c.min())
            }
            "IF" => {
                if args.len() < 3 {
                    return Err(arg_count_error(3));
                }
                when(args[0].clone()).then(args[1].clone()).otherwise(args[2].clone())
            }
            "MOD" => {
                if args.len() < 2 {
                    return Err(arg_count_error(2));
                }
                null_safe_mod(args[0].clone(), args[1].clone())
            }
            "LEN" => args.first().ok_or_else(|| arg_count_error(1))?.clone().str().len_chars(),
            "UPPER" => args
                .first()
                .ok_or_else(|| arg_count_error(1))?
                .clone()
                .str()
                .to_uppercase(),
            "LOWER" => args
                .first()
                .ok_or_else(|| arg_count_error(1))?
                .clone()
                .str()
                .to_lowercase(),
            _ => unreachable!("checked against SUPPORTED_FUNCTIONS above"),
        })
    }
}

fn null_safe_mod(a: Expr, b: Expr) -> Expr {
    when(b.clone().neq(lit(0))).then(a % b).otherwise(lit(NULL))
}

/// Matches the legacy `number()` transformer: a whole-valued literal
/// becomes an integer literal, otherwise a float literal.
fn number_literal(n: f64) -> Expr {
    if n.fract() == 0.0 && n.abs() < i64::MAX as f64 {
        lit(n as i64)
    } else {
        lit(n)
    }
}

fn literal_f64(expr: Option<&Expr>) -> Result<Option<f64>, TransformError> {
    let Some(expr) = expr else { return Ok(None) };
    match expr {
        Expr::Literal(lv) => lv
            .to_any_value()
            .and_then(|av| av.extract::<f64>())
            .map(Some)
            .ok_or_else(|| TransformError::Formula("expected a literal number argument".into())),
        _ => Err(TransformError::Formula(
            "this argument must be a literal number, not an expression".into(),
        )),
    }
}

fn literal_i64(expr: Option<&Expr>) -> Result<Option<i64>, TransformError> {
    Ok(literal_f64(expr)?.map(|f| f as i64))
}

fn literal_u32(expr: Option<&Expr>) -> Result<Option<u32>, TransformError> {
    Ok(literal_f64(expr)?.map(|f| f as u32))
}
