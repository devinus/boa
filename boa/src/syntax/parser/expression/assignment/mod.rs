//! Assignment operator parsing.
//!
//! More information:
//!  - [MDN documentation][mdn]
//!  - [ECMAScript specification][spec]
//!
//! [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/Assignment_Operators#Assignment
//! [spec]: https://tc39.es/ecma262/#sec-assignment-operators

mod arrow_function;
mod conditional;
mod exponentiation;

use self::{arrow_function::ArrowFunction, conditional::ConditionalExpression};
use crate::syntax::lexer::{Error as LexError, InputElement, Token, TokenKind};
use crate::{
    syntax::{
        ast::{
            node::{Assign, BinOp, Node},
            Keyword, Punctuator,
        },
        parser::{AllowAwait, AllowIn, AllowYield, Cursor, ParseError, ParseResult, TokenParser},
    },
    BoaProfiler,
};
pub(super) use exponentiation::ExponentiationExpression;

use std::io::Read;

/// Assignment expression parsing.
///
/// This can be one of the following:
///
///  - [`ConditionalExpression`](../conditional_operator/struct.ConditionalExpression.html)
///  - `YieldExpression`
///  - [`ArrowFunction`](../../function/arrow_function/struct.ArrowFunction.html)
///  - `AsyncArrowFunction`
///  - [`LeftHandSideExpression`][lhs] `=` `AssignmentExpression`
///  - [`LeftHandSideExpression`][lhs] `AssignmentOperator` `AssignmentExpression`
///
/// More information:
///  - [MDN documentation][mdn]
///  - [ECMAScript specification][spec]
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/Assignment_Operators#Assignment
/// [spec]: https://tc39.es/ecma262/#prod-AssignmentExpression
/// [lhs]: ../lhs_expression/struct.LeftHandSideExpression.html
#[derive(Debug, Clone, Copy)]
pub(in crate::syntax::parser) struct AssignmentExpression {
    allow_in: AllowIn,
    allow_yield: AllowYield,
    allow_await: AllowAwait,
}

impl AssignmentExpression {
    /// Creates a new `AssignmentExpression` parser.
    pub(in crate::syntax::parser) fn new<I, Y, A>(
        allow_in: I,
        allow_yield: Y,
        allow_await: A,
    ) -> Self
    where
        I: Into<AllowIn>,
        Y: Into<AllowYield>,
        A: Into<AllowAwait>,
    {
        Self {
            allow_in: allow_in.into(),
            allow_yield: allow_yield.into(),
            allow_await: allow_await.into(),
        }
    }
}

impl<R> TokenParser<R> for AssignmentExpression
where
    R: Read,
{
    type Output = Node;

    fn parse(self, cursor: &mut Cursor<R>) -> ParseResult {
        let _timer = BoaProfiler::global().start_event("AssignmentExpression", "Parsing");
        cursor.set_goal(InputElement::Div);

        // Arrow function
        match cursor.peek(true)?.ok_or(ParseError::AbruptEnd)?.kind() {
            // a=>{}
            TokenKind::Identifier(_)
            | TokenKind::Keyword(Keyword::Yield)
            | TokenKind::Keyword(Keyword::Await) => {
                if cursor.peek_expect_no_lineterminator(true).is_ok() {
                    if let Some(tok) = cursor.peek_skip(false)? {
                        if tok.kind() == &TokenKind::Punctuator(Punctuator::Arrow) {
                            return ArrowFunction::new(
                                self.allow_in,
                                self.allow_yield,
                                self.allow_await,
                            )
                            .parse(cursor)
                            .map(Node::ArrowFunctionDecl);
                        }
                    }
                }
            }

            // (a,b)=>{}
            TokenKind::Punctuator(Punctuator::OpenParen) => {
                if let Some(next_token) = cursor.peek_skip(false)? {
                    match *next_token.kind() {
                        TokenKind::Punctuator(Punctuator::CloseParen)
                        | TokenKind::Punctuator(Punctuator::Spread)
                        | TokenKind::Identifier(_) => {
                            return ArrowFunction::new(
                                self.allow_in,
                                self.allow_yield,
                                self.allow_await,
                            )
                            .parse(cursor)
                            .map(Node::ArrowFunctionDecl);
                        }
                        _ => {}
                    }
                }
            }

            _ => {}
        }

        cursor.set_goal(InputElement::Div);

        let mut lhs = ConditionalExpression::new(self.allow_in, self.allow_yield, self.allow_await)
            .parse(cursor)?;

        let mut line_terminator: Option<Token> = None;

        // Loop to skip line terminators, cannot skip using cursor.peek() as this might remove a line terminator needed by a subsequent parse.
        while let Some(tok) = cursor.peek(false)? {
            match tok.kind() {
                TokenKind::Punctuator(Punctuator::Assign) => {
                    cursor.next(false)?.expect("= token vanished"); // Consume the token.
                    if is_assignable(&lhs) {
                        lhs = Assign::new(lhs, self.parse(cursor)?).into();
                        break;
                    } else {
                        return Err(ParseError::lex(LexError::Syntax(
                            "Invalid left-hand side in assignment".into(),
                        )));
                    }
                }
                TokenKind::Punctuator(p) if p.as_binop().is_some() => {
                    cursor.next(false)?.expect("Token vanished"); // Consume the token.
                    if is_assignable(&lhs) {
                        let expr = self.parse(cursor)?;
                        let binop = p.as_binop().expect("binop disappeared");
                        lhs = BinOp::new(binop, lhs, expr).into();
                        break;
                    } else {
                        return Err(ParseError::lex(LexError::Syntax(
                            "Invalid left-hand side in assignment".into(),
                        )));
                    }
                }
                TokenKind::LineTerminator => {
                    line_terminator = Some(tok);
                    cursor.next(false)?.expect("Line terminator vanished");
                }
                _ => break,
            }
        }

        if let Some(lt) = line_terminator {
            cursor.push_back(lt);
        }

        Ok(lhs)
    }
}

/// Returns true if as per spec[spec] the node can be assigned a value.
///
/// [spec]: https://tc39.es/ecma262/#sec-assignment-operators-static-semantics-early-errors
#[inline]
pub(crate) fn is_assignable(node: &Node) -> bool {
    if let Node::Const(_) | Node::ArrayDecl(_) = node {
        false
    } else {
        true
    }
}
