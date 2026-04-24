use std::collections::{HashMap, HashSet};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    fn dummy() -> Self {
        Self {
            line: usize::MAX,
            column: usize::MAX,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grammar {
    pub pos: Position,
    pub rules: Vec<Rule>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub pos: Position,
    pub name: String,
    pub body: Expr,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Sequence(Position, Box<Expr>, Box<Expr>),
    Alternation(Position, Box<Expr>, Box<Expr>),
    Repeat0(Position, Box<Expr>),
    Repeat1(Position, Box<Expr>),
    Optional(Position, Box<Expr>),
    AndPredicate(Position, Box<Expr>),
    NotPredicate(Position, Box<Expr>),
    StringLiteral(Position, String),
    Wildcard(Position),
    CharClass(Position, bool, Vec<CharClassElement>),
    CharSet(Position, bool, HashSet<char>),
    Debug(Position, Box<Expr>),
    Call(Position, String, Vec<Expr>),
    Identifier(Position, String),
    Function(Position, Vec<String>, Box<Expr>),
}

impl Expr {
    fn pos(&self) -> Position {
        match self {
            Expr::Sequence(pos, ..)
            | Expr::Alternation(pos, ..)
            | Expr::Repeat0(pos, ..)
            | Expr::Repeat1(pos, ..)
            | Expr::Optional(pos, ..)
            | Expr::AndPredicate(pos, ..)
            | Expr::NotPredicate(pos, ..)
            | Expr::StringLiteral(pos, ..)
            | Expr::Wildcard(pos)
            | Expr::CharClass(pos, ..)
            | Expr::CharSet(pos, ..)
            | Expr::Debug(pos, ..)
            | Expr::Call(pos, ..)
            | Expr::Identifier(pos, ..)
            | Expr::Function(pos, ..) => *pos,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CharClassElement {
    OneChar(char),
    CharRange(char, char),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvaluationStrategy {
    CallByName,
    CallByValueSeq,
    CallByValuePar,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvaluationResult {
    Success(String),
    Failure,
}

impl EvaluationResult {
    fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub pos: Position,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}, {}: {}",
            self.pos.line, self.pos.column, self.message
        )
    }
}

impl std::error::Error for ParseError {}

struct Parser<'a> {
    src: &'a str,
    idx: usize,
    line: usize,
    column: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            idx: 0,
            line: 0,
            column: 0,
        }
    }

    fn parse_grammar(mut self) -> Result<Grammar, ParseError> {
        let pos = self.pos();
        self.skip_spacing()?;
        let mut rules = Vec::new();
        while !self.is_eof() {
            rules.push(self.parse_definition()?);
            self.skip_spacing()?;
        }
        Ok(Grammar { pos, rules })
    }

    fn parse_definition(&mut self) -> Result<Rule, ParseError> {
        let (pos, name) = self.parse_identifier_with_spacing()?;
        let args = if self.peek_char() == Some('(') {
            self.expect_char('(')?;
            let mut args = Vec::new();
            if self.peek_char() != Some(')') {
                loop {
                    let (_, arg_name) = self.parse_identifier_with_spacing()?;
                    if self.peek_char() == Some(':') {
                        self.next_char();
                        self.skip_spacing()?;
                        self.skip_type_tree()?;
                    }
                    args.push(arg_name);
                    if self.peek_char() == Some(',') {
                        self.next_char();
                        self.skip_spacing()?;
                    } else {
                        break;
                    }
                }
            }
            self.expect_char(')')?;
            args
        } else {
            Vec::new()
        };
        self.expect_char('=')?;
        let body = self.parse_expression()?;
        self.expect_char(';')?;
        Ok(Rule {
            pos,
            name,
            body,
            args,
        })
    }

    fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_sequence()?;
        loop {
            match self.peek_char() {
                Some('/') | Some('|') => {
                    self.next_char();
                    self.skip_spacing()?;
                    let rhs = self.parse_sequence()?;
                    expr = Expr::Alternation(rhs.pos(), Box::new(expr), Box::new(rhs));
                }
                _ => return Ok(expr),
            }
        }
    }

    fn parse_sequence(&mut self) -> Result<Expr, ParseError> {
        let mut exprs = vec![self.parse_prefix()?];
        while self.is_sequence_start() {
            exprs.push(self.parse_prefix()?);
        }
        let mut iter = exprs.into_iter();
        let mut acc = iter.next().expect("sequence has at least one expression");
        for expr in iter {
            let pos = expr.pos();
            acc = Expr::Sequence(pos, Box::new(acc), Box::new(expr));
        }
        Ok(acc)
    }

    fn parse_prefix(&mut self) -> Result<Expr, ParseError> {
        match self.peek_char() {
            Some('&') => {
                let pos = self.pos();
                self.next_char();
                self.skip_spacing()?;
                Ok(Expr::AndPredicate(pos, Box::new(self.parse_suffix()?)))
            }
            Some('!') => {
                let pos = self.pos();
                self.next_char();
                self.skip_spacing()?;
                Ok(Expr::NotPredicate(pos, Box::new(self.parse_suffix()?)))
            }
            _ => self.parse_suffix(),
        }
    }

    fn parse_suffix(&mut self) -> Result<Expr, ParseError> {
        let primary = self.parse_primary()?;
        match self.peek_char() {
            Some('?') => {
                let pos = self.pos();
                self.next_char();
                self.skip_spacing()?;
                Ok(Expr::Optional(pos, Box::new(primary)))
            }
            Some('*') => {
                let pos = self.pos();
                self.next_char();
                self.skip_spacing()?;
                Ok(Expr::Repeat0(pos, Box::new(primary)))
            }
            Some('+') => {
                let pos = self.pos();
                self.next_char();
                self.skip_spacing()?;
                Ok(Expr::Repeat1(pos, Box::new(primary)))
            }
            _ => Ok(primary),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        if self.try_keyword("Debug") {
            let pos = self.pos();
            self.skip_spacing()?;
            self.expect_char('(')?;
            let body = self.parse_expression()?;
            self.expect_char(')')?;
            return Ok(Expr::Debug(pos, Box::new(body)));
        }
        if let Some(expr) = self.try_parse_call()? {
            return Ok(expr);
        }
        if self.is_ident_start() {
            let (pos, name) = self.parse_identifier_with_spacing()?;
            return Ok(Expr::Identifier(pos, name));
        }
        match self.peek_char() {
            Some('[') => self.parse_char_class(),
            Some('(') => self.parse_group_or_function(),
            Some('.') => {
                let pos = self.pos();
                self.next_char();
                self.skip_spacing()?;
                Ok(Expr::Wildcard(pos))
            }
            Some('_') => {
                let pos = self.pos();
                self.next_char();
                self.skip_spacing()?;
                Ok(Expr::StringLiteral(pos, String::new()))
            }
            Some('"') => self.parse_literal(),
            _ => Err(self.error("expected primary expression")),
        }
    }

    fn parse_group_or_function(&mut self) -> Result<Expr, ParseError> {
        let checkpoint = self.snapshot();
        let open_pos = self.pos();
        self.expect_char('(')?;
        let params_checkpoint = self.snapshot();
        let mut params = Vec::new();
        let mut lambda = true;
        if self.peek_char() != Some(')') {
            if self.is_ident_start() {
                loop {
                    let (_, name) = self.parse_identifier_with_spacing()?;
                    params.push(name);
                    if self.peek_char() == Some(',') {
                        self.next_char();
                        self.skip_spacing()?;
                        if !self.is_ident_start() {
                            lambda = false;
                            break;
                        }
                    } else {
                        break;
                    }
                }
            } else {
                lambda = false;
            }
        }
        if lambda && self.peek_char() == Some('-') {
            self.next_char();
            if self.peek_char() != Some('>') {
                self.restore(checkpoint);
                return self.parse_group_expression();
            }
            self.next_char();
            self.skip_spacing()?;
            let body = self.parse_expression()?;
            self.expect_char(')')?;
            return Ok(Expr::Function(open_pos, params, Box::new(body)));
        }
        self.restore(params_checkpoint);
        self.restore(checkpoint);
        self.parse_group_expression()
    }

    fn parse_group_expression(&mut self) -> Result<Expr, ParseError> {
        self.expect_char('(')?;
        let expr = self.parse_expression()?;
        self.expect_char(')')?;
        Ok(expr)
    }

    fn parse_char_class(&mut self) -> Result<Expr, ParseError> {
        let pos = self.pos();
        self.expect_raw('[')?;
        let positive = if self.peek_char() == Some('^') {
            self.next_char();
            false
        } else {
            true
        };
        let mut elems = Vec::new();
        while self.peek_char() != Some(']') {
            if self.is_eof() {
                return Err(self.error("unterminated character class"));
            }
            let first = self.parse_char_in_class()?;
            if self.peek_char() == Some('-') {
                let range_checkpoint = self.snapshot();
                self.next_char();
                if self.peek_char() == Some(']') {
                    self.restore(range_checkpoint);
                    elems.push(CharClassElement::OneChar(first));
                    continue;
                }
                let second = self.parse_char_in_class()?;
                elems.push(CharClassElement::CharRange(first, second));
            } else {
                elems.push(CharClassElement::OneChar(first));
            }
        }
        self.expect_raw(']')?;
        self.skip_spacing()?;
        Ok(Expr::CharClass(pos, positive, elems))
    }

    fn parse_char_in_class(&mut self) -> Result<char, ParseError> {
        self.parse_escaped_char(Some(']'))
    }

    fn parse_literal(&mut self) -> Result<Expr, ParseError> {
        let pos = self.pos();
        self.expect_raw('"')?;
        let mut out = String::new();
        while self.peek_char() != Some('"') {
            if self.is_eof() {
                return Err(self.error("unterminated string literal"));
            }
            out.push(self.parse_escaped_char(Some('"'))?);
        }
        self.expect_raw('"')?;
        self.skip_spacing()?;
        Ok(Expr::StringLiteral(pos, out))
    }

    fn try_parse_call(&mut self) -> Result<Option<Expr>, ParseError> {
        if !self.is_ident_start() {
            return Ok(None);
        }
        let checkpoint = self.snapshot();
        let (pos, name) = self.parse_identifier_no_spacing()?;
        if self.peek_char() != Some('(') {
            self.restore(checkpoint);
            return Ok(None);
        }
        self.expect_char('(')?;
        let mut args = Vec::new();
        if self.peek_char() != Some(')') {
            loop {
                args.push(self.parse_expression()?);
                if self.peek_char() == Some(',') {
                    self.next_char();
                    self.skip_spacing()?;
                } else {
                    break;
                }
            }
        }
        self.expect_char(')')?;
        Ok(Some(Expr::Call(pos, name, args)))
    }

    fn parse_identifier_with_spacing(&mut self) -> Result<(Position, String), ParseError> {
        let result = self.parse_identifier_no_spacing()?;
        self.skip_spacing()?;
        Ok(result)
    }

    fn parse_identifier_no_spacing(&mut self) -> Result<(Position, String), ParseError> {
        let pos = self.pos();
        let mut out = String::new();
        let ch = self
            .next_char()
            .filter(|c| Self::is_ident_start_char(*c))
            .ok_or_else(|| self.error("expected identifier"))?;
        out.push(ch);
        while let Some(ch) = self.peek_char() {
            if Self::is_ident_continue_char(ch) {
                out.push(self.next_char().expect("peeked char exists"));
            } else {
                break;
            }
        }
        Ok((pos, out))
    }

    fn skip_type_tree(&mut self) -> Result<(), ParseError> {
        self.skip_spacing()?;
        if self.peek_char() == Some('(') {
            self.expect_char('(')?;
            if self.peek_char() != Some(')') {
                loop {
                    self.skip_type_tree()?;
                    if self.peek_char() == Some(',') {
                        self.next_char();
                        self.skip_spacing()?;
                    } else {
                        break;
                    }
                }
            }
            self.expect_char(')')?;
            if self.peek_char() == Some('-') {
                self.next_char();
                self.expect_raw('>')?;
                self.skip_spacing()?;
                self.skip_type_tree()?;
            }
            Ok(())
        } else if self.peek_char() == Some('?') {
            self.next_char();
            self.skip_spacing()?;
            Ok(())
        } else {
            Err(self.error("expected type"))
        }
    }

    fn is_sequence_start(&mut self) -> bool {
        let _ = self.skip_spacing();
        matches!(
            self.peek_char(),
            Some('&' | '!' | '[' | '(' | '.' | '_' | '"' | 'a'..='z' | 'A'..='Z')
        ) && !matches!(self.peek_char(), Some('/' | '|' | ')' | ';'))
    }

    fn try_keyword(&mut self, keyword: &str) -> bool {
        let checkpoint = self.snapshot();
        for expected in keyword.chars() {
            if self.peek_char() != Some(expected) {
                self.restore(checkpoint);
                return false;
            }
            self.next_char();
        }
        if matches!(self.peek_char(), Some(ch) if Self::is_ident_continue_char(ch)) {
            self.restore(checkpoint);
            return false;
        }
        true
    }

    fn parse_escaped_char(&mut self, meta_char: Option<char>) -> Result<char, ParseError> {
        match self.next_char() {
            Some('\\') => match self.next_char() {
                Some('n') => Ok('\n'),
                Some('r') => Ok('\r'),
                Some('t') => Ok('\t'),
                Some('f') => Ok('\u{000C}'),
                Some('u') => {
                    let mut hex = String::new();
                    for _ in 0..4 {
                        let ch = self
                            .next_char()
                            .filter(|c| c.is_ascii_hexdigit())
                            .ok_or_else(|| self.error("expected unicode escape"))?;
                        hex.push(ch);
                    }
                    let value = u32::from_str_radix(&hex, 16)
                        .ok()
                        .and_then(char::from_u32)
                        .ok_or_else(|| self.error("invalid unicode escape"))?;
                    Ok(value)
                }
                Some(ch @ '0'..='7') => {
                    let mut oct = String::new();
                    oct.push(ch);
                    for _ in 0..2 {
                        match self.peek_char() {
                            Some(next @ '0'..='7') if oct.len() < 3 => {
                                oct.push(next);
                                self.next_char();
                            }
                            _ => break,
                        }
                    }
                    let value = u32::from_str_radix(&oct, 8)
                        .ok()
                        .and_then(char::from_u32)
                        .ok_or_else(|| self.error("invalid octal escape"))?;
                    Ok(value)
                }
                Some(ch) => Ok(ch),
                None => Err(self.error("unterminated escape")),
            },
            Some(ch) if meta_char.is_none_or(|meta| ch != meta) && ch != '\\' => Ok(ch),
            Some(_) => Err(self.error("invalid escaped character")),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn expect_char(&mut self, expected: char) -> Result<(), ParseError> {
        self.expect_raw(expected)?;
        self.skip_spacing()?;
        Ok(())
    }

    fn expect_raw(&mut self, expected: char) -> Result<(), ParseError> {
        match self.next_char() {
            Some(ch) if ch == expected => Ok(()),
            _ => Err(self.error(&format!("expected '{}'", expected))),
        }
    }

    fn skip_spacing(&mut self) -> Result<(), ParseError> {
        loop {
            let checkpoint = self.snapshot();
            let mut progressed = false;
            while matches!(self.peek_char(), Some(' ' | '\t' | '\r' | '\n')) {
                progressed = true;
                self.next_char();
            }
            if self.peek_char() == Some('/') && self.peek_next_char() == Some('/') {
                progressed = true;
                self.next_char();
                self.next_char();
                while !matches!(self.peek_char(), None | Some('\r' | '\n')) {
                    self.next_char();
                }
                if self.peek_char() == Some('\r') {
                    self.next_char();
                    if self.peek_char() == Some('\n') {
                        self.next_char();
                    }
                } else if self.peek_char() == Some('\n') {
                    self.next_char();
                }
            }
            if !progressed {
                self.restore(checkpoint);
                return Ok(());
            }
        }
    }

    fn pos(&self) -> Position {
        Position {
            line: self.line,
            column: self.column,
        }
    }

    fn snapshot(&self) -> (usize, usize, usize) {
        (self.idx, self.line, self.column)
    }

    fn restore(&mut self, snapshot: (usize, usize, usize)) {
        self.idx = snapshot.0;
        self.line = snapshot.1;
        self.column = snapshot.2;
    }

    fn error(&self, message: &str) -> ParseError {
        ParseError {
            pos: self.pos(),
            message: message.to_string(),
        }
    }

    fn is_ident_start(&self) -> bool {
        self.peek_char().is_some_and(Self::is_ident_start_char)
    }

    fn is_ident_start_char(ch: char) -> bool {
        ch == '_' || ch.is_ascii_alphabetic()
    }

    fn is_ident_continue_char(ch: char) -> bool {
        Self::is_ident_start_char(ch) || ch.is_ascii_digit()
    }

    fn is_eof(&self) -> bool {
        self.idx >= self.src.len()
    }

    fn peek_char(&self) -> Option<char> {
        self.src[self.idx..].chars().next()
    }

    fn peek_next_char(&self) -> Option<char> {
        let mut chars = self.src[self.idx..].chars();
        chars.next()?;
        chars.next()
    }

    fn next_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.idx += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.column = 0;
        } else {
            self.column += 1;
        }
        Some(ch)
    }
}

#[derive(Clone)]
pub struct Evaluator {
    functions: HashMap<String, Expr>,
    strategy: EvaluationStrategy,
}

impl Evaluator {
    pub fn new(grammar: Grammar, strategy: EvaluationStrategy) -> Self {
        let functions = grammar
            .rules
            .into_iter()
            .map(|rule| {
                let body = expand(rule.body);
                let expr = if rule.args.is_empty() {
                    body
                } else {
                    Expr::Function(rule.pos, rule.args, Box::new(body))
                };
                (rule.name, expr)
            })
            .collect();
        Self {
            functions,
            strategy,
        }
    }

    pub fn evaluate(&self, input: &str, start: &str) -> EvaluationResult {
        let Some(start_expr) = self.functions.get(start) else {
            return EvaluationResult::Failure;
        };
        self.eval(input, start_expr, &self.functions)
    }

    fn eval(&self, input: &str, expr: &Expr, bindings: &HashMap<String, Expr>) -> EvaluationResult {
        match expr {
            Expr::Debug(_, _) => EvaluationResult::Success(input.to_string()),
            Expr::Alternation(_, lhs, rhs) => {
                let left = self.eval(input, lhs, bindings);
                if left.is_success() {
                    left
                } else {
                    self.eval(input, rhs, bindings)
                }
            }
            Expr::Sequence(_, lhs, rhs) => match self.eval(input, lhs, bindings) {
                EvaluationResult::Success(rest) => self.eval(&rest, rhs, bindings),
                EvaluationResult::Failure => EvaluationResult::Failure,
            },
            Expr::AndPredicate(_, body) => match self.eval(input, body, bindings) {
                EvaluationResult::Success(_) => EvaluationResult::Success(input.to_string()),
                EvaluationResult::Failure => EvaluationResult::Failure,
            },
            Expr::NotPredicate(_, body) => match self.eval(input, body, bindings) {
                EvaluationResult::Success(_) => EvaluationResult::Failure,
                EvaluationResult::Failure => EvaluationResult::Success(input.to_string()),
            },
            Expr::Call(_, name, params) => self.eval_call(input, name, params, bindings),
            Expr::Identifier(_, name) => {
                let Some(body) = bindings.get(name) else {
                    return EvaluationResult::Failure;
                };
                self.eval(input, body, bindings)
            }
            Expr::Optional(_, body) => match self.eval(input, body, bindings) {
                ok @ EvaluationResult::Success(_) => ok,
                EvaluationResult::Failure => EvaluationResult::Success(input.to_string()),
            },
            Expr::Repeat0(_, body) => {
                let mut rest = input.to_string();
                loop {
                    match self.eval(&rest, body, bindings) {
                        EvaluationResult::Success(next) if next.len() < rest.len() => rest = next,
                        _ => return EvaluationResult::Success(rest),
                    }
                }
            }
            Expr::Repeat1(_, body) => match self.eval(input, body, bindings) {
                EvaluationResult::Failure => EvaluationResult::Failure,
                EvaluationResult::Success(mut rest) => loop {
                    match self.eval(&rest, body, bindings) {
                        EvaluationResult::Success(next) if next.len() < rest.len() => rest = next,
                        _ => return EvaluationResult::Success(rest),
                    }
                },
            },
            Expr::StringLiteral(_, target) => input
                .strip_prefix(target)
                .map(|rest| EvaluationResult::Success(rest.to_string()))
                .unwrap_or(EvaluationResult::Failure),
            Expr::CharSet(_, positive, set) => match input.chars().next() {
                Some(ch)
                    if (*positive && set.contains(&ch)) || (!*positive && !set.contains(&ch)) =>
                {
                    EvaluationResult::Success(input[ch.len_utf8()..].to_string())
                }
                _ => EvaluationResult::Failure,
            },
            Expr::Wildcard(_) => match input.chars().next() {
                Some(ch) => EvaluationResult::Success(input[ch.len_utf8()..].to_string()),
                None => EvaluationResult::Failure,
            },
            Expr::CharClass(_, _, _) => {
                unreachable!("character classes are expanded before evaluation")
            }
            Expr::Function(_, _, _) => unreachable!("functions are only invoked through call"),
        }
    }

    fn eval_call(
        &self,
        input: &str,
        name: &str,
        params: &[Expr],
        bindings: &HashMap<String, Expr>,
    ) -> EvaluationResult {
        let Some(fun) = bindings.get(name) else {
            return EvaluationResult::Failure;
        };
        let Expr::Function(_, args, body) = fun else {
            return EvaluationResult::Failure;
        };
        if args.len() != params.len() {
            return EvaluationResult::Failure;
        }
        let Some((call_input, new_bindings)) = self.bind_call_args(input, args, params, bindings)
        else {
            return EvaluationResult::Failure;
        };
        let mut scoped = bindings.clone();
        scoped.extend(new_bindings);
        self.eval(&call_input, body, &scoped)
    }

    fn bind_call_args(
        &self,
        input: &str,
        args: &[String],
        params: &[Expr],
        bindings: &HashMap<String, Expr>,
    ) -> Option<(String, HashMap<String, Expr>)> {
        match self.strategy {
            EvaluationStrategy::CallByName => Some((
                input.to_string(),
                args.iter()
                    .cloned()
                    .zip(params.iter().map(|expr| extract(expr, bindings)))
                    .collect(),
            )),
            EvaluationStrategy::CallByValueSeq => {
                let mut rest = input.to_string();
                let mut values = Vec::with_capacity(params.len());
                for param in params {
                    match self.eval(&rest, param, bindings) {
                        EvaluationResult::Success(next) => {
                            let consumed = rest[..rest.len() - next.len()].to_string();
                            values.push(Expr::StringLiteral(Position::dummy(), consumed));
                            rest = next;
                        }
                        EvaluationResult::Failure => return None,
                    }
                }
                Some((rest, args.iter().cloned().zip(values).collect()))
            }
            EvaluationStrategy::CallByValuePar => {
                let mut values = Vec::with_capacity(params.len());
                for param in params {
                    match self.eval(input, param, bindings) {
                        EvaluationResult::Success(rest) => {
                            let consumed = input[..input.len() - rest.len()].to_string();
                            values.push(Expr::StringLiteral(Position::dummy(), consumed));
                        }
                        EvaluationResult::Failure => return None,
                    }
                }
                Some((
                    input.to_string(),
                    args.iter().cloned().zip(values).collect(),
                ))
            }
        }
    }
}

fn expand(expr: Expr) -> Expr {
    match expr {
        Expr::CharClass(pos, positive, elems) => {
            let mut set = HashSet::new();
            for elem in elems {
                match elem {
                    CharClassElement::OneChar(ch) => {
                        set.insert(ch);
                    }
                    CharClassElement::CharRange(from, to) => {
                        for ch in from..=to {
                            set.insert(ch);
                        }
                    }
                }
            }
            Expr::CharSet(pos, positive, set)
        }
        Expr::Alternation(pos, lhs, rhs) => {
            Expr::Alternation(pos, Box::new(expand(*lhs)), Box::new(expand(*rhs)))
        }
        Expr::Sequence(pos, lhs, rhs) => {
            Expr::Sequence(pos, Box::new(expand(*lhs)), Box::new(expand(*rhs)))
        }
        Expr::Repeat0(pos, body) => Expr::Repeat0(pos, Box::new(expand(*body))),
        Expr::Repeat1(pos, body) => Expr::Repeat1(pos, Box::new(expand(*body))),
        Expr::Optional(pos, body) => Expr::Optional(pos, Box::new(expand(*body))),
        Expr::AndPredicate(pos, body) => Expr::AndPredicate(pos, Box::new(expand(*body))),
        Expr::NotPredicate(pos, body) => Expr::NotPredicate(pos, Box::new(expand(*body))),
        Expr::Debug(pos, body) => Expr::Debug(pos, Box::new(expand(*body))),
        Expr::Call(pos, name, args) => {
            Expr::Call(pos, name, args.into_iter().map(expand).collect())
        }
        other => other,
    }
}

fn extract(expr: &Expr, bindings: &HashMap<String, Expr>) -> Expr {
    match expr {
        Expr::Debug(pos, body) => Expr::Debug(*pos, Box::new(extract(body, bindings))),
        Expr::Alternation(pos, lhs, rhs) => Expr::Alternation(
            *pos,
            Box::new(extract(lhs, bindings)),
            Box::new(extract(rhs, bindings)),
        ),
        Expr::Sequence(pos, lhs, rhs) => Expr::Sequence(
            *pos,
            Box::new(extract(lhs, bindings)),
            Box::new(extract(rhs, bindings)),
        ),
        Expr::AndPredicate(pos, body) => {
            Expr::AndPredicate(*pos, Box::new(extract(body, bindings)))
        }
        Expr::NotPredicate(pos, body) => {
            Expr::NotPredicate(*pos, Box::new(extract(body, bindings)))
        }
        Expr::Call(pos, name, params) => Expr::Call(
            *pos,
            name.clone(),
            params
                .iter()
                .map(|param| extract(param, bindings))
                .collect(),
        ),
        Expr::Identifier(_, name) => bindings.get(name).cloned().unwrap_or_else(|| expr.clone()),
        Expr::Optional(pos, body) => Expr::Optional(*pos, Box::new(extract(body, bindings))),
        Expr::Repeat0(pos, body) => Expr::Repeat0(*pos, Box::new(extract(body, bindings))),
        Expr::Repeat1(pos, body) => Expr::Repeat1(*pos, Box::new(extract(body, bindings))),
        Expr::Function(_, _, _) => expr.clone(),
        Expr::StringLiteral(..) | Expr::Wildcard(..) | Expr::CharClass(..) | Expr::CharSet(..) => {
            expr.clone()
        }
    }
}

pub fn parse_macro_peg(input: &str) -> Result<Grammar, ParseError> {
    Parser::new(input).parse_grammar()
}

pub fn eval_grammar(
    source: &str,
    inputs: &[&str],
    strategy: EvaluationStrategy,
) -> Result<Vec<EvaluationResult>, ParseError> {
    let grammar = parse_macro_peg(source)?;
    let evaluator = Evaluator::new(grammar, strategy);
    Ok(inputs
        .iter()
        .map(|input| evaluator.evaluate(input, "S"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{EvaluationResult, EvaluationStrategy, eval_grammar, parse_macro_peg};

    #[test]
    fn parses_simple_macro_peg_grammar() {
        let grammar = parse_macro_peg(r#"S = "<" F([a-zA-Z_]+); F(N) = N ">" "</" N ">";"#)
            .expect("grammar should parse");
        assert_eq!(grammar.rules.len(), 2);
        assert_eq!(grammar.rules[0].name, "S");
        assert_eq!(grammar.rules[1].name, "F");
    }

    #[test]
    fn evaluates_call_by_name_palindrome_example() {
        let results = eval_grammar(
            r#"
              S = P("") !.;
              P(r) = "a" P("a" r) / "b" P("b" r) / r;
            "#,
            &["abba", "abba", "abbbba", "a"],
            EvaluationStrategy::CallByName,
        )
        .expect("grammar should evaluate");
        assert_eq!(
            results,
            vec![
                EvaluationResult::Success(String::new()),
                EvaluationResult::Success(String::new()),
                EvaluationResult::Success(String::new()),
                EvaluationResult::Failure,
            ]
        );
    }

    #[test]
    fn evaluates_call_by_value_seq_examples() {
        let simple = eval_grammar(
            r#"
              S = F("a", "b", "c"); F(A, B, C) = "abc";
            "#,
            &["abcabc"],
            EvaluationStrategy::CallByValueSeq,
        )
        .expect("simple grammar should evaluate");
        assert_eq!(simple, vec![EvaluationResult::Success(String::new())]);

        let xml = eval_grammar(
            r#"
              S = F("<", [a-zA-Z_]+, ">");
              F(LT, N, GT) = F("<", [a-zA-Z_]+, ">")* LT "/" N GT;
            "#,
            &["<a></a>"],
            EvaluationStrategy::CallByValueSeq,
        )
        .expect("xml grammar should evaluate");
        assert_eq!(xml, vec![EvaluationResult::Success(String::new())]);
    }

    #[test]
    fn evaluates_call_by_value_par_examples() {
        let simple = eval_grammar(
            r#"
              S = F("a"); F(A) = A A A;
            "#,
            &["aaa"],
            EvaluationStrategy::CallByValuePar,
        )
        .expect("simple grammar should evaluate");
        assert_eq!(simple, vec![EvaluationResult::Success(String::new())]);

        let xml = eval_grammar(
            r#"
              S = "<" F([a-zA-Z_]+);
              F(N) = N ">" ("<" F([a-zA-Z_]+))* "</" N ">";
            "#,
            &["<a><b></b></a>"],
            EvaluationStrategy::CallByValuePar,
        )
        .expect("xml grammar should evaluate");
        assert_eq!(xml, vec![EvaluationResult::Success(String::new())]);
    }
}
