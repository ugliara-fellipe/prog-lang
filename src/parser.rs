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
    DocComment(String),
    Type,
    Enum,
    Func,
    Routine,
    Loop,
    Import,
    Let,
    Const,
    Mut,
    Spawn,
    Signal,
    Try,
    Catch,
    Throw,
    TailCall,
    Terminate,
    For,
    Em,
    In,
    Match,
    Ref,
    Receive,
    Memory,
    List,
    // Symbols
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Colon,
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
    Pipe,
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

#[derive(Clone)]
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

        if self.is_doc_comment() {
            self.advance();self.advance();self.advance();
            if self.peek()==Some(' ') {self.advance();}
            let mut content=String::new();
            while let Some(c)=self.peek(){if c=='\n' || c=='\r'{break;}content.push(c);self.advance();}
            return Ok(Token{kind:TokenKind::DocComment(content),line,col});
        }

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
                "enum" => TokenKind::Enum,
                "func" => TokenKind::Func,
                "routine" => TokenKind::Routine,
                "loop" => TokenKind::Loop,
                "import" => TokenKind::Import,
                "let" => TokenKind::Let,
                "const" => TokenKind::Const,
                "mut" => TokenKind::Mut,
                "spawn" => TokenKind::Spawn,
                "signal" => TokenKind::Signal,
                "try" => TokenKind::Try,
                "catch" => TokenKind::Catch,
                "throw" => TokenKind::Throw,
                "tailcall" => TokenKind::TailCall,
                "terminate" => TokenKind::Terminate,
                "for" => TokenKind::For,
                "in" => TokenKind::In,
                "em" => TokenKind::Em,
                "match" => TokenKind::Match,
                "ref" => TokenKind::Ref,
                "message" => TokenKind::Receive,
                "memory" => TokenKind::Memory,
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
            ',' => TokenKind::Comma,
            '.' => TokenKind::Dot,
            '=' => TokenKind::Assign,
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '|' => TokenKind::Pipe,
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

    fn is_doc_comment(&self)->bool {
        self.peek()==Some('/') && self.peek_next()==Some('/')
            && self.chars.get(self.pos+2)==Some(&'/') && self.chars.get(self.pos+3)!=Some(&'/')
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

            if self.is_doc_comment(){break;}
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

#[derive(Clone)]
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
        let mut imports = Vec::new();
        while self.current.kind == TokenKind::Import {
            let imp = self.parse_import_decl()?;
            imports.push(imp);
        }

        let mut declarations = Vec::new();
        for imp in &imports {
            declarations.push(TopLevelDecl::Import(imp.clone()));
        }

        while self.current.kind != TokenKind::Eof {
            declarations.push(self.parse_top_level_decl()?);
        }
        Ok(Program { imports, declarations })
    }

    fn parse_import_decl(&mut self) -> Result<ImportDecl, ParseError> {
        self.expect(TokenKind::Import)?;
        // A quoted package stem permits importing fixture/libraries whose
        // filename begins with a digit, such as "03_structural_recursion".
        // Ordinary identifier imports retain the concise `import math` form.
        let package = match &self.current.kind {
            TokenKind::Id(_) => self.expect_id()?,
            TokenKind::Str(value) => {
                let value = value.clone();
                self.advance()?;
                value
            }
            _ => {
                return Err(ParseError::SyntaxError {
                    line: self.current.line,
                    col: self.current.col,
                    message: "Expected package identifier or quoted package filename".to_string(),
                });
            }
        };

        let kind = if self.check(&TokenKind::Dot) {
            self.advance()?;
            if self.check(&TokenKind::LBrace) {
                self.advance()?;
                let mut items = Vec::new();
                while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
                    items.push(self.expect_id()?);
                    if self.check(&TokenKind::Comma) {
                        self.advance()?;
                    } else {
                        break;
                    }
                }
                self.expect(TokenKind::RBrace)?;
                ImportKind::Items(items)
            } else {
                let single = self.expect_id()?;
                ImportKind::Single(single)
            }
        } else {
            ImportKind::All
        };

        Ok(ImportDecl { package, kind })
    }

    fn parse_documentation(&mut self)->Result<String,ParseError>{
        let mut lines=Vec::new();
        while let TokenKind::DocComment(text)=&self.current.kind{lines.push(text.clone());self.advance()?;}
        Ok(lines.join("\n"))
    }

    fn documentation_error(&self)->ParseError{
        ParseError::SyntaxError{line:self.current.line,col:self.current.col,message:"documentation must precede a declaration, field or enum variant".into()}
    }

    fn parse_top_level_decl(&mut self) -> Result<TopLevelDecl, ParseError> {
        if matches!(&self.current.kind,TokenKind::DocComment(_)){
            let documentation=self.parse_documentation()?;
            if !matches!(self.current.kind,TokenKind::Const|TokenKind::Type|TokenKind::Enum|TokenKind::Func|TokenKind::Routine|TokenKind::Loop){return Err(self.documentation_error());}
            let mut decl=self.parse_top_level_decl()?;
            match &mut decl{
                TopLevelDecl::Const(c)=>c.documentation=documentation,
                TopLevelDecl::Type(t)=>t.documentation=documentation,
                TopLevelDecl::Func(f)=>f.documentation=documentation,
                TopLevelDecl::Routine(r)=>r.documentation=documentation,
                TopLevelDecl::Loop(l)=>l.documentation=documentation,
                _=>unreachable!()
            }
            return Ok(decl);
        }
        match self.current.kind {
            TokenKind::Const => self.parse_const_decl().map(TopLevelDecl::Const),
            TokenKind::Import => self.parse_import_decl().map(TopLevelDecl::Import),
            TokenKind::Type => self.parse_type_decl().map(TopLevelDecl::Type),
            TokenKind::Enum => self.parse_enum_decl().map(TopLevelDecl::Type),
            TokenKind::Func => self.parse_func_decl().map(TopLevelDecl::Func),
            TokenKind::Routine => self.parse_routine_decl().map(TopLevelDecl::Routine),
            TokenKind::Loop => self.parse_loop_decl().map(TopLevelDecl::Loop),
            ref other => Err(ParseError::SyntaxError {
                line: self.current.line,
                col: self.current.col,
                message: format!(
                    "Unexpected top-level declaration: expected 'import', 'const', 'type', 'enum', 'func', 'routine', or 'loop', found {:?}",
                    other
                ),
            }),
        }
    }

    fn parse_type_decl(&mut self) -> Result<TypeDecl, ParseError> {
        self.expect(TokenKind::Type)?;
        let name = self.expect_id()?;
        let type_params = self.parse_type_params()?;
        if self.check(&TokenKind::Assign) {
            self.advance()?;
            let mut variants=vec![self.parse_enum_variant()?];
            while self.check(&TokenKind::Pipe) {
                self.advance()?;
                let variant=self.parse_enum_variant()?;
                if variants.iter().any(|v|v.name==variant.name){return Err(ParseError::SyntaxError{line:self.current.line,col:self.current.col,message:format!("duplicate variant {}",variant.name)});}
                variants.push(variant);
            }
            return Ok(TypeDecl{documentation:String::new(),name,type_params,fields:vec![],variants});
        }
        self.expect(TokenKind::LBrace)?;

        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let documentation=self.parse_documentation()?;
            let field_name = self.expect_id()?;
            self.expect(TokenKind::Colon)?;
            let field_type = self.parse_type_spec()?;
            fields.push(StructField {
                documentation,
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
        Ok(TypeDecl {documentation:String::new(), name, type_params, fields, variants: vec![] })
    }

    fn parse_enum_decl(&mut self) -> Result<TypeDecl, ParseError> {
        self.expect(TokenKind::Enum)?;
        let name=self.expect_id()?;
        let type_params=self.parse_type_params()?;
        self.expect(TokenKind::LBrace)?;
        let mut variants=Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let variant=self.parse_enum_variant()?;
            if variants.iter().any(|v:&EnumVariant|v.name==variant.name){return Err(ParseError::SyntaxError{line:self.current.line,col:self.current.col,message:format!("duplicate variant {}",variant.name)});}
            variants.push(variant);
            if !self.check(&TokenKind::Comma) {break;} self.advance()?;
        }
        self.expect(TokenKind::RBrace)?;
        if variants.is_empty() {return Err(ParseError::SyntaxError {line:self.current.line,col:self.current.col,message:"enum must declare at least one variant".into()});}
        Ok(TypeDecl {documentation:String::new(),name,type_params,fields:vec![],variants})
    }

    fn parse_enum_variant(&mut self)->Result<EnumVariant,ParseError>{
        let documentation=self.parse_documentation()?;
        let name=self.expect_id()?;
        if matches!(name.as_str(),"True"|"False"|"Nil"|"Cons") || !name.chars().next().is_some_and(char::is_uppercase){return Err(ParseError::SyntaxError{line:self.current.line,col:self.current.col,message:"variant names must be capitalized and must not replace builtin constructors".into()});}
        let mut fields=Vec::new();
        if self.check(&TokenKind::LParen){
            self.advance()?;
            while !self.check(&TokenKind::RParen){fields.push(self.parse_type_spec()?);if !self.check(&TokenKind::Comma){break;}self.advance()?;}
            self.expect(TokenKind::RParen)?;
        }
        Ok(EnumVariant{documentation,name,fields})
    }

    fn parse_func_decl(&mut self) -> Result<FuncDecl, ParseError> {
        self.expect(TokenKind::Func)?;
        let name = self.expect_id()?;
        let type_params = self.parse_type_params()?;
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
            documentation:String::new(),
            name,
            type_params,
            params,
            return_type,
            body,
        })
    }

    fn parse_routine_decl(&mut self) -> Result<RoutineDecl, ParseError> {
        self.expect(TokenKind::Routine)?;
        let name = self.expect_id()?;
        let type_params = self.parse_type_params()?;
        self.expect(TokenKind::LParen)?;

        let mut params = Vec::new();
        while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
            let param_name = self.expect_id()?;
            self.expect(TokenKind::Colon)?;
            let param_type = self.parse_type_spec()?;
            // Disallow Ref<T> in routine parameters (must be copyable)
            if let TypeSpec::Ref(_) = param_type {
                return Err(ParseError::SyntaxError {
                    line: self.current.line,
                    col: self.current.col,
                    message: "Ref types are not allowed in routine parameters".to_string(),
                });
            }
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
        Ok(RoutineDecl {documentation:String::new(), name, type_params, params, body })
    }

    fn parse_type_params(&mut self) -> Result<Vec<String>, ParseError> {
        if !self.check(&TokenKind::Lt) { return Ok(Vec::new()); }
        self.advance()?;
        let mut params = Vec::new();
        loop {
            let name = self.expect_id()?;
            if params.contains(&name) {
                return Err(ParseError::SyntaxError { line: self.current.line, col: self.current.col, message: format!("Duplicate type parameter '{}'", name) });
            }
            params.push(name);
            if !self.check(&TokenKind::Comma) { break; }
            self.advance()?;
        }
        if self.check(&TokenKind::GtEq) {
            // In a sum declaration, `Name<T>=...` closes the parameter list
            // and starts the definition; preserve the already-lexed `=`.
            self.current.kind=TokenKind::Assign;self.current.col+=1;
        } else {self.expect(TokenKind::Gt)?;}
        Ok(params)
    }

    fn parse_loop_decl(&mut self) -> Result<LoopDecl, ParseError> {
        self.expect(TokenKind::Loop)?;
        let name = self.expect_id()?;
        self.expect(TokenKind::LParen)?;
        if !self.check(&TokenKind::RParen) {
            return Err(ParseError::SyntaxError {
                line: self.current.line,
                col: self.current.col,
                message: "Loops cannot declare parameters; use `message` for signal values".to_string(),
            });
        }
        self.expect(TokenKind::RParen)?;
        let body = self.parse_block()?;
        Ok(LoopDecl {documentation:String::new(), name, body })
    }

    fn parse_type_spec(&mut self) -> Result<TypeSpec, ParseError> {
        if self.check(&TokenKind::LParen) {
            self.advance()?;
            let mut params = Vec::new();
            while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
                params.push(self.parse_type_spec()?);
                if !self.check(&TokenKind::Comma) { break; }
                self.advance()?;
            }
            self.expect(TokenKind::RParen)?;
            self.expect(TokenKind::Arrow)?;
            return Ok(TypeSpec::Function(params, Box::new(self.parse_type_spec()?)));
        }
        let mut name = self.expect_id()?;
        while self.check(&TokenKind::Dot) {
            self.advance()?;
            let item = self.expect_id()?;
            name = format!("{}.{}", name, item);
        }
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
        if matches!(&self.current.kind,TokenKind::DocComment(_)){
            let documentation=self.parse_documentation()?;
            if !self.check(&TokenKind::Const){return Err(self.documentation_error());}
            let mut decl=self.parse_const_decl()?;decl.documentation=documentation;
            return Ok(Statement::Const(decl));
        }
        match self.current.kind {
            TokenKind::Const => self.parse_const_decl().map(Statement::Const),
            TokenKind::Let => self.parse_var_decl(),
            TokenKind::Memory => self.parse_memory(),
            TokenKind::Spawn => self.parse_spawn(),
            TokenKind::Signal => self.parse_signal(),
            TokenKind::Throw => self.parse_throw(),
            TokenKind::Try => self.parse_try_catch(),
            TokenKind::Terminate => {
                self.advance()?;
                Ok(Statement::Terminate)
            }
            TokenKind::For => self.parse_for(),
            TokenKind::Match => self.parse_match(),
            _ => {
                let expr = self.parse_expr()?;
                Ok(Statement::ExprStmt(expr))
            }
        }
    }

    fn parse_const_decl(&mut self) -> Result<ConstDecl,ParseError> {
        self.expect(TokenKind::Const)?;
        let name=self.expect_id()?;
        let type_annot=if self.check(&TokenKind::Colon){self.advance()?;Some(self.parse_type_spec()?)}else{None};
        self.expect(TokenKind::Assign)?;
        Ok(ConstDecl{documentation:String::new(),name,type_annot,value:self.parse_expr()?})
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

        Ok(Statement::VarDecl {
            name,
            is_mutable,
            type_annot,
            value,
        })
    }

    fn parse_memory(&mut self) -> Result<Statement, ParseError> {
        self.expect(TokenKind::Memory)?;
        let name = self.expect_id()?;
        self.expect(TokenKind::Assign)?;
        let value = self.parse_expr()?;
        Ok(Statement::MemorySet { name, value })
    }

    fn parse_spawn(&mut self) -> Result<Statement, ParseError> {
        self.expect(TokenKind::Spawn)?;
        let expr = self.parse_expr()?;
        Ok(Statement::Spawn(expr))
    }

    fn parse_signal(&mut self) -> Result<Statement, ParseError> {
        self.expect(TokenKind::Signal)?;
        let loop_name = self.expect_id()?;
        self.expect(TokenKind::LParen)?;
        let payload = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        Ok(Statement::Signal { loop_name, payload })
    }

    fn parse_throw(&mut self) -> Result<Statement, ParseError> {
        self.expect(TokenKind::Throw)?;
        Ok(Statement::Throw(self.parse_expr()?))
    }

    fn parse_try_catch(&mut self) -> Result<Statement, ParseError> {
        self.expect(TokenKind::Try)?;
        let try_body = self.parse_block()?;
        self.expect(TokenKind::Catch)?;
        let error_name = self.expect_id()?;
        let catch_body = self.parse_block()?;
        Ok(Statement::TryCatch { try_body, error_name, catch_body })
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
        let mut name = self.expect_id()?;
        while self.check(&TokenKind::Dot) {self.advance()?; name.push('.');name.push_str(&self.expect_id()?);}
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
        } else if name.rsplit('.').next().and_then(|n|n.chars().next()).is_some_and(char::is_uppercase) {
            // These are zero-argument constructors, not variable bindings.
            // Keeping them as constructors lets the runtime distinguish an
            // empty list (or boolean branch) from a catch-all identifier.
            Ok(Pattern::Constructor(name, Vec::new()))
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
        if self.check(&TokenKind::TailCall) {
            self.advance()?;
            let call=self.parse_postfix()?;
            if !matches!(call,Expr::FunctionCall{..}|Expr::IndirectCall{..}) {return Err(ParseError::SyntaxError{line:self.current.line,col:self.current.col,message:"tailcall requires a function call".into()});}
            Ok(Expr::TailCall(Box::new(call)))
        } else if self.check(&TokenKind::Type) {
            self.advance()?;
            Ok(Expr::TypeOf(self.expect_id()?))
        } else if self.check(&TokenKind::Ref) {
            self.advance()?;
            let inner = self.parse_prefix()?;
            Ok(Expr::RefCreate(Box::new(inner)))
        } else {
            self.parse_postfix()
        }
    }

    fn qualified_expr(expr:&Expr)->Option<String> {
        match expr {Expr::Variable(n)=>Some(n.clone()),Expr::MemberAccess{target,field}=>Some(format!("{}.{}",Self::qualified_expr(target)?,field)),_=>None}
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            // A tight '<' after a named expression introduces explicit type
            // arguments only if a complete type list can be read. Spaces
            // before '<' retain the ordinary relational interpretation.
            if self.check(&TokenKind::Lt)
                && matches!(expr, Expr::Variable(_) | Expr::MemberAccess { .. })
                && self.lexer.pos >= 2
                && !self.lexer.chars[self.lexer.pos - 2].is_whitespace()
            {
                let mut trial = self.clone();
                if let Ok(type_args) = trial.parse_explicit_type_args() {
                    *self = trial;
                    expr = Expr::Specialize { target: Box::new(expr), type_args };
                    continue;
                }
            }
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
                    Expr::MemberAccess { target, field } => {
                        if let Some(pkg) = Self::qualified_expr(&target) {
                            expr = Expr::FunctionCall {
                                name: format!("{}.{}", pkg, field),
                                args,
                            };
                        } else {
                            return Err(ParseError::SyntaxError {
                                line: self.current.line,
                                col: self.current.col,
                                message: "Dynamic function calls are not supported".to_string(),
                            });
                        }
                    }
                    target => expr = Expr::IndirectCall { target: Box::new(target), args },
                }
            } else if self.check(&TokenKind::LBracket) {
                self.advance()?;let key=self.parse_expr()?;self.expect(TokenKind::RBracket)?;
                expr=Expr::Index{target:Box::new(expr),key:Box::new(key)};
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

    fn parse_explicit_type_args(&mut self) -> Result<Vec<TypeSpec>, ParseError> {
        self.expect(TokenKind::Lt)?;
        let mut args = vec![self.parse_type_spec()?];
        while self.check(&TokenKind::Comma) {
            self.advance()?;
            args.push(self.parse_type_spec()?);
        }
        self.expect(TokenKind::Gt)?;
        Ok(args)
    }

    fn parse_closure(&mut self) -> Result<Expr,ParseError> {
        self.expect(TokenKind::Pipe)?;
        let mut params:Vec<Param>=Vec::new();
        while !self.check(&TokenKind::Pipe) {
            let name=self.expect_id()?;
            if params.iter().any(|p|p.name==name){return Err(ParseError::SyntaxError{line:self.current.line,col:self.current.col,message:format!("duplicate closure parameter {}",name)});}
            self.expect(TokenKind::Colon)?;
            let param_type=self.parse_type_spec()?;
            params.push(Param{name,param_type});
            if !self.check(&TokenKind::Comma){break;}self.advance()?;
        }
        self.expect(TokenKind::Pipe)?;self.expect(TokenKind::Arrow)?;
        let return_type=self.parse_type_spec()?;
        let body=self.parse_block()?;
        Ok(Expr::Closure{params,return_type,body})
    }

    fn parse_dict_literal(&mut self)->Result<Expr,ParseError>{
        self.expect(TokenKind::LBrace)?;let mut entries=Vec::new();
        while !self.check(&TokenKind::RBrace){
            let key=match self.advance()?.kind{TokenKind::Str(s)|TokenKind::Id(s)=>s,_=>return Err(ParseError::SyntaxError{line:self.current.line,col:self.current.col,message:"dictionary keys must be strings or identifiers".into()})};
            if entries.iter().any(|(name,_)|name==&key){return Err(ParseError::SyntaxError{line:self.current.line,col:self.current.col,message:format!("duplicate dictionary key {}",key)});}
            self.expect(TokenKind::Colon)?;entries.push((key,self.parse_expr()?));
            if !self.check(&TokenKind::Comma){break;}self.advance()?;
        }
        self.expect(TokenKind::RBrace)?;Ok(Expr::DictLiteral(entries))
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match &self.current.kind {
            TokenKind::LBrace => self.parse_dict_literal(),
            TokenKind::Pipe => self.parse_closure(),
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
            TokenKind::Receive => {
                self.advance()?;
                Ok(Expr::ReceiveSignal)
            }
            TokenKind::Memory => {
                self.advance()?;
                Ok(Expr::MemoryGet(self.expect_id()?))
            }
            TokenKind::LBracket => {
                self.advance()?;
                let mut items = Vec::new();
                while !self.check(&TokenKind::RBracket) && !self.check(&TokenKind::Eof) {
                    items.push(self.parse_expr()?);
                    if self.check(&TokenKind::Comma) {
                        self.advance()?;
                    } else {
                        break;
                    }
                }
                self.expect(TokenKind::RBracket)?;
                Ok(Expr::ListLiteral(items))
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
