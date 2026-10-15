//! Parameter expressions: `60`, `L*2`, `(width - 2*wall) / 2`, `25.4 in`, `30 deg`, `sqrt(a^2+b^2)`.
//!
//! Lengths are millimetres, angles degrees (Fusion's defaults). Unit suffixes convert into those:
//! `mm cm m in ft` for lengths, `deg rad` for angles. Identifiers refer to named parameters.
//! Input is hostile: length and nesting are capped, results must be finite.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const MAX_LEN: usize = 512;
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ExprError {
    #[error("empty expression")]
    Empty,
    #[error("expression is too long")]
    TooLong,
    #[error("unexpected `{0}` at {1}")]
    Unexpected(String, usize),
    #[error("unknown parameter `{0}`")]
    Unknown(String),
    #[error("unknown function `{0}`")]
    UnknownFunction(String),
    #[error("result is not a finite number")]
    NotFinite,
    #[error("expression is nested too deeply")]
    TooDeep,
    #[error("circular reference through `{0}`")]
    Cycle(String),
}

/// An expression as the user typed it. Stored verbatim so edits keep their formulas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Expr(pub String);

impl Expr {
    pub fn num(v: f64) -> Self {
        Expr(fmt_num(v))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn eval(&self, vars: &dyn Fn(&str) -> Option<f64>) -> Result<f64, ExprError> {
        eval(&self.0, vars)
    }
    /// Parameter names the expression refers to.
    pub fn references(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Ok(toks) = lex(&self.0) {
            for (t, _) in toks {
                if let Tok::Ident(name) = t
                    && !is_function(&name)
                    && unit_factor(&name).is_none()
                    && !matches!(name.as_str(), "pi" | "e")
                    && !out.contains(&name)
                {
                    out.push(name);
                }
            }
        }
        out
    }
}

impl From<f64> for Expr {
    fn from(v: f64) -> Self {
        Expr::num(v)
    }
}

impl From<&str> for Expr {
    fn from(s: &str) -> Self {
        Expr(s.to_string())
    }
}

/// Shortest round-trippable formatting without a trailing `.0`.
pub fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 { format!("{}", v as i64) } else { format!("{v}") }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Op(char),
}

fn lex(s: &str) -> Result<Vec<(Tok, usize)>, ExprError> {
    if s.len() > MAX_LEN {
        return Err(ExprError::TooLong);
    }
    let b = s.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b.get(i).copied().unwrap_or(b' ');
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            let start = i;
            while b.get(i).is_some_and(|c| c.is_ascii_digit() || *c == b'.') {
                i += 1;
            }
            // exponent: 1e-3
            if b.get(i).is_some_and(|c| *c == b'e' || *c == b'E')
                && (b.get(i + 1).is_some_and(u8::is_ascii_digit)
                    || (b.get(i + 1).is_some_and(|c| *c == b'-' || *c == b'+')
                        && b.get(i + 2).is_some_and(u8::is_ascii_digit)))
            {
                i += 2;
                while b.get(i).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                }
            }
            let text = s.get(start..i).unwrap_or("");
            let v: f64 = text.parse().map_err(|_| ExprError::Unexpected(text.into(), start))?;
            out.push((Tok::Num(v), start));
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while b.get(i).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_') {
                i += 1;
            }
            out.push((Tok::Ident(s.get(start..i).unwrap_or("").to_string()), start));
        } else if b"+-*/^(),".contains(&c) {
            out.push((Tok::Op(c as char), i));
            i += 1;
        } else {
            let ch = s.get(i..).and_then(|r| r.chars().next()).unwrap_or('?');
            return Err(ExprError::Unexpected(ch.to_string(), i));
        }
    }
    Ok(out)
}

fn is_function(name: &str) -> bool {
    matches!(
        name,
        "sqrt" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "abs" | "min" | "max" | "round" | "floor" | "ceil"
    )
}

/// Factor converting a unit suffix into mm (lengths) or degrees (angles).
fn unit_factor(name: &str) -> Option<f64> {
    Some(match name {
        "mm" => 1.0,
        "cm" => 10.0,
        "m" => 1000.0,
        "in" => 25.4,
        "ft" => 304.8,
        "deg" => 1.0,
        "rad" => 180.0 / std::f64::consts::PI,
        _ => return None,
    })
}

struct Parser<'a> {
    toks: Vec<(Tok, usize)>,
    pos: usize,
    vars: &'a dyn Fn(&str) -> Option<f64>,
    depth: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.0)
    }
    fn at(&self) -> usize {
        self.toks.get(self.pos).map(|t| t.1).unwrap_or(usize::MAX)
    }
    fn unexpected(&self) -> ExprError {
        match self.toks.get(self.pos) {
            Some((t, at)) => ExprError::Unexpected(
                match t {
                    Tok::Num(v) => fmt_num(*v),
                    Tok::Ident(s) => s.clone(),
                    Tok::Op(c) => c.to_string(),
                },
                *at,
            ),
            None => ExprError::Unexpected("end of input".into(), self.at()),
        }
    }
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(&Tok::Op(c)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn enter(&mut self) -> Result<(), ExprError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH { Err(ExprError::TooDeep) } else { Ok(()) }
    }

    // expr := term (('+'|'-') term)*
    fn expr(&mut self) -> Result<f64, ExprError> {
        self.enter()?;
        let mut v = self.term()?;
        loop {
            if self.eat('+') {
                v += self.term()?;
            } else if self.eat('-') {
                v -= self.term()?;
            } else {
                break;
            }
        }
        self.depth -= 1;
        Ok(v)
    }

    // term := unary (('*'|'/') unary)*
    fn term(&mut self) -> Result<f64, ExprError> {
        let mut v = self.unary()?;
        loop {
            if self.eat('*') {
                v *= self.unary()?;
            } else if self.eat('/') {
                v /= self.unary()?;
            } else {
                break;
            }
        }
        Ok(v)
    }

    // unary := ('-'|'+') unary | power
    fn unary(&mut self) -> Result<f64, ExprError> {
        self.enter()?;
        let v = if self.eat('-') {
            -self.unary()?
        } else if self.eat('+') {
            self.unary()?
        } else {
            self.power()?
        };
        self.depth -= 1;
        Ok(v)
    }

    // power := postfix ('^' unary)?
    fn power(&mut self) -> Result<f64, ExprError> {
        let base = self.postfix()?;
        if self.eat('^') { Ok(base.powf(self.unary()?)) } else { Ok(base) }
    }

    // postfix := atom unit?
    fn postfix(&mut self) -> Result<f64, ExprError> {
        let v = self.atom()?;
        if let Some(Tok::Ident(name)) = self.peek()
            && let Some(f) = unit_factor(name)
        {
            self.pos += 1;
            return Ok(v * f);
        }
        Ok(v)
    }

    fn atom(&mut self) -> Result<f64, ExprError> {
        match self.peek().cloned() {
            Some(Tok::Num(v)) => {
                self.pos += 1;
                Ok(v)
            }
            Some(Tok::Op('(')) => {
                self.pos += 1;
                let v = self.expr()?;
                if !self.eat(')') {
                    return Err(self.unexpected());
                }
                Ok(v)
            }
            Some(Tok::Ident(name)) => {
                self.pos += 1;
                if is_function(&name) && self.eat('(') {
                    let mut args = vec![self.expr()?];
                    while self.eat(',') {
                        args.push(self.expr()?);
                    }
                    if !self.eat(')') {
                        return Err(self.unexpected());
                    }
                    return call(&name, &args);
                }
                match name.as_str() {
                    "pi" => Ok(std::f64::consts::PI),
                    "e" => Ok(std::f64::consts::E),
                    _ => (self.vars)(&name).ok_or(ExprError::Unknown(name)),
                }
            }
            _ => Err(self.unexpected()),
        }
    }
}

fn call(name: &str, a: &[f64]) -> Result<f64, ExprError> {
    let one = |f: fn(f64) -> f64| match a {
        [x] => Ok(f(*x)),
        _ => Err(ExprError::Unexpected(format!("{name} takes 1 argument"), 0)),
    };
    // Trigonometry works in degrees, like the rest of the UI.
    match name {
        "sqrt" => one(f64::sqrt),
        "abs" => one(f64::abs),
        "round" => one(f64::round),
        "floor" => one(f64::floor),
        "ceil" => one(f64::ceil),
        "sin" => one(|x| x.to_radians().sin()),
        "cos" => one(|x| x.to_radians().cos()),
        "tan" => one(|x| x.to_radians().tan()),
        "asin" => one(|x| x.asin().to_degrees()),
        "acos" => one(|x| x.acos().to_degrees()),
        "atan" => one(|x| x.atan().to_degrees()),
        "min" if !a.is_empty() => Ok(a.iter().copied().fold(f64::INFINITY, f64::min)),
        "max" if !a.is_empty() => Ok(a.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
        _ => Err(ExprError::UnknownFunction(name.into())),
    }
}

/// Evaluate an expression; `vars` resolves parameter names.
pub fn eval(src: &str, vars: &dyn Fn(&str) -> Option<f64>) -> Result<f64, ExprError> {
    let toks = lex(src)?;
    if toks.is_empty() {
        return Err(ExprError::Empty);
    }
    let mut p = Parser { toks, pos: 0, vars, depth: 0 };
    let v = p.expr()?;
    if p.pos < p.toks.len() {
        return Err(p.unexpected());
    }
    if v.is_finite() { Ok(v) } else { Err(ExprError::NotFinite) }
}

/// Evaluate a set of named parameters that may refer to each other (any order), detecting
/// cycles. Returns the value of every parameter or the first error with the parameter's name.
pub fn eval_all(params: &[(String, Expr)]) -> Result<HashMap<String, f64>, (String, ExprError)> {
    fn visit(
        name: &str,
        params: &[(String, Expr)],
        done: &mut HashMap<String, f64>,
        stack: &mut Vec<String>,
    ) -> Result<f64, (String, ExprError)> {
        if let Some(v) = done.get(name) {
            return Ok(*v);
        }
        if stack.iter().any(|s| s == name) {
            return Err((name.into(), ExprError::Cycle(name.into())));
        }
        if stack.len() > MAX_DEPTH {
            return Err((name.into(), ExprError::TooDeep));
        }
        let Some((_, e)) = params.iter().find(|(n, _)| n == name) else {
            return Err((name.into(), ExprError::Unknown(name.into())));
        };
        stack.push(name.into());
        let mut deps = HashMap::new();
        for r in e.references() {
            deps.insert(r.clone(), visit(&r, params, done, stack)?);
        }
        stack.pop();
        let v = e.eval(&|n| deps.get(n).copied()).map_err(|err| (name.to_string(), err))?;
        done.insert(name.into(), v);
        Ok(v)
    }
    let mut done = HashMap::new();
    for (n, _) in params {
        visit(n, params, &mut done, &mut Vec::new())?;
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(s: &str) -> Result<f64, ExprError> {
        eval(s, &|n| match n {
            "L" => Some(60.0),
            "W" => Some(40.0),
            _ => None,
        })
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(ev("1 + 2 * 3").unwrap(), 7.0);
        assert_eq!(ev("(1 + 2) * 3").unwrap(), 9.0);
        assert_eq!(ev("2 ^ 3 ^ 2").unwrap(), 512.0);
        assert_eq!(ev("-2 ^ 2").unwrap(), -4.0);
        assert_eq!(ev("L*2").unwrap(), 120.0);
        assert_eq!(ev("(L - W) / 2").unwrap(), 10.0);
        assert_eq!(ev("1e3 + .5").unwrap(), 1000.5);
    }

    #[test]
    fn units_and_functions() {
        assert_eq!(ev("1 in").unwrap(), 25.4);
        assert_eq!(ev("2 cm + 5 mm").unwrap(), 25.0);
        assert!((ev("pi rad").unwrap() - 180.0).abs() < 1e-9);
        assert!((ev("sin(30)").unwrap() - 0.5).abs() < 1e-12);
        assert_eq!(ev("sqrt(3^2 + 4^2)").unwrap(), 5.0);
        assert_eq!(ev("max(1, L, 3)").unwrap(), 60.0);
    }

    #[test]
    fn errors_are_reported() {
        assert_eq!(ev(""), Err(ExprError::Empty));
        assert_eq!(ev("X + 1"), Err(ExprError::Unknown("X".into())));
        assert!(matches!(ev("1 +"), Err(ExprError::Unexpected(..))));
        assert!(matches!(ev("(1"), Err(ExprError::Unexpected(..))));
        assert!(matches!(ev("1 $ 2"), Err(ExprError::Unexpected(..))));
        assert_eq!(ev("1/0"), Err(ExprError::NotFinite));
        assert_eq!(ev("foo(1)"), Err(ExprError::Unknown("foo".into())));
        assert_eq!(ev(&"1+".repeat(400)), Err(ExprError::TooLong));
        assert_eq!(ev(&format!("{}1{}", "(".repeat(100), ")".repeat(100))), Err(ExprError::TooDeep));
        assert_eq!(ev(&"-".repeat(200)).unwrap_err(), ExprError::TooDeep);
    }

    #[test]
    fn references_skip_units_and_functions() {
        let e = Expr::from("sqrt(L^2 + W^2) + 2 mm + pi");
        assert_eq!(e.references(), vec!["L".to_string(), "W".to_string()]);
    }

    #[test]
    fn parameter_table_resolves_in_any_order_and_finds_cycles() {
        let p = vec![("b".to_string(), Expr::from("a * 2")), ("a".to_string(), Expr::from("10"))];
        let v = eval_all(&p).unwrap();
        assert_eq!(v["b"], 20.0);
        let cyc = vec![("a".to_string(), Expr::from("b + 1")), ("b".to_string(), Expr::from("a"))];
        assert!(matches!(eval_all(&cyc), Err((_, ExprError::Cycle(_)))));
    }

    #[test]
    fn number_formatting() {
        assert_eq!(fmt_num(60.0), "60");
        assert_eq!(fmt_num(2.5), "2.5");
        assert_eq!(Expr::num(-3.0).0, "-3");
    }
}
