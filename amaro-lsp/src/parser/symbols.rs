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
    bindings: HashMap<String, Type>,
    scopes: Vec<HashMap<String, Type>>,
}

impl SymbolTable {
    pub fn new() -> Self {
        let mut table = SymbolTable {
            bindings: HashMap::new(),
            scopes: Vec::new(),
        };

        table.add_built_in_types("Arch", Type::ArchT);
        table.add_built_in_types("State", Type::StateT);
        table.add_built_in_types("Instr", Type::InstrT);
        table.add_built_in_types("Gate", Type::Gate);

        table.add_built_in_types("map", Type::Function {
            params: vec![
                Type::Function {
                    params: vec![Type::Unknown],
                    return_type: Box::new(Type::Unknown),
                },
                Type::Vec(Box::new(Type::Unknown)),
            ],
            return_type: Box::new(Type::Vec(Box::new(Type::Unknown))),
        });

        table.add_built_in_types("fold", Type::Function {
            params: vec![
                Type::Unknown,
                Type::Function {
                    params: vec![Type::Unknown, Type::Unknown],
                    return_type: Box::new(Type::Unknown),
                },
                Type::Vec(Box::new(Type::Unknown)),
            ],
            return_type: Box::new(Type::Unknown),
        });

        for func in &["push", "pop", "extend", "values", "all_paths", 
                      "steiner_trees", "horizontal_neighbors", "vertical_neighbors",
                      "identity_application", "value_swap", "contains_edge"] {
            table.add_built_in_types(func, Type::Function {
                params: vec![Type::Unknown],
                return_type: Box::new(Type::Unknown),
            });
        }
        table
    }

    fn add_built_in_types(&mut self, name: &str, ty: Type) {
        self.bindings.insert(name.to_string(), ty);
    }

    pub fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn exit_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn bind(&mut self, name: String, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty);
        } else {
            self.bindings.insert(name, ty);
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        self.bindings.get(name)
    }
}
