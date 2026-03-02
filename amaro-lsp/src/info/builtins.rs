// place for defining built-in functions. this means "raw" functions like map,
// but also functions like .contains

use crate::parser::symbols::Type;
use std::sync::OnceLock;

static GLOBAL: OnceLock<Vec<BuiltIn>> = OnceLock::new();


/// Given an identifier, gets the "raw" built-in associated with it.
/// A "raw" built-in is something (usually a function) that is pre-defined
/// and does not deal with any "indexing". For instace, value_swap is a
/// raw built-in.
pub fn get_raw_built_in(identifier: &str) -> Option<&BuiltIn> {
    let data = GLOBAL.get_or_init(init_global);

    data.iter().filter(|elt| elt.parent_type.is_none()).find(|elt| elt.identifier == identifier)
}

/// Gets all "raw" built-ins. Useful for providing suggestions.
/// A "raw" built-in is something (usually a function) that is pre-defined
/// and does not deal with any "indexing". For instace, value_swap is a
/// raw built-in.
pub fn get_all_raw_built_ins() -> Vec<&'static BuiltIn> { // note the static lifetime
    let data = GLOBAL.get_or_init(init_global);

    data.iter().filter(|elt| elt.parent_type.is_none()).collect()
}

/// Gets all the built-ins that come after a type.
/// For instance, if the type is Vec, then it will give the built-ins for the
/// contains, push, pop, etc functions.
/// 
/// TODO this is awkward with generics.
pub fn get_all_built_ins_after_type(t1: &Type) -> Vec<&'static BuiltIn> {
    let data = GLOBAL.get_or_init(init_global);

    data.iter().filter(|elt| elt.parent_type.is_some()).filter(|elt| *elt.parent_type.as_ref().unwrap() == *t1).collect()
}

/// After a type, checks if the built-in with name "identifier" is valid.
/// For instance, if t1 is Arch and identifier is width, check if Arch.width
/// is a valid built-in (which, it is!). If it is, provides the BuiltIn info.
/// Otherwise, gives None.
pub fn check_built_in_after_type(t1: &Type, identifier: &str) -> Option<&'static BuiltIn> {
    let data = GLOBAL.get_or_init(init_global);

    data.iter().filter(|elt| elt.parent_type.is_some()).filter(|elt| *elt.parent_type.as_ref().unwrap() == *t1).find(|elt| elt.identifier == identifier)
}

/// Run this in get_or_init
fn init_global() -> Vec<BuiltIn> {
    vec![

        // constructors
        BuiltIn {
            parent_type: None,
            identifier: "Qubit".to_string(),
            typ: Type::Function {
                params: vec![Type::Int],
                return_type: Box::new(Type::Qubit)
            },
            details: "Constructor for Qubit".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "Location".to_string(),
            typ: Type::Function { params: vec![Type::Int], return_type: Box::new(Type::Location) },
            details: "Constructor for Location".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "Vec".to_string(),
            typ: Type::Function { params: vec![], return_type: Box::new(Type::Vec(Box::new(Type::Unknown))) },
            details: "Constructor for Vec".to_string()
        },

        // gates
        BuiltIn {
            parent_type: None,
            identifier: "CX".to_string(),
            typ: Type::Gate,
            details: "CX gate".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "T".to_string(),
            typ: Type::Gate,
            details: "T gate".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "Pauli".to_string(),
            typ: Type::Gate,
            details: "Pauli gate".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "PauliMeasurement".to_string(),
            typ: Type::Gate,
            details: "PauliMeasurement gate".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "H".to_string(),
            typ: Type::Gate,
            details: "H gate".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "CZ".to_string(),
            typ: Type::Gate,
            details: "CZ gate".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "X".to_string(),
            typ: Type::Gate,
            details: "X gate".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "Y".to_string(),
            typ: Type::Gate,
            details: "Y gate".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "Z".to_string(),
            typ: Type::Gate,
            details: "Z gate".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "S".to_string(),
            typ: Type::Gate,
            details: "S gate".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "Sdg".to_string(),
            typ: Type::Gate,
            details: "Sdg gate".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "Tdg".to_string(),
            typ: Type::Gate,
            details: "Tdg gate".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "RX".to_string(),
            typ: Type::Gate,
            details: "RX gate".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "RY".to_string(),
            typ: Type::Gate,
            details: "RY gate".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "RZ".to_string(),
            typ: Type::Gate,
            details: "RZ gate".to_string()
        },

        // functions
        BuiltIn {
            parent_type: None,
            identifier: "value_swap".to_string(),
            typ: Type::Function { params: vec![Type::Location, Type::Location], return_type: Box::new(Type::QubitMap) },
            details: "[TODO info about value_swap goes here]".to_string()
        },

        BuiltIn {
            parent_type: None,
            identifier: "values".to_string(),
            typ: Type::Function { params: vec![Type::QubitMap], return_type: Box::new(Type::Vec(Box::new(Type::Location))) },
            details: "[TODO info about values goes here]".to_string()
        },

        BuiltIn {
            parent_type: None,
            identifier: "identity_application".to_string(),
            typ: Type::Function { params: vec![Type::Unknown], return_type: Box::new(Type::Unknown) },
            details: "[TODO info about identity_application goes here]".to_string()
        },

        // generic map
        BuiltIn {
            parent_type: None,
            identifier: "map".to_string(),
            typ: Type::Function { params: vec![
                Type::Function { 
                    params: vec![
                        Type::Generic(0)
                    ], 
                    return_type: Box::new(Type::Generic(1)) }, 
                Type::Vec(Box::new(Type::Generic(0)))], return_type: Box::new(Type::Vec(Box::new(Type::Generic(1)))) },
            details: "Turns a Vec of one type into a Vec of another type by mapping each element.".to_string()
        },
        // generic fold
        BuiltIn {
            parent_type: None,
            identifier: "fold".to_string(),
            typ: Type::Function { params: vec![
                Type::Generic(1), // init acc value
                Type::Function { params: vec![
                    Type::Generic(1), // acc
                    Type::Generic(0), // elt
                ], return_type: Box::new(Type::Generic(1)) },
                Type::Vec(Box::new(Type::Generic(0)))], return_type: Box::new(Type::Unknown) },
            details: "Given a Vec and an initial accumulation value, runs the accumulation function at each element to eventually return a single accumulated value.".to_string()
        },

        //neighbor fcns
        BuiltIn {
            parent_type: None,
            identifier: "vertical_neighbors".to_string(),
            typ: Type::Function { params: vec![
                Type::Location,
                Type::Int,
                Type::Int
                ], 
                return_type: Box::new(Type::Vec(Box::new(Type::Location))) },
            details: "[TODO info about vertical_neighbors goes here]".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "horizontal_neighbors".to_string(),
            typ: Type::Function { params: vec![
                Type::Location,
                Type::Int
                ], 
                return_type: Box::new(Type::Vec(Box::new(Type::Location))) },
            details: "[TODO info about horizontal_neighbors goes here]".to_string()
        },

        // path fcns
        BuiltIn {
            parent_type: None,
            identifier: "path".to_string(),
            typ: Type::Function { params: vec![], 
                return_type: Box::new(Type::Vec(Box::new(Type::Location))) },
            details: "[TODO info about path goes here]".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "tree".to_string(),
            typ: Type::Function { params: vec![], 
                return_type: Box::new(Type::Vec(Box::new(Type::Location))) },
            details: "[TODO info about tree goes here]".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "all_paths".to_string(),
            typ: Type::Function { params: vec![
                Type::ArchT,
                Type::Vec(Box::new(Type::Location)),
                Type::Vec(Box::new(Type::Location)),
                Type::Vec(Box::new(Type::Location)),
            ], 
                return_type: Box::new(Type::Vec(Box::new(Type::Vec(Box::new(Type::Location))))) },
            details: "[TODO info about all_paths goes here]".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "shortest_path".to_string(),
            typ: Type::Function { params: vec![
                Type::ArchT,
                Type::Vec(Box::new(Type::Location)),
                Type::Vec(Box::new(Type::Location)),
                Type::Vec(Box::new(Type::Location)),
            ], 
                return_type: Box::new(Type::Vec(Box::new(Type::Vec(Box::new(Type::Location))))) },
            details: "[TODO info about shortest_path goes here]".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "steiner_trees".to_string(),
            typ: Type::Function { params: vec![
                Type::ArchT,
                Type::Vec(Box::new(Type::Vec(Box::new(Type::Location)))),
                Type::Vec(Box::new(Type::Location)),
            ], 
                return_type: Box::new(Type::Vec(Box::new(Type::Location))) },
            details: "[TODO info about steiner_trees goes here]".to_string()
        },


        // type-specific '.' functions and fields
        // Gate
        BuiltIn {
            parent_type: Some(Type::Gate),
            identifier: "qubits".to_string(),
            typ: Type::Vec(Box::new(Type::Qubit)),
            details: "[TODO info about .qubits goes here]".to_string()   
        },
        BuiltIn {
            parent_type: Some(Type::Gate),
            identifier: "gate_type".to_string(),
            typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Gate),
                },
            details: "[TODO info about .gate_type goes here]".to_string()   
        },
        BuiltIn {
            parent_type: Some(Type::Gate),
            identifier: "implementation".to_string(),
            typ: Type::Unknown,
            details: "[TODO info about .implementation goes here]".to_string()   
        },
        BuiltIn {
            parent_type: Some(Type::Gate),
            identifier: "x_indices".to_string(),
            typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Qubit))),
                },
            details: "[TODO info about .x_indices goes here]".to_string()   
        },
        BuiltIn {
            parent_type: Some(Type::Gate),
            identifier: "y_indices".to_string(),
            typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Qubit))),
                },
            details: "[TODO info about .y_indices goes here]".to_string()   
        },
        BuiltIn {
            parent_type: Some(Type::Gate),
            identifier: "z_indices".to_string(),
            typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Qubit))),
                },
            details: "[TODO info about .z_indices goes here]".to_string()   
        },


        // Arch
        BuiltIn {
            parent_type: Some(Type::ArchT),
            identifier: "width".to_string(),
            typ: Type::Int,
            details: "Width of the architecture".to_string()   
        },
        BuiltIn {
            parent_type: Some(Type::ArchT),
            identifier: "height".to_string(),
            typ: Type::Int,
            details: "Height of the architecture".to_string()   
        },
        BuiltIn {
            parent_type: Some(Type::ArchT),
            identifier: "stack_size".to_string(),
            typ: Type::Int,
            details: "Stack size of the architecture".to_string()   
        },
        BuiltIn {
            parent_type: Some(Type::ArchT),
            identifier: "edges".to_string(),
            typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Tuple(vec![
                        Type::Location,
                        Type::Location,
                    ])))),
                },
            details: "Edges of the architecture".to_string()   
        },
        BuiltIn {
            parent_type: Some(Type::ArchT),
            identifier: "succ_rates".to_string(),
            typ: Type::Vec(Box::new(Type::Vec(Box::new(Type::Float)))),
            details: "[TODO info about Arch.succ_rates goes here]".to_string()   
        },
        BuiltIn {
            parent_type: Some(Type::ArchT),
            identifier: "contains_edge".to_string(),
            typ: Type::Function {
                    params: vec![Type::Tuple(vec![Type::Location, Type::Location])],
                    return_type: Box::new(Type::Bool),
                },
            details: "[TODO info about Arch.contains_edge goes here]".to_string()   
        },

        BuiltIn {
            parent_type: Some(Type::ArchT),
            identifier: "magic_state_qubits".to_string(),
            typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Location))),
                },
            details: "[TODO info about Arch.magic_state_qubits goes here]".to_string()   
        },
        BuiltIn {
            parent_type: Some(Type::ArchT),
            identifier: "alg_qubits".to_string(),
            typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Location))),
                },
            details: "[TODO info about Arch.alg_qubits goes here]".to_string()   
        },


        // State
        BuiltIn {
            parent_type: Some(Type::StateT),
            identifier: "map".to_string(),
            typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::QubitMap),
                },
            details: "[TODO info about State.map goes here]".to_string()   
        },
        BuiltIn {
            parent_type: Some(Type::StateT),
            identifier: "gates".to_string(),
            typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Gate))),
                },
            details: "[TODO info about State.gates goes here]".to_string()   
        },
        BuiltIn {
            parent_type: Some(Type::StateT),
            identifier: "implemented_gates".to_string(),
            typ: Type::Unknown,
            details: "[TODO info about State.implemented_gates goes here]".to_string()   
        },


        // Vec
        BuiltIn {
            parent_type: Some(Type::Vec(Box::new(Type::Generic(0)))),
            identifier: "push".to_string(),
            typ: Type::Function {
                    params: vec![Type::Generic(0)],
                    return_type: Box::new(Type::Vec(Box::new(Type::Generic(0)))),
                },
            details: "Pushes an element to the Vec.".to_string()   
        },
        BuiltIn {
            parent_type: Some(Type::Vec(Box::new(Type::Generic(0)))),
            identifier: "pop".to_string(),
            typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Option(Box::new(Type::Generic(0)))),
                },
            details: "Pops the last element from the Vec.".to_string()   
        },
        BuiltIn {
            parent_type: Some(Type::Vec(Box::new(Type::Generic(0)))),
            identifier: "extend".to_string(),
            typ: Type::Function {
                    params: vec![Type::Vec(Box::new(Type::Generic(0)))],
                    return_type: Box::new(Type::Vec(Box::new(Type::Generic(0)))),
                },
            details: "Extends this Vec with all elements of another Vec.".to_string()   
        },
        BuiltIn {
            parent_type: Some(Type::Vec(Box::new(Type::Generic(0)))),
            identifier: "is_empty".to_string(),
            typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Bool),
                },
            details: "Determines whether this Vec has elements or not.".to_string()   
        },
        BuiltIn {
            parent_type: Some(Type::Vec(Box::new(Type::Generic(0)))),
            identifier: "contains".to_string(),
            typ: Type::Function {
                    params: vec![Type::Generic(0)],
                    return_type: Box::new(Type::Bool),
                },
            details: "Determines whether this Vec contains the element or not.".to_string()   
        },
        BuiltIn {
            parent_type: Some(Type::Vec(Box::new(Type::Generic(0)))),
            identifier: "len".to_string(),
            typ: Type::Int,
            details: "Gets the number of elements in the Vec.".to_string()   
        },

        // Option
        // TODO not sure what Option needs. Does it need all the stuff??
        BuiltIn {
            parent_type: Some(Type::Option(Box::new(Type::Generic(0)))),
            identifier: "unwrap".to_string(),
            typ: Type::Function { params: vec![], return_type: Box::new(Type::Generic(0)) },
            details: "Gets the value in the option if the value exists. Panics if no value exists.".to_string()
        },
    ]
}

/// DO NOT IMPLEMENT CLONE OR COPY.
/// THIS SHOULD NOT BE CLONED OR COPIED.
#[derive(Debug)]
pub struct BuiltIn {
    pub parent_type: Option<Type>,
    pub identifier: String,
    pub typ: Type,
    pub details: String,
}
