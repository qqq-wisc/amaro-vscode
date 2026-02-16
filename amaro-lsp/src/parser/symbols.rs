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
        global_scope.insert("Arch".to_string(), Type::ArchT);
        global_scope.insert("arch".to_string(), Type::ArchT);
        global_scope.insert("State".to_string(), Type::StateT);
        global_scope.insert("Gate".to_string(), Type::Gate);
        global_scope.insert("Transition".to_string(), Type::Struct { 
            name: "Transition".to_string(), 
            fields: HashMap::new() 
        });
        global_scope.insert("Qubit".to_string(), Type::Function {
            params: vec![Type::Int],
            return_type: Box::new(Type::Qubit),
        });
        global_scope.insert("GateRealization".to_string(), Type::Struct {
            name: "GateRealization".to_string(),
            fields: HashMap::new(),
        });

        for gate in ["CX", "T", "Pauli", "PauliMeasurement", "H", "CZ", "X", "Y", "Z", "S", "Sdg", "Tdg", "RX", "RY", "RZ"] {
            global_scope.insert(gate.to_string(), Type::Gate);
        }

        // Built-in functions
        global_scope.insert("value_swap".to_string(), Type::Function {
            params: vec![Type::Location, Type::Location],
            return_type: Box::new(Type::QubitMap),
        });

        global_scope.insert("map".to_string(), Type::Function {
            params: vec![Type::Unknown, Type::Vec(Box::new(Type::Unknown))], 
            return_type: Box::new(Type::Vec(Box::new(Type::Unknown))),
        });

        global_scope.insert("Location".to_string(), Type::Function {
            params: vec![Type::Int],
            return_type: Box::new(Type::Location),
        });

        global_scope.insert("fold".to_string(), Type::Function {
            params: vec![
                Type::Unknown,
                Type::Unknown,
                Type::Vec(Box::new(Type::Unknown)),
            ],
            return_type: Box::new(Type::Unknown),
        });

        global_scope.insert("Vec".to_string(), Type::Function {
            params: vec![], 
            return_type: Box::new(Type::Vec(Box::new(Type::Unknown))),
        });

        global_scope.insert("values".to_string(), Type::Function {
            params: vec![Type::QubitMap], 
            return_type: Box::new(Type::Vec(Box::new(Type::Location))),
        });

        global_scope.insert("vertical_neighbors".to_string(), Type::Function {
            params: vec![Type::Location, Type::Int, Type::Int], 
            return_type: Box::new(Type::Vec(Box::new(Type::Location))),
        });
        global_scope.insert("horizontal_neighbors".to_string(), Type::Function {
            params: vec![Type::Location, Type::Int], 
            return_type: Box::new(Type::Vec(Box::new(Type::Location))),
        });

        global_scope.insert("step".to_string(), Type::Int);

        global_scope.insert("all_paths".to_string(), Type::Function {
            params: vec![
                Type::ArchT,
                Type::Vec(Box::new(Type::Location)),
                Type::Vec(Box::new(Type::Location)),
                Type::Vec(Box::new(Type::Location))
            ],
            return_type: Box::new(Type::Vec(Box::new(Type::Vec(Box::new(Type::Location))))),
        });

        global_scope.insert("identity_application".to_string(), Type::Function {
            params: vec![Type::Unknown],
            return_type: Box::new(Type::Unknown),
        });

        global_scope.insert("path".to_string(), Type::Function {
            params: vec![],
            return_type: Box::new(Type::Vec(Box::new(Type::Location))),
        });

        global_scope.insert("steiner_trees".to_string(), Type::Function {
            params: vec![
                Type::ArchT,
                Type::Vec(Box::new(Type::Vec(Box::new(Type::Location)))),
                Type::Vec(Box::new(Type::Location)),
            ],
            return_type: Box::new(Type::Vec(Box::new(Type::Location))),
        });

        global_scope.insert("shortest_path".to_string(), Type::Function {
            params: vec![
                Type::ArchT,
                Type::Vec(Box::new(Type::Location)),
                Type::Vec(Box::new(Type::Location)),
                Type::Vec(Box::new(Type::Location)),
            ],
            return_type: Box::new(Type::Option(Box::new(Type::Vec(Box::new(Type::Location))))),
        });

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
}
