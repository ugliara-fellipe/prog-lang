use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TypeSpec {
    Primitive(String),
    Generic(String, Vec<TypeSpec>),
    Ref(Box<TypeSpec>),
}

impl TypeSpec {
    pub fn is_copyable(&self) -> bool {
        match self {
            TypeSpec::Ref(_) => false,
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
    IntLiteral(i64),
    StringLiteral(String),
    Variable(String),
    /// The signal delivered to the current loop iteration. Access its fields
    /// as `receive.value` and `receive.pending`.
    ReceiveSignal,
    RefCreate(Box<Expr>),
    FunctionCall { name: String, args: Vec<Expr> },
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
    pub params: Vec<Param>,
    pub return_type: Option<TypeSpec>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineDecl {
    pub name: String,
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
pub struct TypeDecl {
    pub name: String,
    pub fields: Vec<StructField>,
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
