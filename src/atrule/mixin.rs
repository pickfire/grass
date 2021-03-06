use std::mem;
use std::vec::IntoIter;

use codemap::{Span, Spanned};

use peekmore::{PeekMore, PeekMoreIterator};

use super::ruleset_eval;

use crate::args::{eat_call_args, eat_func_args, CallArgs, FuncArgs};
use crate::atrule::AtRule;
use crate::error::SassResult;
use crate::scope::Scope;
use crate::selector::Selector;
use crate::utils::{
    devour_whitespace, devour_whitespace_or_comment, eat_ident, read_until_closing_curly_brace,
};
use crate::value::Value;
use crate::{eat_expr, Expr, RuleSet, Stmt, Token};

#[derive(Debug, Clone)]
pub(crate) struct Mixin {
    scope: Scope,
    args: FuncArgs,
    body: PeekMoreIterator<IntoIter<Token>>,
}

impl Mixin {
    pub fn new(scope: Scope, args: FuncArgs, body: Vec<Token>) -> Self {
        let body = body.into_iter().peekmore();
        Mixin { scope, args, body }
    }

    pub fn decl_from_tokens<I: Iterator<Item = Token>>(
        toks: &mut PeekMoreIterator<I>,
        scope: &Scope,
        super_selector: &Selector,
        span_before: Span,
    ) -> SassResult<Spanned<(String, Mixin)>> {
        devour_whitespace(toks);
        let Spanned { node: name, span } = eat_ident(toks, scope, super_selector, span_before)?;
        devour_whitespace(toks);
        let args = match toks.next() {
            Some(Token { kind: '(', .. }) => eat_func_args(toks, scope, super_selector)?,
            Some(Token { kind: '{', .. }) => FuncArgs::new(),
            Some(t) => return Err(("expected \"{\".", t.pos()).into()),
            None => return Err(("expected \"{\".", span).into()),
        };

        devour_whitespace(toks);

        let mut body = read_until_closing_curly_brace(toks)?;
        body.push(toks.next().unwrap());

        Ok(Spanned {
            node: (name, Mixin::new(scope.clone(), args, body)),
            span,
        })
    }

    pub fn args(
        mut self,
        mut args: CallArgs,
        scope: &Scope,
        super_selector: &Selector,
    ) -> SassResult<Mixin> {
        for (idx, arg) in self.args.0.iter_mut().enumerate() {
            if arg.is_variadic {
                let span = args.span();
                self.scope.insert_var(
                    mem::take(&mut arg.name),
                    Spanned {
                        node: Value::ArgList(args.get_variadic(scope, super_selector)?),
                        span,
                    },
                )?;
                break;
            }
            let val = match args.get(idx, arg.name.clone(), scope, super_selector) {
                Some(v) => v?,
                None => match arg.default.as_mut() {
                    Some(v) => Value::from_vec(mem::take(v), scope, super_selector, args.span())?,
                    None => {
                        return Err(
                            (format!("Missing argument ${}.", &arg.name), args.span()).into()
                        )
                    }
                },
            };
            self.scope.insert_var(mem::take(&mut arg.name), val)?;
        }
        Ok(self)
    }

    pub fn call(
        mut self,
        super_selector: &Selector,
        content: Option<&[Spanned<Stmt>]>,
    ) -> SassResult<Vec<Spanned<Stmt>>> {
        self.eval(super_selector, content)
    }

    fn eval(
        &mut self,
        super_selector: &Selector,
        content: Option<&[Spanned<Stmt>]>,
    ) -> SassResult<Vec<Spanned<Stmt>>> {
        let mut stmts = Vec::new();
        while let Some(expr) = eat_expr(&mut self.body, &mut self.scope, super_selector, content)? {
            let span = expr.span;
            match expr.node {
                Expr::AtRule(a) => match a {
                    AtRule::For(f) => {
                        stmts.extend(f.ruleset_eval(&mut self.scope, super_selector, content)?)
                    }
                    AtRule::Each(e) => {
                        stmts.extend(e.ruleset_eval(&mut self.scope, super_selector, content)?)
                    }
                    AtRule::While(w) => stmts.extend(w.ruleset_eval(
                        &mut self.scope,
                        super_selector,
                        false,
                        content,
                    )?),
                    AtRule::Include(s) => stmts.extend(s),
                    AtRule::If(i) => {
                        stmts.extend(i.eval(&mut self.scope.clone(), super_selector, content)?)
                    }
                    AtRule::Content => {
                        stmts.extend(content.unwrap_or_default().iter().cloned());
                    }
                    AtRule::Return(..) => {
                        return Err(("This at-rule is not allowed here.", span).into())
                    }
                    AtRule::Debug(..) | AtRule::Warn(..) => todo!(),
                    r => stmts.push(Spanned {
                        node: Stmt::AtRule(r),
                        span,
                    }),
                },
                Expr::Style(s) => stmts.push(Spanned {
                    node: Stmt::Style(s),
                    span,
                }),
                Expr::Styles(s) => stmts.extend(
                    s.into_iter()
                        .map(Box::new)
                        .map(Stmt::Style)
                        .map(|style| Spanned { node: style, span }),
                ),
                Expr::FunctionDecl(..) => {
                    return Err(("Mixins may not contain function declarations.", span).into())
                }
                Expr::MixinDecl(..) => {
                    return Err(("Mixins may not contain mixin declarations.", span).into())
                }
                Expr::Selector(selector) => {
                    let rules = self.eval(&super_selector.zip(&selector), content)?;
                    stmts.push(Spanned {
                        node: Stmt::RuleSet(RuleSet {
                            super_selector: super_selector.clone(),
                            selector,
                            rules,
                        }),
                        span,
                    });
                }
                Expr::VariableDecl(name, val) => {
                    self.scope.insert_var(&name, *val)?;
                }
                Expr::MultilineComment(s) => stmts.push(Spanned {
                    node: Stmt::MultilineComment(s),
                    span,
                }),
            }
        }
        Ok(stmts)
    }
}

pub(crate) fn eat_include<I: Iterator<Item = Token>>(
    toks: &mut PeekMoreIterator<I>,
    scope: &Scope,
    super_selector: &Selector,
    content: Option<&[Spanned<Stmt>]>,
    span_before: Span,
) -> SassResult<Vec<Spanned<Stmt>>> {
    devour_whitespace_or_comment(toks)?;
    let name = eat_ident(toks, scope, super_selector, span_before)?;

    devour_whitespace_or_comment(toks)?;

    let mut has_content = false;

    let args = if let Some(tok) = toks.next() {
        match tok.kind {
            ';' => CallArgs::new(name.span),
            '(' => {
                let tmp = eat_call_args(toks, tok.pos)?;
                devour_whitespace_or_comment(toks)?;
                if let Some(tok) = toks.peek() {
                    match tok.kind {
                        ';' => {
                            toks.next();
                        }
                        '{' => {
                            toks.next();
                            has_content = true
                        }
                        _ => {}
                    }
                }
                tmp
            }
            '{' => {
                has_content = true;
                CallArgs::new(name.span)
            }
            _ => return Err(("expected \"{\".", tok.pos()).into()),
        }
    } else {
        return Err(("unexpected EOF", name.span).into());
    };

    devour_whitespace(toks);

    let mut this_content = Vec::new();

    if let Some(tok) = toks.peek() {
        if tok.kind == '{' {
            toks.next();
            ruleset_eval(
                toks,
                &mut scope.clone(),
                super_selector,
                false,
                content,
                &mut this_content,
            )?;
        } else if has_content {
            ruleset_eval(
                toks,
                &mut scope.clone(),
                super_selector,
                false,
                content,
                &mut this_content,
            )?;
        }
    }

    let mixin = scope.get_mixin(name)?;

    let rules = mixin
        .args(args, scope, super_selector)?
        .call(super_selector, Some(&this_content))?;
    Ok(rules)
}
