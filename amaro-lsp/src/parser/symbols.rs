use std::{collections::HashMap, fmt::Write};

use crate::parser::utils;

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

    UserDef(String),

    // Struct types
    Struct {
        name: String,
        fields: HashMap<String, Type>,
    },

    Unknown,
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Int => f.write_str("int"),
            Type::Float => f.write_str("float"),
            Type::Bool => f.write_str("bool"),
            Type::String => f.write_str("string"),
            Type::Location => f.write_str("Location"),
            Type::Qubit => f.write_str("Qubit"),
            Type::QubitMap => f.write_str("QubitMap"),
            Type::Gate => f.write_str("Gate"),
            Type::ArchT => f.write_str("Arch"),
            Type::StateT => f.write_str("State"),
            Type::InstrT => f.write_str("Instr"),
            Type::Vec(inner) => f.write_str("Vec<").and(inner.fmt(f)).and(f.write_char('>')),
            Type::Tuple(items) => {
                f.write_char('(')?;
                let mut iter = items.iter();
                if let Some(first) = iter.next() {

                    write!(f, "{}", first)?;
                    for item in iter {
                        f.write_str(", ")?;
                        write!(f, "{}", item)?;
                    }
                }
                f.write_char(')')
            }, // make this pleasant..
            Type::Option(inner) => f.write_str("Option<").and(inner.fmt(f)).and(f.write_char('>')),
            Type::Function { params, return_type } => {
                f.write_char('(')?;
                let mut iter = params.iter();
                if let Some(first) = iter.next() {

                    write!(f, "{}", first)?;
                    for item in iter {
                        f.write_str(", ")?;
                        write!(f, "{}", item)?;
                    }
                }
                f.write_str(") -> ")?;
                return_type.fmt(f)
            },
            Type::UserDef(name) => f.write_str(name),
            Type::Struct { name, fields } => f.write_str(name),
            Type::Unknown => f.write_char('?'),
        }
    }
}

/// A scoped symbol table for tracking variable bindings and their types.
///
/// Uses a stack of scopes to support nested let-bindings and lambda parameters.
/// The global scope contains all built-in functions and type constructors.
pub struct SymbolTable {
    // bindings: HashMap<String, Type>,
    scopes: Vec<HashMap<String, Type>>,
}

/// There are user-defined types, like Transition.
/// We need to have ONE place where we store these types.
/// Then, we can reference these types from here by name.
pub struct UserDefTable {
    /// maps from type names (like Transition) to their fields
    map: HashMap<String, UserDefEntry>
}

struct UserDefEntry {
    fields: HashMap<String, Type>
}

impl UserDefTable {
    /// Given an AmaroFile, creates a UserDefTable which determines the fields 
    /// of user-defined types
    pub fn new(file: &crate::ast::AmaroFile) -> Self {
        // TODO what if there are errors? need diagnostics?
        let mut map: HashMap<String, UserDefEntry> = HashMap::new();

        for block in &file.blocks {
            let items = match &block.content {
                crate::ast::BlockContent::Fields(block_items) => block_items,
            };
            items.iter().filter_map(|elt| match elt {
                crate::ast::BlockItem::Field(_) => None,
                crate::ast::BlockItem::StructDef(struct_def) => Some(struct_def),
            }).for_each(|struct_def| {
                let fields = struct_def.fields.iter().map(|elt| (elt.name.clone(), utils::type_annotation_to_type(&elt.type_annotation))).collect();
                map.insert(struct_def.name.clone(), UserDefEntry { fields: fields });
            })
        }

        UserDefTable { map }
    }

    pub fn get_fields(&self, identifier: &str) -> Option<&HashMap<String, Type>> {
        self.map.get(identifier).map(|elt| &elt.fields)
    }
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
            Type::UserDef("Transition".to_string())
            // Type::Struct {
            //     name: "Transition".to_string(),
            //     fields: HashMap::new(),
            // },
        );
        scope.insert(
            "GateRealization".to_string(),
            Type::UserDef("GateRealization".to_string())
            // Type::Struct {
            //     name: "GateRealization".to_string(),
            //     fields: HashMap::new(),
            // },
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

/// For built-in keywords. Identifies the expected type.
/// For instance, the cost function has an expected type.
pub struct KeywordTable {
    /// maps from function names to their signatures
    functions: HashMap<String, KeywordInfo>
}

pub struct KeywordInfo {
    typ: Type,
    description: String
}



impl KeywordTable {
    /// Makes a KeywordTable with all the built-in signatures
    pub fn new() -> Self {

        let mut functions: HashMap<String, KeywordInfo> = HashMap::new();

        // do the functions
        // hmm.. i dont actually know the types of many of them

        // TODO is this the correct signature for cost?
        functions.insert(
            "cost".to_string(), 

            KeywordInfo { typ: Type::Function { params: vec![Type::UserDef("Transition".to_string())], return_type: Box::new(Type::Float) }
            , description: "Explanation for cost function.\nFunction which determines cost of transitions.".to_string() }
            
            );
        
        KeywordTable { functions: functions }
    }
}

impl Default for KeywordTable {
    fn default() -> Self {
        Self::new()
    }
}


