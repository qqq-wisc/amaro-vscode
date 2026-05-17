use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
};

use crate::{ast::TypeAnnotation, parser::FetchAndAdd};

/// The type system for Amaro expressions.
///
/// Each expression has a Type.
///
/// Represents all possible types that can appear in the language, including
/// primitives, quantum-specific types, compound types, and function signatures.
///
/// Some types are special and perhaps obtuse. They are:
/// - UserDef
/// - Generic
/// - Unknown
///
/// See more about these below.
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
    /// Users can define structs, such as GateRealization and Transition.
    /// These are called UserDef, where the String is the name of the struct.
    /// Whenever a user defines a struct, information about its fields are
    /// stored in a UserDefTable, which can be referenced to determine the
    /// type of indexing off of a UserDef.
    UserDef(String),

    // Generic
    /// Oftentimes a type must use generic types. Use this in place of things
    /// like <T> or <H> or whatever. The passed u8 corresponds to the "name" of
    /// the variable used. So, all types that should be type T need to have the
    /// same u8 locally, and all the types that should be type H need to have
    /// the same u8 locally, but distinct from T. Just like normal generics.
    ///
    /// Throughout the semantic checking process, generics are resolved locally
    /// and conflicts are taken care of. For instance, recognize that the Vec()
    /// function has the type Vec(Generic(0)). So, if I do Vec().push(Vec()),
    /// then in doing so, each Vec is assigned a different generic value by the
    /// system, before the generics are resolved. This means conflicts won't
    /// occur with generics of equal values, because generics are resolved
    /// locally and not globally.
    ///
    /// Warning that there will be issues if there are more than 256 different
    /// generic types in a single expression. Can simply increase from u8 to u16
    /// TODO increase from u8 to u16
    Generic(u8), // local id for generic is the u8

    /// Unknown is used when there are details about the language we are
    /// uncertain of, OR if a user provides some bad input and we don't wish to
    /// propagate errors. By setting a type as Unknown, we ensure that the
    /// program will be "kind" to the type going forward and not report errors
    /// about it.
    Unknown,
}

impl Type {
    /// Turns a TypeAnnotation from the parser into a Type that's useable by
    /// the semantic checker
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
                        Type::Generic(0) // TODO should this be generic or unknown?
                    } else {
                        Type::Vec(Box::new(Self::from_type_annotation(&type_annotations[0])))
                    }
                }
                "Option" => {
                    if type_annotations.len() != 1 {
                        Type::Generic(0) // TODO should this be generic or unknown?
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

    /// Use this for rendering type to markdown display
    pub fn to_markdown_display(&self) -> String {
        format!("```rust\n{}\n```", self._to_markdown_display_visitor())
    }
    fn _to_markdown_display_visitor(&self) -> String {
        match self {
            Type::Int
            | Type::Float
            | Type::Bool
            | Type::String
            | Type::Location
            | Type::Qubit
            | Type::QubitMap
            | Type::Gate
            | Type::ArchT
            | Type::StateT
            | Type::InstrT
            | Type::Unknown
            | Type::UserDef(_)
            | Type::Generic(_) => format!("{}", self), // fall back to default, no nesting
            Type::Function {
                params,
                return_type,
            } => {
                let mut iter = params.iter();
                let mut string = String::new();
                if let Some(first) = iter.next() {
                    string += first._to_markdown_display_visitor().as_str();
                    for item in iter {
                        string += ", ";
                        string += item._to_markdown_display_visitor().as_str();
                    }
                }
                format!(
                    "|{}| -> {}",
                    string,
                    return_type._to_markdown_display_visitor().as_str()
                )
            }
            Type::Tuple(items) => {
                let mut iter = items.iter();
                let mut string = String::new();
                if let Some(first) = iter.next() {
                    string += first._to_markdown_display_visitor().as_str();
                    for item in iter {
                        string += ", ";
                        string += item._to_markdown_display_visitor().as_str();
                    }
                }
                format!("({})", string)
            }
            Type::Vec(inner) => format!("Vec<{}>", inner._to_markdown_display_visitor()),
            Type::Option(inner) => format!("Option<{}>", inner._to_markdown_display_visitor()),
        }
    }

    /// Determines whether the type has generics somewhere within it
    pub fn contains_generic(&self) -> bool {
        match self {
            Type::Generic(_) => true,
            Type::Int
            | Type::Float
            | Type::Bool
            | Type::String
            | Type::Location
            | Type::Qubit
            | Type::QubitMap
            | Type::Gate
            | Type::ArchT
            | Type::StateT
            | Type::InstrT
            | Type::UserDef(_)
            | Type::Unknown => false,
            Type::Vec(inner) => inner.contains_generic(),
            Type::Tuple(items) => items.iter().any(|f| f.contains_generic()),
            Type::Option(inner) => inner.contains_generic(),
            Type::Function {
                params,
                return_type,
            } => return_type.contains_generic() || params.iter().any(|f| f.contains_generic()),
        }
    }

    /// Determines whether the type has generics or unknowns somewhere within it
    pub fn contains_generic_or_unknown(&self) -> bool {
        match self {
            Type::Unknown => true,
            Type::Generic(_) => true,
            Type::Int
            | Type::Float
            | Type::Bool
            | Type::String
            | Type::Location
            | Type::Qubit
            | Type::QubitMap
            | Type::Gate
            | Type::ArchT
            | Type::StateT
            | Type::InstrT
            | Type::UserDef(_) => false,
            Type::Vec(inner) => inner.contains_generic_or_unknown(),
            Type::Tuple(items) => items.iter().any(|f| f.contains_generic_or_unknown()),
            Type::Option(inner) => inner.contains_generic_or_unknown(),
            Type::Function {
                params,
                return_type,
            } => {
                return_type.contains_generic_or_unknown()
                    || params.iter().any(|f| f.contains_generic_or_unknown())
            }
        }
    }

    /// Given a type that has generics (usually from built-ins), makes the
    /// generics unique by shifting them forward accoridng to the
    /// next_generic_num
    pub fn make_generics_unique(&mut self, next_generic_num: &mut FetchAndAdd<u8>) {
        let mut generic_set: HashSet<u8> = HashSet::new();
        self.generic_visitor(&mut generic_set);

        if generic_set.is_empty() {
            return; // nothing else to do
        }

        // ok great. now, devise generic shifts.
        // maps from previos value to new value
        let generic_shifts: HashMap<u8, u8> = generic_set
            .iter()
            .map(|elt| (*elt, next_generic_num.fetch_and_add()))
            .collect();

        // now finally, apply these shifts.
        self.generic_shift_applicator(&generic_shifts);
    }

    /// Visits a type, and identifies all the generic numbers inside.
    fn generic_visitor(&self, set: &mut HashSet<u8>) {
        match self {
            Type::Int
            | Type::Float
            | Type::Bool
            | Type::String
            | Type::Location
            | Type::Qubit
            | Type::QubitMap
            | Type::Gate
            | Type::ArchT
            | Type::StateT
            | Type::InstrT
            | Type::UserDef(_)
            | Type::Unknown => { /* do nothing */ }
            Type::Vec(inner) => inner.generic_visitor(set),
            Type::Tuple(items) => items.iter().for_each(|elt| elt.generic_visitor(set)),
            Type::Option(inner) => inner.generic_visitor(set),
            Type::Function {
                params,
                return_type,
            } => {
                params.iter().for_each(|elt| elt.generic_visitor(set));
                return_type.generic_visitor(set);
            }
            Type::Generic(c) => {
                set.insert(*c);
            }
        }
    }

    /// Visits a type, and maps all generics from one value to another using
    /// the shifts HashMap.
    fn generic_shift_applicator(&mut self, shifts: &HashMap<u8, u8>) {
        match self {
            Type::Int
            | Type::Float
            | Type::Bool
            | Type::String
            | Type::Location
            | Type::Qubit
            | Type::QubitMap
            | Type::Gate
            | Type::ArchT
            | Type::StateT
            | Type::InstrT
            | Type::UserDef(_)
            | Type::Unknown => { /* do nothing */ }
            Type::Vec(inner) => inner.generic_shift_applicator(shifts),
            Type::Tuple(items) => items
                .iter_mut()
                .for_each(|elt| elt.generic_shift_applicator(shifts)),
            Type::Option(inner) => inner.generic_shift_applicator(shifts),
            Type::Function {
                params,
                return_type,
            } => {
                params
                    .iter_mut()
                    .for_each(|elt| elt.generic_shift_applicator(shifts));
                return_type.generic_shift_applicator(shifts);
            }
            Type::Generic(c) => {
                *self = Type::Generic(*shifts.get(c).unwrap());
            }
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
                    first.fmt(f)?;
                    for item in iter {
                        f.write_str(", ")?;
                        item.fmt(f)?;
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

/// There are user-defined structs, like Transition.
/// This provides a single place to store information about these types.
/// Then, the types of the fields of the user-defined types can be determined
/// from here.
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
    /// user-defined types, which really only happens for testing.
    pub fn empty() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    /// Only used by tests
    /// TODO: Make this only compile in tests
    pub fn add(&mut self, name: String, fields: HashMap<String, Type>) {
        self.map.insert(name, UserDefEntry { fields });
    }

    /// Gets the fields of a user-defined type, if the type has a definition.
    ///
    /// The returned value, if exists, is a map from field names (strings) to
    /// their associated types.
    pub fn get_fields(&self, identifier: &str) -> Option<&HashMap<String, Type>> {
        self.map.get(identifier).map(|elt| &elt.fields)
    }
}
