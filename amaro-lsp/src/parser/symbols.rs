use std::collections::HashMap;

// Type System
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum Type {
    // Primitives
    Int,
    Float,
    Bool,
    String,
    
    // Quantum-specific
    Location,
    Qubit,
    QubitMap,
    Gate,
    
    // Block types
    ArchT,
    StateT,
    InstrT,
    
    // Compound types
    Vec(Box<Type>),
    Tuple(Vec<Type>),
    Option(Box<Type>),
    
    // Function types
    Function {
        params: Vec<Type>,
        return_type: Box<Type>,
    },
    
    // Struct types
    Struct {
        name: String,
        fields: HashMap<String, Type>,
    },
    
    Unknown,
}

// Symbol Table
pub struct SymbolTable {
    // bindings: HashMap<String, Type>,
    scopes: Vec<HashMap<String, Type>>,
}

impl SymbolTable {
    pub fn new() -> Self {
        let mut global_scope = HashMap::new();

        Self::register_context_vars(&mut global_scope);
        Self::register_constructors(&mut global_scope);
        Self::register_gate_literals(&mut global_scope);
        Self::register_builtin_functions(&mut global_scope);
        SymbolTable {
            scopes: vec![global_scope],
        }
    }

    pub fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn exit_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn bind(&mut self, name: String, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty);
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }

    fn register_context_vars(scope: &mut HashMap<String, Type>) {
        scope.insert("Arch".to_string(), Type::ArchT);
        scope.insert("arch".to_string(), Type::ArchT);
        scope.insert("State".to_string(), Type::StateT);
        scope.insert("Gate".to_string(), Type::Gate);
        scope.insert("step".to_string(), Type::Int);
        scope.insert("Transition".to_string(), Type::Struct { 
            name: "Transition".to_string(), 
            fields: HashMap::new() 
        });
        scope.insert("GateRealization".to_string(), Type::Struct {
            name: "GateRealization".to_string(),
            fields: HashMap::new(),
        });
    }

    fn register_constructors(scope: &mut HashMap<String, Type>) {
        scope.insert("Qubit".to_string(), Type::Function {
            params: vec![Type::Int],
            return_type: Box::new(Type::Qubit),
        });
        scope.insert("Location".to_string(), Type::Function {
            params: vec![Type::Int],
            return_type: Box::new(Type::Location),
        });
        scope.insert("Vec".to_string(), Type::Function {
            params: vec![], 
            return_type: Box::new(Type::Vec(Box::new(Type::Unknown))),
        });
    }

    fn register_gate_literals(scope: &mut HashMap<String, Type>) {
        for gate in ["CX", "T", "Pauli", "PauliMeasurement", "H", "CZ", "X", "Y", "Z", "S", "Sdg", "Tdg", "RX", "RY", "RZ"] {
            scope.insert(gate.to_string(), Type::Gate);
        }
    }

    fn register_builtin_functions(scope: &mut HashMap<String, Type>) {
        // Quantum map operations
        scope.insert("value_swap".to_string(), Type::Function {
            params: vec![Type::Location, Type::Location],
            return_type: Box::new(Type::QubitMap),
        });

        scope.insert("values".to_string(), Type::Function {
            params: vec![Type::QubitMap], 
            return_type: Box::new(Type::Vec(Box::new(Type::Location))),
        });
        
        scope.insert("identity_application".to_string(), Type::Function {
            params: vec![Type::Unknown],
            return_type: Box::new(Type::Unknown),
        });

        // Higher-order
        scope.insert("map".to_string(), Type::Function {
            params: vec![Type::Unknown, Type::Vec(Box::new(Type::Unknown))], 
            return_type: Box::new(Type::Vec(Box::new(Type::Unknown))),
        });

        scope.insert("fold".to_string(), Type::Function {
            params: vec![
                Type::Unknown,
                Type::Unknown,
                Type::Vec(Box::new(Type::Unknown)),
            ],
            return_type: Box::new(Type::Unknown),
        });

        // Neighbor functions
        scope.insert("vertical_neighbors".to_string(), Type::Function {
            params: vec![Type::Location, Type::Int, Type::Int], 
            return_type: Box::new(Type::Vec(Box::new(Type::Location))),
        });
        scope.insert("horizontal_neighbors".to_string(), Type::Function {
            params: vec![Type::Location, Type::Int], 
            return_type: Box::new(Type::Vec(Box::new(Type::Location))),
        });

        // Path functions
        scope.insert("path".to_string(), Type::Function {
            params: vec![],
            return_type: Box::new(Type::Vec(Box::new(Type::Location))),
        });
        scope.insert("tree".to_string(), Type::Function {
            params: vec![],
            return_type: Box::new(Type::Vec(Box::new(Type::Location))),
        });
        scope.insert("all_paths".to_string(), Type::Function {
            params: vec![
                Type::ArchT,
                Type::Vec(Box::new(Type::Location)),
                Type::Vec(Box::new(Type::Location)),
                Type::Vec(Box::new(Type::Location))
            ],
            return_type: Box::new(Type::Vec(Box::new(Type::Vec(Box::new(Type::Location))))),
        });
        scope.insert("shortest_path".to_string(), Type::Function {
            params: vec![
                Type::ArchT,
                Type::Vec(Box::new(Type::Location)),
                Type::Vec(Box::new(Type::Location)),
                Type::Vec(Box::new(Type::Location)),
            ],
            return_type: Box::new(Type::Option(Box::new(Type::Vec(Box::new(Type::Location))))),
        });
        scope.insert("steiner_trees".to_string(), Type::Function {
            params: vec![
                Type::ArchT,
                Type::Vec(Box::new(Type::Vec(Box::new(Type::Location)))),
                Type::Vec(Box::new(Type::Location)),
            ],
            return_type: Box::new(Type::Vec(Box::new(Type::Location))),
        });

    }

}
