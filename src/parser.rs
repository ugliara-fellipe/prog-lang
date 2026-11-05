use crate::ast::*;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ParseError {
    #[error("Syntax error at line {line}, column {col}: {message}")]
    SyntaxError {
        line: usize,
        col: usize,
        message: String,
    },
    #[error("Unexpected end of file")]
    UnexpectedEof,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Type,
    Func,
    Routine,
    Let,
    Mut,
    Spawn,
    For,
    Em,
    In,
    Match,
    Ref,
    List,
    // Symbols
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Colon,
    Semicolon,
    Comma,
    Dot,
    Arrow,    // ->
    FatArrow, // =>
    Assign,   // =
    // Operators
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Plus,
    Minus,
    Star,
    Slash,
    // Literals & Identifiers
    Id(String),
    Int(i64),
    Str(String),
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub col: usize,
}

pub struct Lexer<'a> {
    _src: &'a str,
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            _src: src,
            chars: src.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn peek(&self) -> Option<char> {
        if self.pos < self.chars.len() {
            Some(self.chars[self.pos])
        } else {
            None
        }
    }

    fn peek_next(&self) -> Option<char> {
        if self.pos + 1 < self.chars.len() {
            Some(self.chars[self.pos + 1])
        } else {
            None
        }
    }

    fn advance(&mut self) -> Option<char> {
        if self.pos < self.chars.len() {
            let ch = self.chars[self.pos];
            self.pos += 1;
            if ch == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
            Some(ch)
        } else {
            None
        }
    }

    pub fn next_token(&mut self) -> Result<Token, ParseError> {
        self.skip_whitespace_and_comments()?;

        let line = self.line;
        let col = self.col;

        let ch = match self.peek() {
            Some(c) => c,
            None => {
                return Ok(Token {
                    kind: TokenKind::Eof,
                    line,
                    col,
                })
            }
        };

        if ch.is_alphabetic() || ch == '_' {
            let mut ident = String::new();
            while let Some(c) = self.peek() {
                if c.is_alphanumeric() || c == '_' {
                    ident.push(c);
                    self.advance();
                } else {
                    break;
                }
            }

            let kind = match ident.as_str() {
                "type" => TokenKind::Type,
                "func" => TokenKind::Func,
                "routine" => TokenKind::Routine,
                "let" => TokenKind::Let,
                "mut" => TokenKind::Mut,
                "spawn" => TokenKind::Spawn,
                "for" => TokenKind::For,
                "in" => TokenKind::In,
                "em" => TokenKind::Em,
                "match" => TokenKind::Match,
                "ref" => TokenKind::Ref,
                "List" => TokenKind::List,
                _ => TokenKind::Id(ident),
            };

            return Ok(Token { kind, line, col });
        }

        if ch.is_ascii_digit() {
            let mut num_str = String::new();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    num_str.push(c);
                    self.advance();
                } else {
                    break;
                }
            }
            let val = num_str.parse::<i64>().map_err(|e| ParseError::SyntaxError {
                line,
                col,
                message: format!("Invalid integer literal: {}", e),
            })?;
            return Ok(Token {
                kind: TokenKind::Int(val),
                line,
                col,
            });
        }

        if ch == '"' {
            self.advance(); // consume opening quote
            let mut str_val = String::new();
            while let Some(c) = self.peek() {
                if c == '"' {
                    self.advance(); // consume closing quote
                    return Ok(Token {
                        kind: TokenKind::Str(str_val),
                        line,
                        col,
                    });
                } else if c == '\\' {
                    self.advance();
                    if let Some(escaped) = self.advance() {
                        match escaped {
                            'n' => str_val.push('\n'),
                            't' => str_val.push('\t'),
                            'r' => str_val.push('\r'),
                            '\\' => str_val.push('\\'),
                            '"' => str_val.push('"'),
                            other => str_val.push(other),
                        }
                    }
                } else {
                    str_val.push(c);
                    self.advance();
                }
            }
            return Err(ParseError::SyntaxError {
                line,
                col,
                message: "Unterminated string literal".to_string(),
            });
        }

        // Two-character operators
        if ch == '-' && self.peek_next() == Some('>') {
            self.advance();
            self.advance();
            return Ok(Token {
                kind: TokenKind::Arrow,
                line,
                col,
            });
        }
        if ch == '=' && self.peek_next() == Some('>') {
            self.advance();
            self.advance();
            return Ok(Token {
                kind: TokenKind::FatArrow,
                line,
                col,
            });
        }
        if ch == '=' && self.peek_next() == Some('=') {
            self.advance();
            self.advance();
            return Ok(Token {
                kind: TokenKind::EqEq,
                line,
                col,
            });
        }
        if ch == '!' && self.peek_next() == Some('=') {
            self.advance();
            self.advance();
            return Ok(Token {
                kind: TokenKind::NotEq,
                line,
                col,
            });
        }
        if ch == '<' && self.peek_next() == Some('=') {
            self.advance();
            self.advance();
            return Ok(Token {
                kind: TokenKind::LtEq,
                line,
                col,
            });
        }
        if ch == '>' && self.peek_next() == Some('=') {
            self.advance();
            self.advance();
            return Ok(Token {
                kind: TokenKind::GtEq,
                line,
                col,
            });
        }

        // Single-character symbols
        self.advance();
        let kind = match ch {
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ':' => TokenKind::Colon,
            ';' => TokenKind::Semicolon,
            ',' => TokenKind::Comma,
            '.' => TokenKind::Dot,
            '=' => TokenKind::Assign,
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '<' => TokenKind::Lt,
            '>' => TokenKind::Gt,
            other => {
                return Err(ParseError::SyntaxError {
                    line,
                    col,
                    message: format!("Unexpected character: '{}'", other),
                })
            }
        };

        Ok(Token { kind, line, col })
    }

    fn skip_whitespace_and_comments(&mut self) -> Result<(), ParseError> {
        loop {
            while let Some(c) = self.peek() {
                if c.is_whitespace() {
                    self.advance();
                } else {
                    break;
                }
            }

            if self.peek() == Some('/') && self.peek_next() == Some('/') {
                // Line comment
                while let Some(c) = self.peek() {
                    self.advance();
                    if c == '\n' {
                        break;
                    }
                }
            } else if self.peek() == Some('/') && self.peek_next() == Some('*') {
                // Block comment
                self.advance();
                self.advance();
                let start_line = self.line;
                let start_col = self.col;
                let mut closed = false;
                while let Some(c) = self.peek() {
                    if c == '*' && self.peek_next() == Some('/') {
                        self.advance();
                        self.advance();
                        closed = true;
                        break;
                    }
                    self.advance();
                }
                if !closed {
                    return Err(ParseError::SyntaxError {
                        line: start_line,
                        col: start_col,
                        message: "Unclosed block comment".to_string(),
                    });
                }
            } else {
                break;
            }
        }
        Ok(())
    }
}

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Token,
}

impl<'a> Parser<'a> {
    pub fn new(src: &'a str) -> Result<Self, ParseError> {
        let mut lexer = Lexer::new(src);
        let current = lexer.next_token()?;
        Ok(Self { lexer, current })
    }

    fn advance(&mut self) -> Result<Token, ParseError> {
        let prev = std::mem::replace(&mut self.current, self.lexer.next_token()?);
        Ok(prev)
    }

    fn check(&self, kind: &TokenKind) -> bool {
        &self.current.kind == kind
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        if self.current.kind == kind {
            self.advance()
        } else {
            Err(ParseError::SyntaxError {
                line: self.current.line,
                col: self.current.col,
                message: format!("Expected {:?}, found {:?}", kind, self.current.kind),
            })
        }
    }

    fn expect_id(&mut self) -> Result<String, ParseError> {
        match &self.current.kind {
            TokenKind::Id(name) => {
                let name = name.clone();
                self.advance()?;
                Ok(name)
            }
            TokenKind::List => {
                self.advance()?;
                Ok("List".to_string())
            }
            other => Err(ParseError::SyntaxError {
                line: self.current.line,
                col: self.current.col,
                message: format!("Expected identifier, found {:?}", other),
            }),
        }
    }

    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut declarations = Vec::new();
        while self.current.kind != TokenKind::Eof {
            declarations.push(self.parse_top_level_decl()?);
        }
        Ok(Program { declarations })
    }

    fn parse_top_level_decl(&mut self) -> Result<TopLevelDecl, ParseError> {
        match self.current.kind {
            TokenKind::Type => self.parse_type_decl().map(TopLevelDecl::Type),
            TokenKind::Func => self.parse_func_decl().map(TopLevelDecl::Func),
            TokenKind::Routine => self.parse_routine_decl().map(TopLevelDecl::Routine),
            ref other => Err(ParseError::SyntaxError {
                line: self.current.line,
                col: self.current.col,
                message: format!(
                    "Unexpected top-level declaration: expected 'type', 'func', or 'routine', found {:?}",
                    other
                ),
            }),
        }
    }

    fn parse_type_decl(&mut self) -> Result<TypeDecl, ParseError> {
        self.expect(TokenKind::Type)?;
        let name = self.expect_id()?;
        self.expect(TokenKind::LBrace)?;

        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let field_name = self.expect_id()?;
            self.expect(TokenKind::Colon)?;
            let field_type = self.parse_type_spec()?;
            fields.push(StructField {
                name: field_name,
                field_type,
            });

            if self.check(&TokenKind::Comma) {
                self.advance()?;
            } else {
                break;
            }
        }
        self.expect(TokenKind::RBrace)?;
        Ok(TypeDecl { name, fields })
    }

    fn parse_func_decl(&mut self) -> Result<FuncDecl, ParseError> {
        self.expect(TokenKind::Func)?;
        let name = self.expect_id()?;
        self.expect(TokenKind::LParen)?;

        let mut params = Vec::new();
        while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
            let param_name = self.expect_id()?;
            self.expect(TokenKind::Colon)?;
            let param_type = self.parse_type_spec()?;
            params.push(Param {
                name: param_name,
                param_type,
            });

            if self.check(&TokenKind::Comma) {
                self.advance()?;
            } else {
                break;
            }
        }
        self.expect(TokenKind::RParen)?;

        let return_type = if self.check(&TokenKind::Arrow) {
            self.advance()?;
            Some(self.parse_type_spec()?)
        } else {
            None
        };

        let body = self.parse_block()?;
        Ok(FuncDecl {
            name,
            params,
            return_type,
            body,
        })
    }

    fn parse_routine_decl(&mut self) -> Result<RoutineDecl, ParseError> {
        self.expect(TokenKind::Routine)?;
        let name = self.expect_id()?;
        self.expect(TokenKind::LParen)?;

        let mut params = Vec::new();
        while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
            let param_name = self.expect_id()?;
            self.expect(TokenKind::Colon)?;
            let param_type = self.parse_type_spec()?;
            params.push(Param {
                name: param_name,
                param_type,
            });

            if self.check(&TokenKind::Comma) {
                self.advance()?;
            } else {
                break;
            }
        }
        self.expect(TokenKind::RParen)?;

        let body = self.parse_block()?;
        Ok(RoutineDecl { name, params, body })
    }

    fn parse_type_spec(&mut self) -> Result<TypeSpec, ParseError> {
        let name = self.expect_id()?;
        if self.check(&TokenKind::LBracket) {
            self.advance()?;
            let mut inner = Vec::new();
            while !self.check(&TokenKind::RBracket) && !self.check(&TokenKind::Eof) {
                inner.push(self.parse_type_spec()?);
                if self.check(&TokenKind::Comma) {
                    self.advance()?;
                } else {
                    break;
                }
            }
            self.expect(TokenKind::RBracket)?;
            if name == "Ref" && inner.len() == 1 {
                Ok(TypeSpec::Ref(Box::new(inner.remove(0))))
            } else {
                Ok(TypeSpec::Generic(name, inner))
            }
        } else {
            Ok(TypeSpec::Primitive(name))
        }
    }

    fn parse_block(&mut self) -> Result<Vec<Statement>, ParseError> {
        self.expect(TokenKind::LBrace)?;
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            stmts.push(self.parse_statement()?);
        }
        self.expect(TokenKind::RBrace)?;
        Ok(stmts)
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        match self.current.kind {
            TokenKind::Let => self.parse_var_decl(),
            TokenKind::Spawn => self.parse_spawn(),
            TokenKind::For => self.parse_for(),
            TokenKind::Match => self.parse_match(),
            _ => {
                let expr = self.parse_expr()?;
                self.expect(TokenKind::Semicolon)?;
                Ok(Statement::ExprStmt(expr))
            }
        }
    }

    fn parse_var_decl(&mut self) -> Result<Statement, ParseError> {
        self.expect(TokenKind::Let)?;
        let is_mutable = if self.check(&TokenKind::Mut) {
            self.advance()?;
            true
        } else {
            false
        };

        let name = self.expect_id()?;
        let type_annot = if self.check(&TokenKind::Colon) {
            self.advance()?;
            Some(self.parse_type_spec()?)
        } else {
            None
        };

        self.expect(TokenKind::Assign)?;
        let value = self.parse_expr()?;
        self.expect(TokenKind::Semicolon)?;

        Ok(Statement::VarDecl {
            name,
            is_mutable,
            type_annot,
            value,
        })
    }

    fn parse_spawn(&mut self) -> Result<Statement, ParseError> {
        self.expect(TokenKind::Spawn)?;
        let expr = self.parse_expr()?;
        self.expect(TokenKind::Semicolon)?;
        Ok(Statement::Spawn(expr))
    }

    fn parse_for(&mut self) -> Result<Statement, ParseError> {
        self.expect(TokenKind::For)?;
        let var_name = self.expect_id()?;
        if self.check(&TokenKind::In) || self.check(&TokenKind::Em) {
            self.advance()?;
        } else {
            return Err(ParseError::SyntaxError {
                line: self.current.line,
                col: self.current.col,
                message: "Expected 'in' after variable in for loop".to_string(),
            });
        }

        let collection = self.parse_expr()?;
        let body = self.parse_block()?;

        Ok(Statement::ForLoop {
            var_name,
            collection,
            body,
        })
    }

    fn parse_match(&mut self) -> Result<Statement, ParseError> {
        self.expect(TokenKind::Match)?;
        let target = self.parse_expr()?;
        self.expect(TokenKind::LBrace)?;

        let mut cases = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let pattern = self.parse_pattern()?;
            self.expect(TokenKind::FatArrow)?;

            let body = if self.check(&TokenKind::LBrace) {
                self.parse_block()?
            } else {
                let expr = self.parse_expr()?;
                vec![Statement::ExprStmt(expr)]
            };

            cases.push(MatchCase { pattern, body });

            if self.check(&TokenKind::Comma) {
                self.advance()?;
            } else if !self.check(&TokenKind::RBrace) {
                // If the case ended with a block {}, comma is optional
            }
        }
        self.expect(TokenKind::RBrace)?;

        Ok(Statement::Match { target, cases })
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        let name = self.expect_id()?;
        if name == "_" {
            return Ok(Pattern::Wildcard);
        }

        if self.check(&TokenKind::LParen) {
            self.advance()?;
            let mut sub_pats = Vec::new();
            while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
                sub_pats.push(self.parse_pattern()?);
                if self.check(&TokenKind::Comma) {
                    self.advance()?;
                } else {
                    break;
                }
            }
            self.expect(TokenKind::RParen)?;
            Ok(Pattern::Constructor(name, sub_pats))
        } else {
            Ok(Pattern::Identifier(name))
        }
    }

    pub fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_relational()
    }

    fn parse_relational(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_additive()?;
        while matches!(
            self.current.kind,
            TokenKind::EqEq
                | TokenKind::NotEq
                | TokenKind::Lt
                | TokenKind::LtEq
                | TokenKind::Gt
                | TokenKind::GtEq
        ) {
            let op = match self.advance()?.kind {
                TokenKind::EqEq => "==",
                TokenKind::NotEq => "!=",
                TokenKind::Lt => "<",
                TokenKind::LtEq => "<=",
                TokenKind::Gt => ">",
                TokenKind::GtEq => ">=",
                _ => unreachable!(),
            };
            let right = self.parse_additive()?;
            left = Expr::BinaryOp {
                op: op.to_string(),
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplicative()?;
        while matches!(self.current.kind, TokenKind::Plus | TokenKind::Minus) {
            let op = match self.advance()?.kind {
                TokenKind::Plus => "+",
                TokenKind::Minus => "-",
                _ => unreachable!(),
            };
            let right = self.parse_multiplicative()?;
            left = Expr::BinaryOp {
                op: op.to_string(),
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_prefix()?;
        while matches!(self.current.kind, TokenKind::Star | TokenKind::Slash) {
            let op = match self.advance()?.kind {
                TokenKind::Star => "*",
                TokenKind::Slash => "/",
                _ => unreachable!(),
            };
            let right = self.parse_prefix()?;
            left = Expr::BinaryOp {
                op: op.to_string(),
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_prefix(&mut self) -> Result<Expr, ParseError> {
        if self.check(&TokenKind::Ref) {
            self.advance()?;
            let inner = self.parse_prefix()?;
            Ok(Expr::RefCreate(Box::new(inner)))
        } else {
            self.parse_postfix()
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.check(&TokenKind::LParen) {
                // Function call
                self.advance()?;
                let mut args = Vec::new();
                while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
                    args.push(self.parse_expr()?);
                    if self.check(&TokenKind::Comma) {
                        self.advance()?;
                    } else {
                        break;
                    }
                }
                self.expect(TokenKind::RParen)?;

                match expr {
                    Expr::Variable(name) => {
                        expr = Expr::FunctionCall { name, args };
                    }
                    _ => {
                        return Err(ParseError::SyntaxError {
                            line: self.current.line,
                            col: self.current.col,
                            message: "Dynamic function calls are not supported".to_string(),
                        });
                    }
                }
            } else if self.check(&TokenKind::Dot) {
                // Member access
                self.advance()?;
                let field = self.expect_id()?;
                expr = Expr::MemberAccess {
                    target: Box::new(expr),
                    field,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match &self.current.kind {
            TokenKind::Int(val) => {
                let v = *val;
                self.advance()?;
                Ok(Expr::IntLiteral(v))
            }
            TokenKind::Str(val) => {
                let s = val.clone();
                self.advance()?;
                Ok(Expr::StringLiteral(s))
            }
            TokenKind::Id(name) => {
                let n = name.clone();
                self.advance()?;
                Ok(Expr::Variable(n))
            }
            TokenKind::List => {
                self.advance()?;
                Ok(Expr::Variable("List".to_string()))
            }
            TokenKind::LParen => {
                self.advance()?;
                let expr = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Ok(expr)
            }
            other => Err(ParseError::SyntaxError {
                line: self.current.line,
                col: self.current.col,
                message: format!("Unexpected primary expression: {:?}", other),
            }),
        }
    }
}
