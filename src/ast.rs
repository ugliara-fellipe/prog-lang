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
    Closure { params: Vec<Param>, return_type: TypeSpec, body: Vec<Statement> },
    IntLiteral(i64),
    StringLiteral(String),
    ListLiteral(Vec<Expr>),
    Variable(String),
    /// Returns the type name of a variable as a string.
    TypeOf(String),
    /// The signal delivered to the current loop iteration. Access its fields
    /// as `message.value` and `message.pending`.
    ReceiveSignal,
    /// Reads a value stored in the current persistent loop's private memory.
    MemoryGet(String),
    RefCreate(Box<Expr>),
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
    pub pattern: Pattern,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
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
    pub name: String,
    #[serde(default)]
    pub type_params: Vec<String>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeSpec>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineDecl {
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
    pub name: String,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<TypeSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeDecl {
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
