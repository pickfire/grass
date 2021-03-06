use peekmore::PeekMoreIterator;

use codemap::{Span, Spanned};

use crate::error::SassResult;
use crate::scope::Scope;
use crate::selector::Selector;
use crate::utils::{devour_whitespace, devour_whitespace_or_comment, eat_ident};
use crate::value::Value;
use crate::{Expr, Token};

/// A style: `color: red`
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Style {
    pub property: String,
    pub value: Spanned<Value>,
}

impl Style {
    pub fn parse_property<I: Iterator<Item = Token>>(
        toks: &mut PeekMoreIterator<I>,
        scope: &Scope,
        super_selector: &Selector,
        super_property: String,
        span_before: Span,
    ) -> SassResult<String> {
        StyleParser::new(scope, super_selector).parse_property(toks, super_property, span_before)
    }

    pub fn to_string(&self) -> SassResult<String> {
        Ok(format!(
            "{}: {};",
            self.property,
            self.value.node.to_css_string(self.value.span)?
        ))
    }

    pub(crate) fn eval(self) -> SassResult<Self> {
        Ok(Style {
            property: self.property,
            value: Spanned {
                span: self.value.span,
                node: self.value.node.eval(self.value.span)?.node,
            },
        })
    }

    pub fn parse_value<I: Iterator<Item = Token>>(
        toks: &mut PeekMoreIterator<I>,
        scope: &Scope,
        super_selector: &Selector,
        span_before: Span,
    ) -> SassResult<Spanned<Value>> {
        StyleParser::new(scope, super_selector).parse_style_value(toks, scope, span_before)
    }

    pub fn from_tokens<I: Iterator<Item = Token>>(
        toks: &mut PeekMoreIterator<I>,
        scope: &Scope,
        super_selector: &Selector,
        super_property: String,
    ) -> SassResult<Expr> {
        StyleParser::new(scope, super_selector).eat_style_group(toks, super_property, scope)
    }
}

struct StyleParser<'a> {
    scope: &'a Scope,
    super_selector: &'a Selector,
}

impl<'a> StyleParser<'a> {
    const fn new(scope: &'a Scope, super_selector: &'a Selector) -> Self {
        StyleParser {
            scope,
            super_selector,
        }
    }

    pub(crate) fn parse_style_value<I: Iterator<Item = Token>>(
        &self,
        toks: &mut PeekMoreIterator<I>,
        scope: &Scope,
        span_before: Span,
    ) -> SassResult<Spanned<Value>> {
        devour_whitespace(toks);
        Value::from_tokens(toks, scope, self.super_selector, span_before)
    }

    pub(crate) fn eat_style_group<I: Iterator<Item = Token>>(
        &self,
        toks: &mut PeekMoreIterator<I>,
        super_property: String,
        scope: &Scope,
    ) -> SassResult<Expr> {
        let mut styles = Vec::new();
        devour_whitespace(toks);
        while let Some(tok) = toks.peek().cloned() {
            match tok.kind {
                '{' => {
                    toks.next();
                    devour_whitespace(toks);
                    loop {
                        let property =
                            self.parse_property(toks, super_property.clone(), tok.pos)?;
                        if let Some(tok) = toks.peek() {
                            if tok.kind == '{' {
                                match self.eat_style_group(toks, property, scope)? {
                                    Expr::Styles(s) => styles.extend(s),
                                    Expr::Style(s) => styles.push(*s),
                                    _ => unreachable!(),
                                }
                                devour_whitespace(toks);
                                if let Some(tok) = toks.peek() {
                                    if tok.kind == '}' {
                                        toks.next();
                                        devour_whitespace(toks);
                                        return Ok(Expr::Styles(styles));
                                    } else {
                                        continue;
                                    }
                                }
                                continue;
                            }
                        }
                        let value = self.parse_style_value(toks, scope, tok.pos)?;
                        match toks.peek() {
                            Some(Token { kind: '}', .. }) => {
                                styles.push(Style { property, value });
                            }
                            Some(Token { kind: ';', .. }) => {
                                toks.next();
                                devour_whitespace(toks);
                                styles.push(Style { property, value });
                            }
                            Some(Token { kind: '{', .. }) => {
                                styles.push(Style {
                                    property: property.clone(),
                                    value,
                                });
                                match self.eat_style_group(toks, property, scope)? {
                                    Expr::Style(s) => styles.push(*s),
                                    Expr::Styles(s) => styles.extend(s),
                                    _ => unreachable!(),
                                }
                            }
                            Some(..) | None => {
                                devour_whitespace(toks);
                                styles.push(Style { property, value });
                            }
                        }
                        if let Some(tok) = toks.peek() {
                            match tok.kind {
                                '}' => {
                                    toks.next();
                                    devour_whitespace(toks);
                                    return Ok(Expr::Styles(styles));
                                }
                                _ => continue,
                            }
                        }
                    }
                }
                _ => {
                    let value = self.parse_style_value(toks, scope, tok.pos)?;
                    let t = toks.peek().ok_or(("expected more input.", value.span))?;
                    match t.kind {
                        ';' => {
                            toks.next();
                            devour_whitespace(toks);
                        }
                        '{' => {
                            let mut v = vec![Style {
                                property: super_property.clone(),
                                value,
                            }];
                            match self.eat_style_group(toks, super_property, scope)? {
                                Expr::Style(s) => v.push(*s),
                                Expr::Styles(s) => v.extend(s),
                                _ => unreachable!(),
                            }
                            return Ok(Expr::Styles(v));
                        }
                        _ => {}
                    }
                    return Ok(Expr::Style(Box::new(Style {
                        property: super_property,
                        value,
                    })));
                }
            }
        }
        Ok(Expr::Styles(styles))
    }

    pub(crate) fn parse_property<I: Iterator<Item = Token>>(
        &self,
        toks: &mut PeekMoreIterator<I>,
        mut super_property: String,
        span_before: Span,
    ) -> SassResult<String> {
        devour_whitespace(toks);
        let property = eat_ident(toks, self.scope, self.super_selector, span_before)?;
        devour_whitespace_or_comment(toks)?;
        if let Some(Token { kind: ':', .. }) = toks.peek() {
            toks.next();
            devour_whitespace_or_comment(toks)?;
        } else {
            return Err(("Expected \":\".", property.span).into());
        }

        if super_property.is_empty() {
            Ok(property.node)
        } else {
            super_property.reserve(1 + property.node.len());
            super_property.push('-');
            super_property.push_str(&property.node);
            Ok(super_property)
        }
    }
}
