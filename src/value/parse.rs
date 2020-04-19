use std::convert::TryFrom;
use std::iter::{Iterator, Peekable};
use std::mem;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::pow;

use codemap::{Span, Spanned};

use super::css_function::{eat_calc_args, eat_progid};

use crate::args::eat_call_args;
use crate::builtin::GLOBAL_FUNCTIONS;
use crate::color::Color;
use crate::common::{Brackets, ListSeparator, Op, QuoteKind};
use crate::error::SassResult;
use crate::scope::Scope;
use crate::selector::Selector;
use crate::unit::Unit;
use crate::utils::{
    devour_whitespace, eat_comment, eat_ident, eat_ident_no_interpolation, eat_number,
    parse_quoted_string, read_until_char, read_until_closing_paren,
    read_until_closing_square_brace, read_until_newline, IsWhitespace,
};
use crate::value::Value;
use crate::Token;

use super::map::SassMap;
use super::number::Number;

fn parse_hex<I: Iterator<Item = Token>>(
    toks: &mut Peekable<I>,
    scope: &Scope,
    super_selector: &Selector,
    mut span: Span,
) -> SassResult<Spanned<Value>> {
    let mut s = String::with_capacity(8);
    if toks
        .peek()
        .ok_or(("Expected identifier.", span))?
        .kind
        .is_ascii_digit()
    {
        while let Some(c) = toks.peek() {
            if !c.kind.is_ascii_hexdigit() || s.len() == 8 {
                break;
            }
            let tok = toks.next().unwrap();
            span = span.merge(tok.pos());
            s.push(tok.kind);
        }
    } else {
        let i = eat_ident(toks, scope, super_selector)?;
        if i.node.chars().all(|c| c.is_ascii_hexdigit()) {
            s = i.node;
            span = span.merge(i.span);
        } else {
            return Ok(Spanned {
                node: Value::Ident(format!("#{}", i.node), QuoteKind::None),
                span: i.span,
            });
        }
    }
    match s.len() {
        3 => {
            let v = match u16::from_str_radix(&s, 16) {
                Ok(a) => a,
                Err(_) => return Ok(Value::Ident(format!("#{}", s), QuoteKind::None).span(span)),
            };
            let red = (((v & 0xf00) >> 8) * 0x11) as u8;
            let green = (((v & 0x0f0) >> 4) * 0x11) as u8;
            let blue = ((v & 0x00f) * 0x11) as u8;
            Ok(Value::Color(Color::new(red, green, blue, 1, format!("#{}", s))).span(span))
        }
        4 => {
            let v = match u16::from_str_radix(&s, 16) {
                Ok(a) => a,
                Err(_) => return Ok(Value::Ident(format!("#{}", s), QuoteKind::None).span(span)),
            };
            let red = (((v & 0xf000) >> 12) * 0x11) as u8;
            let green = (((v & 0x0f00) >> 8) * 0x11) as u8;
            let blue = (((v & 0x00f0) >> 4) * 0x11) as u8;
            let alpha = ((v & 0x000f) * 0x11) as u8;
            Ok(Value::Color(Color::new(red, green, blue, alpha, format!("#{}", s))).span(span))
        }
        6 => {
            let v = match u32::from_str_radix(&s, 16) {
                Ok(a) => a,
                Err(_) => return Ok(Value::Ident(format!("#{}", s), QuoteKind::None).span(span)),
            };
            let red = ((v & 0x00ff_0000) >> 16) as u8;
            let green = ((v & 0x0000_ff00) >> 8) as u8;
            let blue = (v & 0x0000_00ff) as u8;
            Ok(Value::Color(Color::new(red, green, blue, 1, format!("#{}", s))).span(span))
        }
        8 => {
            let v = match u32::from_str_radix(&s, 16) {
                Ok(a) => a,
                Err(_) => return Ok(Value::Ident(format!("#{}", s), QuoteKind::None).span(span)),
            };
            let red = ((v & 0xff00_0000) >> 24) as u8;
            let green = ((v & 0x00ff_0000) >> 16) as u8;
            let blue = ((v & 0x0000_ff00) >> 8) as u8;
            let alpha = (v & 0x0000_00ff) as u8;
            Ok(Value::Color(Color::new(red, green, blue, alpha, format!("#{}", s))).span(span))
        }
        _ => Err(("Expected hex digit.", span).into()),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum IntermediateValue {
    Value(Spanned<Value>),
    Op(Spanned<Op>),
    Bracketed(Spanned<Vec<Token>>),
    Paren(Spanned<Vec<Token>>),
    Comma,
    Whitespace,
}

impl IsWhitespace for IntermediateValue {
    fn is_whitespace(&self) -> bool {
        if self == &IntermediateValue::Whitespace {
            return true;
        }
        false
    }
}

fn parse_paren(
    t: Spanned<Vec<Token>>,
    scope: &Scope,
    super_selector: &Selector,
    space_separated: &mut Vec<Spanned<Value>>,
) -> SassResult<()> {
    if t.is_empty() {
        space_separated
            .push(Value::List(Vec::new(), ListSeparator::Space, Brackets::None).span(t.span));
        return Ok(());
    }

    let paren_toks = &mut t.node.into_iter().peekable();

    let mut map = SassMap::new();
    let key = Value::from_vec(read_until_char(paren_toks, ':'), scope, super_selector)?;

    if paren_toks.peek().is_none() {
        space_separated.push(Spanned {
            node: Value::Paren(Box::new(key.node)),
            span: key.span,
        });
        return Ok(());
    }

    let val = Value::from_vec(read_until_char(paren_toks, ','), scope, super_selector)?;

    map.insert(key.node, val.node);

    if paren_toks.peek().is_none() {
        space_separated.push(Spanned {
            node: Value::Map(map),
            span: key.span.merge(val.span),
        });
        return Ok(());
    }

    let mut span = key.span;

    loop {
        let key = Value::from_vec(read_until_char(paren_toks, ':'), scope, super_selector)?;
        devour_whitespace(paren_toks);
        let val = Value::from_vec(read_until_char(paren_toks, ','), scope, super_selector)?;
        span = span.merge(val.span);
        devour_whitespace(paren_toks);
        if map.insert(key.node, val.node) {
            return Err(("Duplicate key.", key.span).into());
        }
        if paren_toks.peek().is_none() {
            break;
        }
    }
    space_separated.push(Spanned {
        node: Value::Map(map),
        span,
    });
    Ok(())
}

fn eat_op<I: Iterator<Item = IntermediateValue>>(
    iter: &mut Peekable<I>,
    scope: &Scope,
    super_selector: &Selector,
    op: Spanned<Op>,
    space_separated: &mut Vec<Spanned<Value>>,
) -> SassResult<()> {
    match op.node {
        Op::Not => {
            devour_whitespace(iter);
            let right = single_value(iter, scope, super_selector, op.span)?;
            space_separated.push(Spanned {
                node: Value::UnaryOp(op.node, Box::new(right.node)),
                span: right.span,
            });
        }
        Op::Plus => {
            if let Some(left) = space_separated.pop() {
                devour_whitespace(iter);
                let right = single_value(iter, scope, super_selector, op.span)?;
                space_separated.push(Spanned {
                    node: Value::BinaryOp(Box::new(left.node), op.node, Box::new(right.node)),
                    span: left.span.merge(right.span),
                });
            } else {
                devour_whitespace(iter);
                let right = single_value(iter, scope, super_selector, op.span)?;
                space_separated.push(Spanned {
                    node: Value::UnaryOp(op.node, Box::new(right.node)),
                    span: right.span,
                });
            }
        }
        Op::Minus => {
            if devour_whitespace(iter) {
                let right = single_value(iter, scope, super_selector, op.span)?;
                if let Some(left) = space_separated.pop() {
                    space_separated.push(Spanned {
                        node: Value::BinaryOp(Box::new(left.node), op.node, Box::new(right.node)),
                        span: left.span.merge(right.span),
                    });
                } else {
                    space_separated.push(right.map_node(|n| Value::UnaryOp(op.node, Box::new(n))));
                }
            } else {
                let right = single_value(iter, scope, super_selector, op.span)?;
                if right.node == Value::Null {
                    space_separated.push(
                        right.map_node(|_| Value::Ident("-null".to_string(), QuoteKind::None)),
                    );
                    return Ok(());
                }
                space_separated.push(right.map_node(|n| Value::UnaryOp(op.node, Box::new(n))));
            }
        }
        Op::And | Op::Or => {
            devour_whitespace(iter);
            if iter.peek().is_none() {
                space_separated.push(Value::Ident(op.to_string(), QuoteKind::None).span(op.span));
            } else if let Some(left) = space_separated.pop() {
                devour_whitespace(iter);
                let right = single_value(iter, scope, super_selector, left.span)?;
                space_separated.push(
                    Value::BinaryOp(Box::new(left.node), op.node, Box::new(right.node))
                        .span(left.span.merge(right.span)),
                );
            } else {
                return Err(("Expected expression.", op.span).into());
            }
        }
        _ => {
            if let Some(left) = space_separated.pop() {
                devour_whitespace(iter);
                let right = single_value(iter, scope, super_selector, left.span)?;
                space_separated.push(
                    Value::BinaryOp(Box::new(left.node), op.node, Box::new(right.node))
                        .span(left.span.merge(right.span)),
                );
            } else {
                return Err(("Expected expression.", op.span).into());
            }
        }
    }
    Ok(())
}

fn single_value<I: Iterator<Item = IntermediateValue>>(
    iter: &mut Peekable<I>,
    scope: &Scope,
    super_selector: &Selector,
    span: Span,
) -> SassResult<Spanned<Value>> {
    Ok(match iter.next().ok_or(("Expected expression.", span))? {
        IntermediateValue::Value(v) => v,
        IntermediateValue::Op(op) => match op.node {
            Op::Minus => {
                devour_whitespace(iter);
                let val = single_value(iter, scope, super_selector, span)?;
                Spanned {
                    node: val.node.neg(val.span)?,
                    span: op.span.merge(val.span),
                }
            }
            Op::Not => {
                devour_whitespace(iter);
                let val = single_value(iter, scope, super_selector, span)?;
                Spanned {
                    node: Value::UnaryOp(op.node, Box::new(val.node)),
                    span: op.span.merge(val.span),
                }
            }
            _ => todo!(),
        },
        IntermediateValue::Whitespace => unreachable!(),
        IntermediateValue::Comma => return Err(("Expected expression.", span).into()),
        IntermediateValue::Bracketed(t) => {
            let v = Value::from_vec(t.node, scope, super_selector)?;
            match v.node {
                Value::List(v, sep, Brackets::None) => Value::List(v, sep, Brackets::Bracketed),
                v => Value::List(vec![v], ListSeparator::Space, Brackets::Bracketed),
            }
            .span(v.span)
        }
        IntermediateValue::Paren(t) => {
            let val = Value::from_vec(t.node, scope, super_selector)?;
            Spanned {
                node: Value::Paren(Box::new(val.node)),
                span: val.span,
            }
        }
    })
}

impl Value {
    pub fn from_tokens<I: Iterator<Item = Token>>(
        toks: &mut Peekable<I>,
        scope: &Scope,
        super_selector: &Selector,
    ) -> SassResult<Spanned<Self>> {
        let mut intermediate_values = Vec::new();
        let span = match toks.peek() {
            Some(Token { pos, .. }) => *pos,
            None => todo!("Expected expression."),
        };
        while toks.peek().is_some() {
            intermediate_values.push(Self::parse_intermediate_value(toks, scope, super_selector)?);
        }
        let mut space_separated = Vec::new();
        let mut comma_separated = Vec::new();
        let mut iter = intermediate_values.into_iter().peekable();
        while let Some(val) = iter.next() {
            match val {
                IntermediateValue::Value(v) => space_separated.push(v),
                IntermediateValue::Op(op) => {
                    eat_op(&mut iter, scope, super_selector, op, &mut space_separated)?;
                }
                IntermediateValue::Whitespace => continue,
                IntermediateValue::Comma => {
                    if space_separated.len() == 1 {
                        comma_separated.push(space_separated.pop().unwrap());
                    } else {
                        let mut span = space_separated[0].span;
                        comma_separated.push(
                            Value::List(
                                mem::take(&mut space_separated)
                                    .into_iter()
                                    .map(|a| {
                                        span = span.merge(a.span);
                                        a.node
                                    })
                                    .collect(),
                                ListSeparator::Space,
                                Brackets::None,
                            )
                            .span(span),
                        );
                    }
                }
                IntermediateValue::Bracketed(t) => {
                    if t.node.is_empty() {
                        space_separated.push(
                            Value::List(Vec::new(), ListSeparator::Space, Brackets::Bracketed)
                                .span(t.span),
                        );
                        continue;
                    }
                    space_separated.push(
                        match Value::from_vec(t.node, scope, super_selector)?.node {
                            Value::List(v, sep, Brackets::None) => {
                                Value::List(v, sep, Brackets::Bracketed).span(t.span)
                            }
                            v => Value::List(vec![v], ListSeparator::Space, Brackets::Bracketed)
                                .span(t.span),
                        },
                    )
                }
                IntermediateValue::Paren(t) => {
                    parse_paren(t, scope, super_selector, &mut space_separated)?;
                }
            }
        }

        Ok(if !comma_separated.is_empty() {
            if space_separated.len() == 1 {
                comma_separated.push(space_separated.pop().unwrap());
            } else if !space_separated.is_empty() {
                comma_separated.push(
                    Value::List(
                        space_separated.into_iter().map(|a| a.node).collect(),
                        ListSeparator::Space,
                        Brackets::None,
                    )
                    .span(span),
                );
            }
            Value::List(
                comma_separated.into_iter().map(|a| a.node).collect(),
                ListSeparator::Comma,
                Brackets::None,
            )
            .span(span)
        } else if space_separated.len() == 1 {
            space_separated.pop().unwrap()
        } else {
            Value::List(
                space_separated.into_iter().map(|a| a.node).collect(),
                ListSeparator::Space,
                Brackets::None,
            )
            .span(span)
        })
    }

    pub fn from_vec(
        toks: Vec<Token>,
        scope: &Scope,
        super_selector: &Selector,
    ) -> SassResult<Spanned<Value>> {
        Self::from_tokens(&mut toks.into_iter().peekable(), scope, super_selector)
    }

    fn ident<I: Iterator<Item = Token>>(
        toks: &mut Peekable<I>,
        scope: &Scope,
        super_selector: &Selector,
    ) -> SassResult<IntermediateValue> {
        let Spanned { node: mut s, span } = eat_ident(toks, scope, super_selector)?;
        if s == "progid" && toks.peek().is_some() && toks.peek().unwrap().kind == ':' {
            toks.next();
            s.push(':');
            s.push_str(&eat_progid(toks, scope, super_selector)?);
            return Ok(IntermediateValue::Value(Spanned {
                node: Value::Ident(s, QuoteKind::None),
                span,
            }));
        }
        match toks.peek() {
            Some(Token { kind: '(', .. }) => {
                toks.next();
                let func = match scope.get_fn(Spanned {
                    node: s.clone(),
                    span,
                }) {
                    Ok(f) => f,
                    Err(_) => match GLOBAL_FUNCTIONS.get(&s.replace('_', "-")) {
                        Some(f) => {
                            return Ok(IntermediateValue::Value(Spanned {
                                node: f.0(
                                    eat_call_args(toks, scope, super_selector)?,
                                    scope,
                                    super_selector,
                                )?,
                                span,
                            }))
                        }
                        None => {
                            match s.as_str() {
                                "calc" | "element" | "expression" => {
                                    s.push_str(&eat_calc_args(toks, scope, super_selector)?)
                                }
                                // "min" => {}
                                // "max" => {}
                                // "url" => {}
                                _ => s.push_str(
                                    &eat_call_args(toks, scope, super_selector)?
                                        .to_css_string(scope, super_selector)?,
                                ),
                            }
                            return Ok(IntermediateValue::Value(Spanned {
                                node: Value::Ident(s, QuoteKind::None),
                                span,
                            }));
                        }
                    },
                };
                Ok(IntermediateValue::Value(
                    func.clone()
                        .args(
                            eat_call_args(toks, scope, super_selector)?,
                            scope,
                            super_selector,
                        )?
                        .call(super_selector, func.body())?
                        .span(span),
                ))
            }
            _ => {
                if let Ok(c) = crate::color::ColorName::try_from(s.as_ref()) {
                    Ok(IntermediateValue::Value(Spanned {
                        node: Value::Color(c.into_color(s)),
                        span,
                    }))
                } else {
                    Ok(match s.to_ascii_lowercase().as_str() {
                        "true" => IntermediateValue::Value(Value::True.span(span)),
                        "false" => IntermediateValue::Value(Value::False.span(span)),
                        "null" => IntermediateValue::Value(Value::Null.span(span)),
                        "not" => IntermediateValue::Op(Spanned {
                            node: Op::Not,
                            span,
                        }),
                        "and" => IntermediateValue::Op(Spanned {
                            node: Op::And,
                            span,
                        }),
                        "or" => IntermediateValue::Op(Spanned { node: Op::Or, span }),
                        _ => IntermediateValue::Value(Spanned {
                            node: Value::Ident(s, QuoteKind::None),
                            span,
                        }),
                    })
                }
            }
        }
    }

    fn parse_intermediate_value<I: Iterator<Item = Token>>(
        toks: &mut Peekable<I>,
        scope: &Scope,
        super_selector: &Selector,
    ) -> SassResult<IntermediateValue> {
        if devour_whitespace(toks) {
            return Ok(IntermediateValue::Whitespace);
        }
        let (kind, span) = match toks.peek() {
            Some(v) => (v.kind, v.pos()),
            None => panic!("unexpected eof"),
        };
        match kind {
            ',' => {
                toks.next();
                Ok(IntermediateValue::Comma)
            }
            '0'..='9' | '.' => {
                let Spanned {
                    node: val,
                    mut span,
                } = eat_number(toks)?;
                let unit = if let Some(tok) = toks.peek() {
                    match tok.kind {
                        'a'..='z' | 'A'..='Z' | '_' => {
                            let u = eat_ident(toks, scope, super_selector)?;
                            span = span.merge(u.span);
                            Unit::from(&u.node)
                        }
                        '%' => {
                            span = span.merge(toks.next().unwrap().pos());
                            Unit::Percent
                        }
                        _ => Unit::None,
                    }
                } else {
                    Unit::None
                };
                let n = if let Ok(v) = val.parse::<BigRational>() {
                    // the number is an integer!
                    v
                // the number is floating point
                } else {
                    let mut num = String::new();
                    let mut chars = val.chars();
                    let mut num_dec = 0;
                    while let Some(c) = chars.next() {
                        if c == '.' {
                            break;
                        }
                        num.push(c);
                    }
                    for c in chars {
                        num_dec += 1;
                        num.push(c);
                    }
                    BigRational::new(num.parse().unwrap(), pow(BigInt::from(10), num_dec))
                };
                Ok(IntermediateValue::Value(
                    Value::Dimension(Number::new(n), unit).span(span),
                ))
            }
            '(' => {
                let mut span = toks.next().unwrap().pos();
                let mut inner = read_until_closing_paren(toks);
                // todo: the above shouldn't eat the closing paren
                if !inner.is_empty() {
                    let last_tok = inner.pop().unwrap();
                    if last_tok.kind != ')' {
                        return Err(("expected \")\".", span).into());
                    }
                    span = span.merge(last_tok.pos());
                }
                Ok(IntermediateValue::Paren(Spanned { node: inner, span }))
            }
            '&' => {
                let span = toks.next().unwrap().pos();
                Ok(IntermediateValue::Value(Spanned {
                    node: Value::Ident(super_selector.to_string(), QuoteKind::None),
                    span,
                }))
            }
            '#' => {
                if let Ok(s) = eat_ident(toks, scope, super_selector) {
                    Ok(IntermediateValue::Value(Spanned {
                        node: Value::Ident(s.node, QuoteKind::None),
                        span: s.span,
                    }))
                } else {
                    Ok(IntermediateValue::Value(parse_hex(
                        toks,
                        scope,
                        super_selector,
                        span,
                    )?))
                }
            }
            _ if kind.is_ascii_alphabetic()
                || kind == '_'
                || kind == '\\'
                || (!kind.is_ascii() && !kind.is_control()) =>
            {
                Self::ident(toks, scope, super_selector)
            }
            q @ '"' | q @ '\'' => {
                let span_start = toks.next().unwrap().pos();
                let Spanned { node, span } = parse_quoted_string(toks, scope, q, super_selector)?;
                Ok(IntermediateValue::Value(Spanned {
                    node,
                    span: span_start.merge(span),
                }))
            }
            '[' => {
                let mut span = toks.next().unwrap().pos();
                let mut inner = read_until_closing_square_brace(toks);
                if !inner.is_empty() {
                    let last_tok = inner.pop().unwrap();
                    if last_tok.kind != ']' {
                        return Err(("expected \"]\".", span).into());
                    }
                    span = span.merge(last_tok.pos());
                }
                Ok(IntermediateValue::Bracketed(Spanned { node: inner, span }))
            }
            '$' => {
                toks.next();
                let val = eat_ident_no_interpolation(toks)?;
                Ok(IntermediateValue::Value(Spanned {
                    node: scope.get_var(val.clone())?.node,
                    span: val.span,
                }))
            }
            '@' => Err(("expected \";\".", span).into()),
            '+' => {
                let span = toks.next().unwrap().pos();
                Ok(IntermediateValue::Op(Spanned {
                    node: Op::Plus,
                    span,
                }))
            }
            '-' => {
                let span = toks.next().unwrap().pos();
                Ok(IntermediateValue::Op(Spanned {
                    node: Op::Minus,
                    span,
                }))
            }
            '*' => {
                let span = toks.next().unwrap().pos();
                Ok(IntermediateValue::Op(Spanned {
                    node: Op::Mul,
                    span,
                }))
            }
            '%' => {
                let span = toks.next().unwrap().pos();
                Ok(IntermediateValue::Op(Spanned {
                    node: Op::Rem,
                    span,
                }))
            }
            q @ '>' | q @ '<' => {
                let mut span = toks.next().unwrap().pos();
                Ok(IntermediateValue::Op(Spanned {
                    node: if toks.peek().unwrap().kind == '=' {
                        span = span.merge(toks.next().unwrap().pos());
                        match q {
                            '>' => Op::GreaterThanEqual,
                            '<' => Op::LessThanEqual,
                            _ => unreachable!(),
                        }
                    } else {
                        match q {
                            '>' => Op::GreaterThan,
                            '<' => Op::LessThan,
                            _ => unreachable!(),
                        }
                    },
                    span,
                }))
            }
            '=' => {
                let mut span = toks.next().unwrap().pos();
                if let Token { kind: '=', pos } = toks.next().unwrap() {
                    span = span.merge(pos);
                    Ok(IntermediateValue::Op(Spanned {
                        node: Op::Equal,
                        span,
                    }))
                } else {
                    Err(("expected \"=\".", span).into())
                }
            }
            '!' => {
                let mut span = toks.next().unwrap().pos();
                if toks.peek().is_some() && toks.peek().unwrap().kind == '=' {
                    span = span.merge(toks.next().unwrap().pos());
                    return Ok(IntermediateValue::Op(Spanned {
                        node: Op::NotEqual,
                        span,
                    }));
                }
                devour_whitespace(toks);
                let v = eat_ident(toks, scope, super_selector)?;
                span = span.merge(v.span);
                if v.node.to_ascii_lowercase().as_str() == "important" {
                    Ok(IntermediateValue::Value(Spanned {
                        node: Value::Important,
                        span,
                    }))
                } else {
                    Err(("Expected \"important\".", span).into())
                }
            }
            '/' => {
                let span = toks.next().unwrap().pos();
                if toks.peek().is_none() {
                    return Err(("Expected expression.", span).into());
                }
                if '*' == toks.peek().unwrap().kind {
                    toks.next();
                    eat_comment(toks, &Scope::new(), &Selector::new())?;
                    Ok(IntermediateValue::Whitespace)
                } else if '/' == toks.peek().unwrap().kind {
                    read_until_newline(toks);
                    devour_whitespace(toks);
                    Ok(IntermediateValue::Whitespace)
                } else {
                    Ok(IntermediateValue::Op(Spanned {
                        node: Op::Div,
                        span,
                    }))
                }
            }
            ':' | '?' | ')' => Err(("expected \";\".", span).into()),
            v if v.is_control() => Err(("Expected expression.", span).into()),
            v => {
                dbg!(v);
                panic!("Unexpected token in value parsing")
            }
        }
    }
}
