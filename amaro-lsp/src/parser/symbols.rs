use std::{collections::HashMap, fmt::Write};

use crate::ast::TypeAnnotation;

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
    UserDef(String),

    // Generic
    Generic(u8), // local id for generic is the u8

    Unknown,
}

impl Type {
    pub fn from_type_annotation(type_annotation: &TypeAnnotation) -> Self {
        match type_annotation {
            TypeAnnotation::Simple(name) => match name.as_str() {
                "Int" => Type::Int,
                "Float" => Type::Float,
                "Bool" => Type::Bool,
                "String" => Type::String,
                "Location" => Type::Location,
                "Arch" => Type::ArchT,
                "Gate" => Type::Gate,
                "Instr" => Type::InstrT, // TODO determine if this is accurate
                "Qubit" => Type::Qubit,
                "QubitMap" => Type::QubitMap,
                "State" => Type::StateT,
                // if none of the built-in types, then assume it is a type defined
                // by the user. could also be garbage
                el => Type::UserDef(el.to_string()),
            },
            TypeAnnotation::Generic(name, type_annotations) => match name.as_str() {
                "Vec" => {
                    if type_annotations.len() != 1 {
                        Type::Unknown
                    } else {
                        Type::Vec(Box::new(Self::from_type_annotation(&type_annotations[0])))
                    }
                }
                "Option" => {
                    if type_annotations.len() != 1 {
                        Type::Unknown
                    } else {
                        Type::Option(Box::new(Self::from_type_annotation(&type_annotations[0])))
                    }
                }
                _ => Type::Unknown,
            },
            TypeAnnotation::Tuple(type_annotations) => Type::Tuple(
                type_annotations
                    .iter()
                    .map(Self::from_type_annotation)
                    .collect(),
            ),
            TypeAnnotation::Function {
                params,
                return_type,
            } => Type::Function {
                params: params.iter().map(Self::from_type_annotation).collect(),
                return_type: Box::new(Self::from_type_annotation(return_type)),
            },
        }
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Int => f.write_str("Int"),
            Type::Float => f.write_str("Float"),
            Type::Bool => f.write_str("Bool"),
            Type::String => f.write_str("String"),
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
            }
            Type::Option(inner) => f
                .write_str("Option<")
                .and(inner.fmt(f))
                .and(f.write_char('>')),
            Type::Function {
                params,
                return_type,
            } => {
                f.write_char('|')?;
                let mut iter = params.iter();
                if let Some(first) = iter.next() {
                    write!(f, "{}", first)?;
                    for item in iter {
                        f.write_str(", ")?;
                        write!(f, "{}", item)?;
                    }
                }
                f.write_str("| -> ")?;
                return_type.fmt(f)
            }
            Type::UserDef(name) => f.write_str(name),
            Type::Generic(c) => write!(f, "T{}", c),
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

impl SymbolTable {
    /// Creates a new symbol table with all built-in types and functions registered.
    pub fn new() -> Self {
        SymbolTable {
            scopes: vec![HashMap::new()],
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
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

/// There are user-defined types, like Transition.
/// We need to have ONE place where we store these types.
/// Then, we can reference these types from here by name.
#[derive(Debug)]
pub struct UserDefTable {
    /// maps from type names (like Transition) to their fields
    map: HashMap<String, UserDefEntry>,
}

#[derive(Debug)]
struct UserDefEntry {
    fields: HashMap<String, Type>,
}

impl UserDefTable {
    /// Given an AmaroFile, creates a UserDefTable which determines the fields
    /// of user-defined types.
    pub fn new(file: &crate::ast::AmaroFile) -> Self {
        // TODO work on adding diagnostics in the case of errors
        let mut map: HashMap<String, UserDefEntry> = HashMap::new();

        for block in &file.blocks {
            let crate::ast::BlockContent::Fields(items) = &block.content;
            items
                .iter()
                .filter_map(|elt| match elt {
                    crate::ast::BlockItem::Field(_) => None,
                    crate::ast::BlockItem::StructDef(struct_def) => Some(struct_def),
                    crate::ast::BlockItem::ReturnKeyword { .. } => None,
                })
                .for_each(|struct_def| {
                    let fields = struct_def
                        .fields
                        .iter()
                        .map(|elt| {
                            (
                                elt.name.clone(),
                                Type::from_type_annotation(&elt.type_annotation),
                            )
                        })
                        .collect();
                    map.insert(struct_def.name.clone(), UserDefEntry { fields });
                })
        }

        UserDefTable { map }
    }

    /// Creates an empty UserDefTable. Useful if it is known that there are no
    /// user-defined types.
    #[cfg(test)]
    pub fn empty() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    /// Gets the fields of a user-defined type, if the type has a definition.
    pub fn get_fields(&self, identifier: &str) -> Option<&HashMap<String, Type>> {
        self.map.get(identifier).map(|elt| &elt.fields)
    }
}

/// Blocks have fields.
/// Fields each have an expected type signature.
/// For instance, 'cost' maps from Transition to Float.
/// This lets us lookup the expected type signature of a field.
pub fn field_lookup(field: &str) -> Option<Type> {
    match field {
        "cost" => Some(Type::Function {
            params: vec![Type::UserDef("Transition".to_string())],
            return_type: Box::new(Type::Float),
        }),
        "realize_gate" => Some(Type::Function {
            params: vec![Type::ArchT, Type::StateT, Type::Gate],
            return_type: Box::new(Type::Option(Box::new(Type::UserDef(
                "GateRealization".to_string(),
            )))),
        }),
        "get_transitions" => Some(Type::Function {
            params: vec![Type::ArchT, Type::StateT],
            return_type: Box::new(Type::Vec(Box::new(Type::UserDef("Transition".to_string())))),
        }),
        "apply" => Some(Type::Function {
            params: vec![Type::QubitMap, Type::UserDef("Transition".to_string())],
            return_type: Box::new(Type::QubitMap),
        }),
        "routed_gates" => Some(Type::Vec(Box::new(Type::Gate))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_lookup() {
        assert_eq!(field_lookup("total garbage"), None);
        assert_eq!(
            field_lookup("cost"),
            Some(Type::Function {
                params: vec![Type::UserDef("Transition".to_string())],
                return_type: Box::new(Type::Float),
            })
        );
        assert_eq!(
            field_lookup("realize_gate"),
            Some(Type::Function {
                params: vec![Type::ArchT, Type::StateT, Type::Gate],
                return_type: Box::new(Type::Option(Box::new(Type::UserDef(
                    "GateRealization".to_string(),
                )))),
            })
        );
        assert_eq!(
            field_lookup("get_transitions"),
            Some(Type::Function {
                params: vec![Type::ArchT, Type::StateT],
                return_type: Box::new(Type::Vec(Box::new(Type::UserDef("Transition".to_string())))),
            })
        );
        assert_eq!(
            field_lookup("apply"),
            Some(Type::Function {
                params: vec![Type::QubitMap, Type::UserDef("Transition".to_string())],
                return_type: Box::new(Type::QubitMap),
            })
        );
        assert_eq!(
            field_lookup("routed_gates"),
            Some(Type::Vec(Box::new(Type::Gate)))
        )
    }
}
