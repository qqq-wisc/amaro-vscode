use std::collections::HashMap;

/// The type system for Amaro expressions.
///
/// Represents all possible types that can appear in the language, including
/// primitives, quantum-specific types, compound types, and function signatures.
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

/// A scoped symbol table for tracking variable bindings and their types.
///
/// Uses a stack of scopes to support nested let-bindings and lambda parameters.
/// The global scope contains all built-in functions and type constructors.
pub struct SymbolTable {
    // bindings: HashMap<String, Type>,
    scopes: Vec<HashMap<String, Type>>,
}

impl SymbolTable {
    /// Creates a new symbol table with all built-in types and functions registered.
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

    /// Enters a new scope for let-bindings or lambda parameters.
    pub fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Exits the current scope, restoring the previous binding context.
    pub fn exit_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Binds a variable name to a type in the current scope.
    pub fn bind(&mut self, name: String, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty);
        }
    }

    /// Looks up a variable name in the scope stack, starting from innermost scope.
    pub fn lookup(&self, name: &str) -> Option<&Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }

    /// Registers context variables (Arch, State, Gate, Transition, etc.).
    fn register_context_vars(scope: &mut HashMap<String, Type>) {
        scope.insert("Arch".to_string(), Type::ArchT);
        scope.insert("arch".to_string(), Type::ArchT);
        scope.insert("State".to_string(), Type::StateT);
        scope.insert("Gate".to_string(), Type::Gate);
        scope.insert("step".to_string(), Type::Int);
        scope.insert(
            "Transition".to_string(),
            Type::Struct {
                name: "Transition".to_string(),
                fields: HashMap::new(),
            },
        );
        scope.insert(
            "GateRealization".to_string(),
            Type::Struct {
                name: "GateRealization".to_string(),
                fields: HashMap::new(),
            },
        );
    }

    /// Registers type constructors (Location, Qubit, Vec).
    fn register_constructors(scope: &mut HashMap<String, Type>) {
        scope.insert(
            "Qubit".to_string(),
            Type::Function {
                params: vec![Type::Int],
                return_type: Box::new(Type::Qubit),
            },
        );
        scope.insert(
            "Location".to_string(),
            Type::Function {
                params: vec![Type::Int],
                return_type: Box::new(Type::Location),
            },
        );
        scope.insert(
            "Vec".to_string(),
            Type::Function {
                params: vec![],
                return_type: Box::new(Type::Vec(Box::new(Type::Unknown))),
            },
        );
    }

    /// Registers gate literals (CX, T, Pauli, etc.) as Gate type.
    fn register_gate_literals(scope: &mut HashMap<String, Type>) {
        for gate in [
            "CX",
            "T",
            "Pauli",
            "PauliMeasurement",
            "H",
            "CZ",
            "X",
            "Y",
            "Z",
            "S",
            "Sdg",
            "Tdg",
            "RX",
            "RY",
            "RZ",
        ] {
            scope.insert(gate.to_string(), Type::Gate);
        }
    }

    /// Registers built-in helper functions (map, fold, all_paths, steiner_trees, etc.).
    fn register_builtin_functions(scope: &mut HashMap<String, Type>) {
        // Quantum map operations
        scope.insert(
            "value_swap".to_string(),
            Type::Function {
                params: vec![Type::Location, Type::Location],
                return_type: Box::new(Type::QubitMap),
            },
        );

        scope.insert(
            "values".to_string(),
            Type::Function {
                params: vec![Type::QubitMap],
                return_type: Box::new(Type::Vec(Box::new(Type::Location))),
            },
        );

        scope.insert(
            "identity_application".to_string(),
            Type::Function {
                params: vec![Type::Unknown],
                return_type: Box::new(Type::Unknown),
            },
        );

        // Higher-order
        scope.insert(
            "map".to_string(),
            Type::Function {
                params: vec![Type::Unknown, Type::Vec(Box::new(Type::Unknown))],
                return_type: Box::new(Type::Vec(Box::new(Type::Unknown))),
            },
        );

        scope.insert(
            "fold".to_string(),
            Type::Function {
                params: vec![
                    Type::Unknown,
                    Type::Unknown,
                    Type::Vec(Box::new(Type::Unknown)),
                ],
                return_type: Box::new(Type::Unknown),
            },
        );

        // Neighbor functions
        scope.insert(
            "vertical_neighbors".to_string(),
            Type::Function {
                params: vec![Type::Location, Type::Int, Type::Int],
                return_type: Box::new(Type::Vec(Box::new(Type::Location))),
            },
        );
        scope.insert(
            "horizontal_neighbors".to_string(),
            Type::Function {
                params: vec![Type::Location, Type::Int],
                return_type: Box::new(Type::Vec(Box::new(Type::Location))),
            },
        );

        // Path functions
        scope.insert(
            "path".to_string(),
            Type::Function {
                params: vec![],
                return_type: Box::new(Type::Vec(Box::new(Type::Location))),
            },
        );
        scope.insert(
            "tree".to_string(),
            Type::Function {
                params: vec![],
                return_type: Box::new(Type::Vec(Box::new(Type::Location))),
            },
        );
        scope.insert(
            "all_paths".to_string(),
            Type::Function {
                params: vec![
                    Type::ArchT,
                    Type::Vec(Box::new(Type::Location)),
                    Type::Vec(Box::new(Type::Location)),
                    Type::Vec(Box::new(Type::Location)),
                ],
                return_type: Box::new(Type::Vec(Box::new(Type::Vec(Box::new(Type::Location))))),
            },
        );
        scope.insert(
            "shortest_path".to_string(),
            Type::Function {
                params: vec![
                    Type::ArchT,
                    Type::Vec(Box::new(Type::Location)),
                    Type::Vec(Box::new(Type::Location)),
                    Type::Vec(Box::new(Type::Location)),
                ],
                return_type: Box::new(Type::Option(Box::new(Type::Vec(Box::new(Type::Location))))),
            },
        );
        scope.insert(
            "steiner_trees".to_string(),
            Type::Function {
                params: vec![
                    Type::ArchT,
                    Type::Vec(Box::new(Type::Vec(Box::new(Type::Location)))),
                    Type::Vec(Box::new(Type::Location)),
                ],
                return_type: Box::new(Type::Vec(Box::new(Type::Location))),
            },
        );
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}
