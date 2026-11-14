use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TypeSpec {
    Primitive(String),
    Generic(String, Vec<TypeSpec>),
    Ref(Box<TypeSpec>),
    Function(Vec<TypeSpec>, Box<TypeSpec>),
}

impl TypeSpec {
    pub fn is_copyable(&self) -> bool {
        match self {
            TypeSpec::Ref(_) => false,
            TypeSpec::Function(_, _) => true,
            TypeSpec::Primitive(_) => true,
            TypeSpec::Generic(name, inner) => {
                name != "Ref" && inner.iter().all(|t| t.is_copyable())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructField {
    /// Markdown from the preceding `///` comment block.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub documentation: String,
    pub name: String,
    pub field_type: TypeSpec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub param_type: TypeSpec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Pattern {
    Wildcard,
    Identifier(String),
    Constructor(String, Vec<Pattern>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    /// Forward composition, lowered to typed value-capturing closures.
    Compose { left: Box<Expr>, right: Box<Expr> },
    /// Typed, already evaluated compile-time data.
    ConstValue { value: Box<Expr>, value_type: TypeSpec },
    Closure { params: Vec<Param>, return_type: TypeSpec, body: Vec<Statement> },
    IntLiteral(i64),
    StringLiteral(String),
    ListLiteral(Vec<Expr>),
    DictLiteral(Vec<(String,Expr)>),
    Index { target: Box<Expr>, key: Box<Expr> },
    Variable(String),
    /// Returns the type name of a variable as a string.
    TypeOf(String),
    /// The signal delivered to the current loop iteration. Access its fields
    /// as `message.value` and `message.pending`.
    ReceiveSignal,
    /// Reads a value stored in the current persistent loop's private memory.
    MemoryGet(String),
    RefCreate(Box<Expr>),
    /// Asserts that a call is in a function return position.
    TailCall(Box<Expr>),
    FunctionCall { name: String, args: Vec<Expr> },
    /// Calls the function value produced by an expression.
    IndirectCall { target: Box<Expr>, args: Vec<Expr> },
    /// Explicit instantiation of a generic named function, e.g. `id<Int>`.
    Specialize { target: Box<Expr>, type_args: Vec<TypeSpec> },
    MemberAccess { target: Box<Expr>, field: String },
    BinaryOp { op: String, left: Box<Expr>, right: Box<Expr> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchCase {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guard: Option<Expr>,
    pub pattern: Pattern,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    Const(ConstDecl),
    VarDecl {
        name: String,
        is_mutable: bool,
        type_annot: Option<TypeSpec>,
        value: Expr,
    },
    /// Stores a value in the current persistent loop's private memory.
    MemorySet { name: String, value: Expr },
    Spawn(Expr),
    /// Asynchronously sends a value to the named persistent loop.
    Signal { loop_name: String, payload: Expr },
    /// Raises an exception value. Only pure functions may emit exceptions.
    Throw(Expr),
    /// Handles an exception raised by the protected block.
    TryCatch {
        try_body: Vec<Statement>,
        error_name: String,
        catch_body: Vec<Statement>,
    },
    /// Ends the current loop iteration permanently. `terminate` is meaningful only
    /// while a loop task is executing.
    Terminate,
    ForLoop {
        var_name: String,
        collection: Expr,
        body: Vec<Statement>,
    },
    Match {
        target: Expr,
        cases: Vec<MatchCase>,
    },
    ExprStmt(Expr),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FuncDecl {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreign: Option<ForeignDecl>,
    /// Markdown from the preceding `///` comment block.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub documentation: String,
    pub name: String,
    #[serde(default)]
    pub type_params: Vec<String>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeSpec>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineDecl {
    /// Markdown from the preceding `///` comment block.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub documentation: String,
    pub name: String,
    #[serde(default)]
    pub type_params: Vec<String>,
    pub params: Vec<Param>,
    pub body: Vec<Statement>,
}

/// A long-lived actor. Each completed iteration is put back on the same work
/// queue used by routines; it never owns a worker thread or CPU core.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoopDecl {
    /// Markdown from the preceding `///` comment block.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub documentation: String,
    pub name: String,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumVariant {
    /// Markdown from the preceding `///` comment block.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub documentation: String,
    pub name: String,
    pub fields: Vec<TypeSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeDecl {
    /// Markdown from the preceding `///` comment block.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub documentation: String,
    pub name: String,
    #[serde(default)]
    pub type_params: Vec<String>,
    pub fields: Vec<StructField>,
    #[serde(default)]
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ImportKind {
    All,
    Single(String),
    Items(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportDecl {
    pub package: String,
    pub kind: ImportKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TopLevelDecl {
    Const(ConstDecl),
    Import(ImportDecl),
    Type(TypeDecl),
    Func(FuncDecl),
    Routine(RoutineDecl),
    Loop(LoopDecl),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub imports: Vec<ImportDecl>,
    pub declarations: Vec<TopLevelDecl>,
}

/// A declaration whose initializer must be evaluated before execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstDecl {
    /// Markdown from the preceding `///` comment block.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub documentation: String,
    pub name: String,
    pub type_annot: Option<TypeSpec>,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForeignDecl {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    pub abi: String,
    pub symbol: String,
    pub library: Option<String>,
}
