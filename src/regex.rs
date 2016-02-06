use regex_syntax;
use std::error::Error;
use syntax::ast::Lit_::LitStr;
use syntax::codemap::{Span, BytePos};
use syntax::parse::token::InternedString;
use rustc_front::hir::*;
use rustc::middle::const_eval::{eval_const_expr_partial, ConstVal};
use rustc::middle::const_eval::EvalHint::ExprTypeChecked;
use rustc::lint::*;

use utils::{match_path, REGEX_NEW_PATH, span_lint, span_help_and_lint};

/// **What it does:** This lint checks `Regex::new(_)` invocations for correct regex syntax.
///
/// **Why is this bad?** This will lead to a runtime panic.
///
/// **Known problems:** None.
///
/// **Example:** `Regex::new("|")`
declare_lint! {
    pub INVALID_REGEX,
    Deny,
    "finds invalid regular expressions in `Regex::new(_)` invocations"
}

/// **What it does:** This lint checks for `Regex::new(_)` invocations with trivial regex.
///
/// **Why is this bad?** This can likely be replaced by `==` or `str::starts_with`,
/// `str::ends_with` or `std::contains` or other `str` methods.
///
/// **Known problems:** None.
///
/// **Example:** `Regex::new("^foobar")`
declare_lint! {
    pub TRIVIAL_REGEX,
    Warn,
    "finds trivial regular expressions in `Regex::new(_)` invocations"
}

#[derive(Copy,Clone)]
pub struct RegexPass;

impl LintPass for RegexPass {
    fn get_lints(&self) -> LintArray {
        lint_array!(INVALID_REGEX, TRIVIAL_REGEX)
    }
}

impl LateLintPass for RegexPass {
    fn check_expr(&mut self, cx: &LateContext, expr: &Expr) {
        if_let_chain!{[
            let ExprCall(ref fun, ref args) = expr.node,
            let ExprPath(_, ref path) = fun.node,
            match_path(path, &REGEX_NEW_PATH) && args.len() == 1
        ], {
            if let ExprLit(ref lit) = args[0].node {
                if let LitStr(ref r, _) = lit.node {
                    match regex_syntax::Expr::parse(r) {
                        Ok(r) => {
                            if let Some(repl) = is_trivial_regex(&r) {
                                span_help_and_lint(cx, TRIVIAL_REGEX, args[0].span,
                                                   &"trivial regex",
                                                   &format!("consider using {}", repl));
                            }
                        }
                        Err(e) => {
                            span_lint(cx,
                                      INVALID_REGEX,
                                      str_span(args[0].span, &r, e.position()),
                                      &format!("regex syntax error: {}",
                                               e.description()));
                        }
                    }
                }
            } else if let Some(r) = const_str(cx, &*args[0]) {
                match regex_syntax::Expr::parse(&r) {
                    Ok(r) => {
                        if let Some(repl) = is_trivial_regex(&r) {
                            span_help_and_lint(cx, TRIVIAL_REGEX, args[0].span,
                                               &"trivial regex",
                                               &format!("consider using {}", repl));
                        }
                    }
                    Err(e) => {
                        span_lint(cx,
                                  INVALID_REGEX,
                                  args[0].span,
                                  &format!("regex syntax error on position {}: {}",
                                           e.position(),
                                           e.description()));
                    }
                }
            }
        }}
    }
}

#[allow(cast_possible_truncation)]
fn str_span(base: Span, s: &str, c: usize) -> Span {
    let lo = match s.char_indices().nth(c) {
        Some((b, _)) => base.lo + BytePos(b as u32),
        _ => base.hi
    };
    Span{ lo: lo, hi: lo, ..base }
}

fn const_str(cx: &LateContext, e: &Expr) -> Option<InternedString> {
    match eval_const_expr_partial(cx.tcx, e, ExprTypeChecked, None) {
        Ok(ConstVal::Str(r)) => Some(r),
        _ => None
    }
}

fn is_trivial_regex(s: &regex_syntax::Expr) -> Option<&'static str> {
    use regex_syntax::Expr;

    match *s {
        Expr::Empty | Expr::StartText | Expr::EndText => Some("the regex is unlikely to be useful as it is"),
        Expr::Literal {..} => Some("consider using `str::contains`"),
        Expr::Concat(ref exprs) => {
            match exprs.len() {
                2 => match (&exprs[0], &exprs[1]) {
                    (&Expr::StartText, &Expr::EndText) => Some("consider using `str::is_empty`"),
                    (&Expr::StartText, &Expr::Literal {..}) => Some("consider using `str::starts_with`"),
                    (&Expr::Literal {..}, &Expr::EndText) => Some("consider using `str::ends_with`"),
                    _ => None,
                },
                3 => {
                    if let (&Expr::StartText, &Expr::Literal {..}, &Expr::EndText) = (&exprs[0], &exprs[1], &exprs[2]) {
                        Some("consider using `==` on `str`s")
                    }
                    else {
                        None
                    }
                },
                _ => None,
            }
        }
        _ => None,
    }
}