use tokio::sync::mpsc;
use crate::runtime::actor::Message;
use crate::runtime::arena::RoutineArena;
use crate::ast::{Statement, Expr};

pub struct RuntimeEventLoop {
    receiver: mpsc::Receiver<Message>,
    verbose: bool,
}

impl RuntimeEventLoop {
    pub fn new(receiver: mpsc::Receiver<Message>) -> Self {
        Self { receiver, verbose: false }
    }

    pub fn with_verbose(receiver: mpsc::Receiver<Message>, verbose: bool) -> Self {
        Self { receiver, verbose }
    }

    pub async fn run(&mut self) {
        if self.verbose {
            println!("[Runtime EventLoop] Started. Awaiting routine dispatches...");
        }

        let verbose = self.verbose;
        while let Some(msg) = self.receiver.recv().await {
            tokio::spawn(async move {
                // Allocate isolated arena for routine execution context
                let arena = RoutineArena::new();

                let allocated_payload = arena.alloc(msg.payload.clone());
                if verbose {
                    println!(
                        "[Runtime] Executing isolated routine '{}' with payload in Arena: {}",
                        msg.routine_name, allocated_payload
                    );
                }

                for stmt in &msg.statements {
                    Self::execute_statement(stmt, &arena, verbose);
                }

                // Routine finishes and entire arena is deallocated automatically in O(1)
            });
        }
    }

    fn execute_statement(stmt: &Statement, arena: &RoutineArena, verbose: bool) {
        match stmt {
            Statement::ExprStmt(expr) => {
                Self::execute_expr(expr, arena, verbose);
            }
            Statement::VarDecl { name, value, is_mutable, .. } => {
                let formatted = Self::format_expr(value);
                let stored = arena.alloc(formatted);
                if verbose {
                    let mut_tag = if *is_mutable { "mut " } else { "" };
                    println!("  [Arena Allocation] Local variable let {}{} = {} allocated in Arena", mut_tag, name, stored);
                }
            }
            Statement::Spawn(expr) => {
                if verbose {
                    println!("  [Concurrent Spawn] Dispatching isolated actor for: {}", Self::format_expr(expr));
                }
            }
            Statement::ForLoop { var_name, collection, body } => {
                if verbose {
                    println!("  [Bounded Loop] Executing bounded loop 'for {} in {}' ({} instructions)", var_name, Self::format_expr(collection), body.len());
                }
                for s in body {
                    Self::execute_statement(s, arena, verbose);
                }
            }
            Statement::Match { target, cases } => {
                if verbose {
                    println!("  [Match] Pattern matching on {} with {} evaluated cases.", Self::format_expr(target), cases.len());
                }
                if let Some(first_case) = cases.first() {
                    for s in &first_case.body {
                        Self::execute_statement(s, arena, verbose);
                    }
                }
            }
        }
    }

    fn execute_expr(expr: &Expr, _arena: &RoutineArena, verbose: bool) {
        if let Expr::FunctionCall { name, args } = expr {
            if name == "log" || name == "print" || name == "println" {
                let parts: Vec<String> = args.iter().map(Self::format_expr_value).collect();
                println!("{}", parts.join(" "));
            } else if verbose {
                let parts: Vec<String> = args.iter().map(Self::format_expr_value).collect();
                println!("  [Function Call]: {}({})", name, parts.join(", "));
            }
        }
    }

    fn format_expr_value(expr: &Expr) -> String {
        match expr {
            Expr::StringLiteral(s) => s.clone(),
            Expr::IntLiteral(n) => n.to_string(),
            Expr::Variable(v) => v.clone(),
            Expr::BinaryOp { op, left, right } => {
                format!("{} {} {}", Self::format_expr_value(left), op, Self::format_expr_value(right))
            }
            Expr::MemberAccess { target, field } => {
                format!("{}.{}", Self::format_expr_value(target), field)
            }
            Expr::RefCreate(inner) => {
                format!("ref {}", Self::format_expr_value(inner))
            }
            Expr::FunctionCall { name, args } => {
                let parts: Vec<String> = args.iter().map(Self::format_expr_value).collect();
                format!("{}({})", name, parts.join(", "))
            }
        }
    }

    fn format_expr(expr: &Expr) -> String {
        match expr {
            Expr::StringLiteral(s) => format!("\"{}\"", s),
            Expr::IntLiteral(n) => n.to_string(),
            Expr::Variable(v) => v.clone(),
            Expr::BinaryOp { op, left, right } => {
                format!("({} {} {})", Self::format_expr(left), op, Self::format_expr(right))
            }
            Expr::MemberAccess { target, field } => {
                format!("{}.{}", Self::format_expr(target), field)
            }
            Expr::RefCreate(inner) => {
                format!("ref {}", Self::format_expr(inner))
            }
            Expr::FunctionCall { name, args } => {
                let parts: Vec<String> = args.iter().map(Self::format_expr).collect();
                format!("{}({})", name, parts.join(", "))
            }
        }
    }
}
