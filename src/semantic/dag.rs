use std::collections::{HashMap, HashSet};
use crate::ast::*;
use crate::semantic::{flow::FunctionFlow, SemanticError};

/// Checks the synchronous call graph obtained from the shared fixed-point
/// analysis. Only direct structural self recursion is delegated to the
/// termination checker; indirect self edges must remain in the graph.
pub struct CallGraphValidator {
    adjacency_list: HashMap<String, HashSet<String>>,
}
impl CallGraphValidator {
    pub fn new() -> Self { Self { adjacency_list: HashMap::new() } }
    pub fn build_and_validate(&mut self, program: &Program) -> Result<(), SemanticError> {
        let flow = FunctionFlow::analyze(program)?;
        self.adjacency_list = flow.calls_by_owner().clone();
        for decl in &program.declarations {
            let name = match decl { TopLevelDecl::Func(f) => &f.name, TopLevelDecl::Routine(r) => &r.name, TopLevelDecl::Loop(l) => &l.name, _ => continue };
                let indirect_self = flow.indirect_calls_by_owner().get(name).is_some_and(|targets| targets.contains(name));
                if !indirect_self {
                    if let Some(callees) = self.adjacency_list.get_mut(name) { callees.remove(name); }
                }
        }
        let mut visited=HashSet::new();
        let mut stack=HashSet::new();
        let mut nodes: Vec<_> = self.adjacency_list.keys().collect();
        nodes.sort();
        for node in nodes {
            if self.cycle(node,&mut visited,&mut stack) { return Err(SemanticError::CallGraphCycle(crate::semantic::closures::display_name(node).into())); }
        }
        Ok(())
    }
    fn cycle(&self, node: &str, visited: &mut HashSet<String>, stack: &mut HashSet<String>) -> bool {
        if stack.contains(node) { return true; }
        if !visited.insert(node.to_owned()) { return false; }
        stack.insert(node.to_owned());
        if let Some(callees)=self.adjacency_list.get(node) {
            for callee in callees { if self.cycle(callee,visited,stack) { return true; } }
        }
        stack.remove(node);
        false
    }
}
