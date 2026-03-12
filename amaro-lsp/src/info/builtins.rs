// place for defining built-in keywords and functions. this means "raw" functions like
// map, but also functions like .contains on a vec. also just types too, like Arch.
// the SymbolTable should derive from this, along with everything else.
// no hard-coding should take place except for in here.
// then, if later plans change about the types, only this needs to be modified.

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, MarkupContent,
};

use crate::parser::{semantics, symbols::Type};
use std::{collections::HashMap, sync::OnceLock};

pub enum Owner<'a, T> {
    Owned(T),
    Borrowed(&'a T),
}

/// Global, persistent location where the built-ins are kept. They do not change
/// between different files.
static GLOBAL: OnceLock<Vec<(Option<Type>, Vec<BuiltIn>)>> = OnceLock::new();

/// Given an identifier, gets the "raw" built-in associated with it.
/// A "raw" built-in is something (usually a function) that is pre-defined
/// and does not deal with any "indexing". For instace, value_swap is a
/// raw built-in.
pub fn get_raw_built_in(identifier: &str) -> Option<&BuiltIn> {
    let data = GLOBAL.get_or_init(init_global);

    // TODO write test that ensures .unwrap always works.
    data.iter()
        .find(|elt| elt.0.is_none())
        .unwrap()
        .1
        .iter()
        .find(|elt| elt.identifier == identifier)
}

/// Gets all "raw" built-ins. Useful for providing suggestions.
/// A "raw" built-in is something (usually a function) that is pre-defined
/// and does not deal with any "indexing". For instace, value_swap is a
/// raw built-in.
pub fn get_all_raw_built_ins() -> Vec<&'static BuiltIn> {
    // note the static lifetime
    let data = GLOBAL.get_or_init(init_global);
    data.iter()
        .find(|elt| elt.0.is_none())
        .unwrap()
        .1
        .iter()
        .collect()
}

/// Gets all the built-ins that come after a type.
/// For instance, if the type is Vec, then it will give the built-ins for the
/// contains, push, pop, etc functions.
///
/// TODO this is awkward with generics.
pub fn get_all_built_ins_after_type(t1: &Type) -> Option<Owner<'static, Vec<BuiltIn>>> {
    let data = GLOBAL.get_or_init(init_global);

    // if type has generics, then we need to be able to understand this.
    let res: Option<(&Vec<BuiltIn>, HashMap<u8, Type>)> =
        data.iter().filter(|elt| elt.0.is_some()).find_map(|elt| {
            let mut map = HashMap::new();
            match semantics::infer_generic_type(elt.0.as_ref().unwrap(), t1, &mut map) {
                Err(_) => None,
                Ok(_) => Some((elt.1.as_ref(), map)),
            }
        });

    match res {
        None => {
            // either nothing after the type t1, or generics were handled improperly.
            None
        }
        Some(pair) => {
            if pair.1.is_empty() {
                // we can just output the vec, bc it is not generic
                Some(Owner::Borrowed(pair.0))
            } else {
                // well, we have some elements in the map.
                // this indicates that there are generics.
                // we need to output modified built-ins to respect this

                // LS: There are a lot of allocations here. Could this be improved?
                Some(Owner::Owned(
                    pair.0
                        .iter()
                        .filter_map(|elt| match semantics::degenerisize(&elt.typ, &pair.1) {
                            (false, _) => None, // failed to degenerisize
                            (_, true_type) => Some(BuiltIn {
                                parent_type: elt.parent_type.clone(),
                                identifier: elt.identifier.clone(),
                                typ: true_type,
                                details: elt.details.clone(),
                            }),
                        })
                        .collect(),
                ))
            }
        }
    }
}

/// After a type, checks if the built-in with name "identifier" is valid.
/// For instance, if t1 is Arch and identifier is width, check if Arch.width
/// is a valid built-in (which, it is!). If it is, provides the BuiltIn info.
/// Otherwise, gives None.
/// Handles generics, giving the proper type.
pub fn check_built_in_after_type(t1: &Type, identifier: &str) -> Option<Owner<'static, BuiltIn>> {
    let data = GLOBAL.get_or_init(init_global);

    match data.iter().filter(|elt| elt.0.is_some()).find_map(|elt| {
        let mut map = HashMap::new();
        match semantics::infer_generic_type(elt.0.as_ref().unwrap(), t1, &mut map) {
            Ok(_) => Some((&elt.1, map)),
            Err(_) => None,
        }
    }) {
        None => None, // did not find assoc type in our data
        Some((built_in_vec, map)) => {
            built_in_vec
                .iter()
                .find(|elt| elt.identifier == identifier)
                .map(|found| {
                    if map.is_empty() {
                        Owner::Borrowed(found)
                    } else {
                        match semantics::degenerisize(&found.typ, &map) {
                            (false, _) => Owner::Borrowed(found), // This case is a fall back if something goes wrong
                            // LS: Investigate this case and see if it's needed or what we could
                            // maybe do about it.
                            (_, degenerisized_type) => Owner::Owned(BuiltIn {
                                parent_type: Some(t1.clone()),
                                identifier: identifier.to_string(),
                                typ: degenerisized_type,
                                details: found.details.clone(),
                            }),
                        }
                    }
                })
        }
    }
}

/// Run this in get_or_init
fn init_global() -> Vec<(Option<Type>, Vec<BuiltIn>)> {
    vec![
        (None, vec![
        // type keywords
        BuiltIn {
            parent_type: None,
            identifier: "Arch".to_string(),
            typ: Type::ArchT,
            details: "Architecture type.".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "arch".to_string(),
            typ: Type::ArchT,
            details: "Architecture type.".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "State".to_string(),
            typ: Type::StateT,
            details: "State type.".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "Gate".to_string(),
            typ: Type::Gate,
            details: "Gate type.".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "step".to_string(),
            typ: Type::Int,
            details: "Step type. Alias for Int".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "Transition".to_string(),
            typ: Type::UserDef("Transition".to_string()),
            details: "Transition type. Fields defined by user.".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "GateRealization".to_string(),
            typ: Type::UserDef("GateRealization".to_string()),
            details: "Step type. Alias for Int".to_string()
        },


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
            details: "".to_string() // TODO details
        },

        BuiltIn {
            parent_type: None,
            identifier: "values".to_string(),
            typ: Type::Function { params: vec![Type::QubitMap], return_type: Box::new(Type::Vec(Box::new(Type::Location))) },
            details: "".to_string() // TODO details
        },

        BuiltIn {
            parent_type: None,
            identifier: "identity_application".to_string(),
            typ: Type::Function { params: vec![Type::Unknown], return_type: Box::new(Type::Unknown) },
            details: "".to_string() // TODO details
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
            details: "".to_string() // TODO details
        },
        BuiltIn {
            parent_type: None,
            identifier: "horizontal_neighbors".to_string(),
            typ: Type::Function { params: vec![
                Type::Location,
                Type::Int
                ],
                return_type: Box::new(Type::Vec(Box::new(Type::Location))) },
            details: "".to_string() // TODO details
        },

        // path fcns
        BuiltIn {
            parent_type: None,
            identifier: "path".to_string(),
            typ: Type::Function {
                params: vec![],
                return_type: Box::new(Type::Vec(Box::new(Type::Location)))
            },
            details: "".to_string() // TODO details
        },
        BuiltIn {
            parent_type: None,
            identifier: "tree".to_string(),
            typ: Type::Function {
                params: vec![],
                return_type: Box::new(Type::Vec(Box::new(Type::Location)))
            },
            details: "".to_string() // TODO details
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
            details: "".to_string() // TODO details
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
            details: "".to_string() // TODO details
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
            details: "".to_string() // TODO details
        },
    ]),
    (
        Some(Type::Gate),
        vec![
            // type-specific '.' functions and fields
            // Gate
            BuiltIn {
                parent_type: Some(Type::Gate),
                identifier: "qubits".to_string(),
                typ: Type::Vec(Box::new(Type::Qubit)),
                details: "[TODO info about .qubits goes here]".to_string(),
            },
            BuiltIn {
                parent_type: Some(Type::Gate),
                identifier: "gate_type".to_string(),
                typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Gate),
                },
                details: "[TODO info about .gate_type goes here]".to_string(),
            },
            BuiltIn {
                parent_type: Some(Type::Gate),
                identifier: "implementation".to_string(),
                typ: Type::Unknown,
                details: "[TODO info about .implementation goes here]".to_string(),
            },
            BuiltIn {
                parent_type: Some(Type::Gate),
                identifier: "x_indices".to_string(),
                typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Qubit))),
                },
                details: "[TODO info about .x_indices goes here]".to_string(),
            },
            BuiltIn {
                parent_type: Some(Type::Gate),
                identifier: "y_indices".to_string(),
                typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Qubit))),
                },
                details: "[TODO info about .y_indices goes here]".to_string(),
            },
            BuiltIn {
                parent_type: Some(Type::Gate),
                identifier: "z_indices".to_string(),
                typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Qubit))),
                },
                details: "[TODO info about .z_indices goes here]".to_string(),
            },
        ],
    ),
    (
        Some(Type::ArchT),
        vec![
            // Arch
            BuiltIn {
                parent_type: Some(Type::ArchT),
                identifier: "width".to_string(),
                typ: Type::Int,
                details: "Width of the architecture".to_string(),
            },
            BuiltIn {
                parent_type: Some(Type::ArchT),
                identifier: "height".to_string(),
                typ: Type::Int,
                details: "Height of the architecture".to_string(),
            },
            BuiltIn {
                parent_type: Some(Type::ArchT),
                identifier: "stack_size".to_string(),
                typ: Type::Int,
                details: "Stack size of the architecture".to_string(),
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
                details: "Edges of the architecture".to_string(),
            },
            BuiltIn {
                parent_type: Some(Type::ArchT),
                identifier: "succ_rates".to_string(),
                typ: Type::Vec(Box::new(Type::Vec(Box::new(Type::Float)))),
                details: "[TODO info about Arch.succ_rates goes here]".to_string(),
            },
            BuiltIn {
                parent_type: Some(Type::ArchT),
                identifier: "contains_edge".to_string(),
                typ: Type::Function {
                    params: vec![Type::Tuple(vec![Type::Location, Type::Location])],
                    return_type: Box::new(Type::Bool),
                },
                details: "[TODO info about Arch.contains_edge goes here]".to_string(),
            },
            BuiltIn {
                parent_type: Some(Type::ArchT),
                identifier: "magic_state_qubits".to_string(),
                typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Location))),
                },
                details: "[TODO info about Arch.magic_state_qubits goes here]".to_string(),
            },
            BuiltIn {
                parent_type: Some(Type::ArchT),
                identifier: "alg_qubits".to_string(),
                typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Location))),
                },
                details: "[TODO info about Arch.alg_qubits goes here]".to_string(),
            },
        ],
    ),
    (
        Some(Type::StateT),
        vec![
            // State
            BuiltIn {
                parent_type: Some(Type::StateT),
                identifier: "map".to_string(),
                typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::QubitMap),
                },
                details: "[TODO info about State.map goes here]".to_string(),
            },
            BuiltIn {
                parent_type: Some(Type::StateT),
                identifier: "gates".to_string(),
                typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Gate))),
                },
                details: "[TODO info about State.gates goes here]".to_string(),
            },
            BuiltIn {
                parent_type: Some(Type::StateT),
                identifier: "implemented_gates".to_string(),
                typ: Type::Function { params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Unknown))) }, // TODO what is the type of this?
                details: "[TODO info about State.implemented_gates goes here]".to_string(),
            },
        ],
    ),
    (
        Some(Type::Vec(Box::new(Type::Generic(0)))),
        vec![
            // Vec
            BuiltIn {
                parent_type: Some(Type::Vec(Box::new(Type::Generic(0)))),
                identifier: "push".to_string(),
                typ: Type::Function {
                    params: vec![Type::Generic(0)],
                    return_type: Box::new(Type::Vec(Box::new(Type::Generic(0)))),
                },
                details: "Pushes an element to the Vec.".to_string(),
            },
            BuiltIn {
                parent_type: Some(Type::Vec(Box::new(Type::Generic(0)))),
                identifier: "pop".to_string(),
                typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Option(Box::new(Type::Generic(0)))),
                },
                details: "Pops the last element from the Vec.".to_string(),
            },
            BuiltIn {
                parent_type: Some(Type::Vec(Box::new(Type::Generic(0)))),
                identifier: "extend".to_string(),
                typ: Type::Function {
                    params: vec![Type::Vec(Box::new(Type::Generic(0)))],
                    return_type: Box::new(Type::Vec(Box::new(Type::Generic(0)))),
                },
                details: "Extends this Vec with all elements of another Vec.".to_string(),
            },
            BuiltIn {
                parent_type: Some(Type::Vec(Box::new(Type::Generic(0)))),
                identifier: "is_empty".to_string(),
                typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Bool),
                },
                details: "Determines whether this Vec has elements or not.".to_string(),
            },
            BuiltIn {
                parent_type: Some(Type::Vec(Box::new(Type::Generic(0)))),
                identifier: "contains".to_string(),
                typ: Type::Function {
                    params: vec![Type::Generic(0)],
                    return_type: Box::new(Type::Bool),
                },
                details: "Determines whether this Vec contains the element or not.".to_string(),
            },
            BuiltIn {
                parent_type: Some(Type::Vec(Box::new(Type::Generic(0)))),
                identifier: "len".to_string(),
                typ: Type::Int,
                details: "Gets the number of elements in the Vec.".to_string(),
            },
        ],
    ),
    (
        Some(Type::Option(Box::new(Type::Generic(0)))),
        vec![
            // Option
            // TODO not sure what Option needs. Does it need all the stuff??
            BuiltIn {
                parent_type: Some(Type::Option(Box::new(Type::Generic(0)))),
                identifier: "unwrap".to_string(),
                typ: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Generic(0)),
                },
                details:
                    "Gets the value in the option if the value exists. Panics if no value exists."
                        .to_string(),
            },
        ],
    )
    ]
}

/// DO NOT IMPLEMENT CLONE OR COPY.
/// THIS SHOULD NOT BE CLONED OR COPIED.
#[derive(Debug, PartialEq)]
pub struct BuiltIn {
    pub parent_type: Option<Type>, // redundant. remove this extra info
    pub identifier: String,
    pub typ: Type,
    pub details: String,
}

impl BuiltIn {
    pub fn to_completion_item(
        &self,
        additional_text_edits: Option<Vec<tower_lsp::lsp_types::TextEdit>>,
    ) -> CompletionItem {
        CompletionItem {
            label: self.identifier.clone(),
            label_details: Some(BuiltIn::get_completion_item_label_details(&self.typ)),
            kind: Some(BuiltIn::type_to_completion_item_kind(&self.typ)),
            // detail: Some(BuiltIn::get_completion_item_detail(&self.typ, &self.details)),
            documentation: Some(tower_lsp::lsp_types::Documentation::MarkupContent(
                MarkupContent {
                    kind: tower_lsp::lsp_types::MarkupKind::Markdown,
                    value: BuiltIn::get_completion_item_detail(&self.typ, &self.details),
                },
            )),
            sort_text: Some(self.to_sort_string()),
            additional_text_edits,
            ..Default::default()
        }
    }

    fn to_sort_string(&self) -> String {
        match self.typ {
            Type::Function { .. } => format!("a{}", self.identifier),
            _ => format!("z{}{}", self.typ, self.identifier),
        }
    }

    fn type_to_completion_item_kind(typ: &Type) -> CompletionItemKind {
        match typ {
            Type::Int => CompletionItemKind::FIELD,
            Type::Float => CompletionItemKind::FIELD,
            Type::Bool => CompletionItemKind::FIELD,
            Type::String => CompletionItemKind::FIELD,
            Type::Location => CompletionItemKind::STRUCT,
            Type::Qubit => CompletionItemKind::STRUCT,
            Type::QubitMap => CompletionItemKind::STRUCT,
            Type::Gate => CompletionItemKind::STRUCT,
            Type::ArchT => CompletionItemKind::STRUCT,
            Type::StateT => CompletionItemKind::STRUCT,
            Type::InstrT => CompletionItemKind::STRUCT,
            Type::Vec(..) => CompletionItemKind::VARIABLE,
            Type::Tuple(..) => CompletionItemKind::VARIABLE,
            Type::Option(..) => CompletionItemKind::ENUM,
            Type::Function { .. } => CompletionItemKind::FUNCTION,
            //Type::Struct { .. } => CompletionItemKind::STRUCT,
            Type::UserDef(..) => CompletionItemKind::STRUCT,
            _ => CompletionItemKind::CONSTANT,
        }
    }

    fn get_completion_item_label_details(typ: &Type) -> CompletionItemLabelDetails {
        CompletionItemLabelDetails {
            detail: match typ {
                Type::Vec(..) => Some(": Array".to_string()),
                Type::Tuple(..) => Some(": Tuple".to_string()),
                Type::Function { .. } => Some(": Function".to_string()),
                Type::UserDef { .. } => Some(": Struct".to_string()),
                _ => None,
            },
            description: Some(format!("{}", typ)),
        }
    }

    fn get_completion_item_detail(typ: &Type, details: &str) -> String {
        format!("{}\n\n{}", typ, details)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::info::builtins::get_raw_built_in;

    #[test]
    fn test_get_raw_built_in() {
        assert!(get_raw_built_in("total_garbage").is_none());

        let built_in = get_raw_built_in("value_swap");
        assert!(built_in.is_some());
        let built_in = built_in.unwrap();
        assert!(built_in.identifier == "value_swap");
        assert!(built_in.parent_type == None);
        assert!(
            built_in.typ
                == Type::Function {
                    params: vec![Type::Location, Type::Location],
                    return_type: Box::new(Type::QubitMap)
                }
        );

        assert!(get_raw_built_in("contains").is_none());
    }

    #[test]
    fn test_get_all_raw() {
        let raws_expected = vec![
        // type keywords
        BuiltIn {
            parent_type: None,
            identifier: "Arch".to_string(),
            typ: Type::ArchT,
            details: "Architecture type.".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "arch".to_string(),
            typ: Type::ArchT,
            details: "Architecture type.".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "State".to_string(),
            typ: Type::StateT,
            details: "State type.".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "Gate".to_string(),
            typ: Type::Gate,
            details: "Gate type.".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "step".to_string(),
            typ: Type::Int,
            details: "Step type. Alias for Int".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "Transition".to_string(),
            typ: Type::UserDef("Transition".to_string()),
            details: "Transition type. Fields defined by user.".to_string()
        },
        BuiltIn {
            parent_type: None,
            identifier: "GateRealization".to_string(),
            typ: Type::UserDef("GateRealization".to_string()),
            details: "Step type. Alias for Int".to_string()
        },


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
            details: "".to_string() // TODO details
        },

        BuiltIn {
            parent_type: None,
            identifier: "values".to_string(),
            typ: Type::Function { params: vec![Type::QubitMap], return_type: Box::new(Type::Vec(Box::new(Type::Location))) },
            details: "".to_string() // TODO details
        },

        BuiltIn {
            parent_type: None,
            identifier: "identity_application".to_string(),
            typ: Type::Function { params: vec![Type::Unknown], return_type: Box::new(Type::Unknown) },
            details: "".to_string() // TODO details
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
            details: "".to_string() // TODO details
        },
        BuiltIn {
            parent_type: None,
            identifier: "horizontal_neighbors".to_string(),
            typ: Type::Function { params: vec![
                Type::Location,
                Type::Int
                ],
                return_type: Box::new(Type::Vec(Box::new(Type::Location))) },
            details: "".to_string() // TODO details
        },

        // path fcns
        BuiltIn {
            parent_type: None,
            identifier: "path".to_string(),
            typ: Type::Function {
                params: vec![],
                return_type: Box::new(Type::Vec(Box::new(Type::Location)))
            },
            details: "".to_string() // TODO details
        },
        BuiltIn {
            parent_type: None,
            identifier: "tree".to_string(),
            typ: Type::Function {
                params: vec![],
                return_type: Box::new(Type::Vec(Box::new(Type::Location)))
            },
            details: "".to_string() // TODO details
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
            details: "".to_string() // TODO details
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
            details: "".to_string() // TODO details
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
            details: "".to_string() // TODO details
        },
    ];

        let raws_actual = get_all_raw_built_ins();
        for expected in raws_expected {
            assert!(
                raws_actual
                    .iter()
                    .find(|elt| elt.details == expected.details && elt.typ == expected.typ)
                    .is_some()
            );
        }
    }

    #[test]
    fn test_built_in_after_garbage() {
        assert!(get_all_built_ins_after_type(&Type::Float).is_none())
    }

    #[test]
    fn test_built_in_after_non_generic() {
        let built_ins_after = get_all_built_ins_after_type(&Type::ArchT);
        assert!(built_ins_after.is_some());
        let built_ins_after = built_ins_after.unwrap();
        if let Owner::Borrowed(actual) = built_ins_after {
            let expected = vec![
                // Arch
                BuiltIn {
                    parent_type: Some(Type::ArchT),
                    identifier: "width".to_string(),
                    typ: Type::Int,
                    details: "Width of the architecture".to_string(),
                },
                BuiltIn {
                    parent_type: Some(Type::ArchT),
                    identifier: "height".to_string(),
                    typ: Type::Int,
                    details: "Height of the architecture".to_string(),
                },
                BuiltIn {
                    parent_type: Some(Type::ArchT),
                    identifier: "stack_size".to_string(),
                    typ: Type::Int,
                    details: "Stack size of the architecture".to_string(),
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
                    details: "Edges of the architecture".to_string(),
                },
                BuiltIn {
                    parent_type: Some(Type::ArchT),
                    identifier: "succ_rates".to_string(),
                    typ: Type::Vec(Box::new(Type::Vec(Box::new(Type::Float)))),
                    details: "[TODO info about Arch.succ_rates goes here]".to_string(),
                },
                BuiltIn {
                    parent_type: Some(Type::ArchT),
                    identifier: "contains_edge".to_string(),
                    typ: Type::Function {
                        params: vec![Type::Tuple(vec![Type::Location, Type::Location])],
                        return_type: Box::new(Type::Bool),
                    },
                    details: "[TODO info about Arch.contains_edge goes here]".to_string(),
                },
                BuiltIn {
                    parent_type: Some(Type::ArchT),
                    identifier: "magic_state_qubits".to_string(),
                    typ: Type::Function {
                        params: vec![],
                        return_type: Box::new(Type::Vec(Box::new(Type::Location))),
                    },
                    details: "[TODO info about Arch.magic_state_qubits goes here]".to_string(),
                },
                BuiltIn {
                    parent_type: Some(Type::ArchT),
                    identifier: "alg_qubits".to_string(),
                    typ: Type::Function {
                        params: vec![],
                        return_type: Box::new(Type::Vec(Box::new(Type::Location))),
                    },
                    details: "[TODO info about Arch.alg_qubits goes here]".to_string(),
                },
            ];

            for entry in expected {
                assert!(
                    actual.contains(&entry),
                    "entry not found in actual: {:?}",
                    entry
                )
            }
        } else {
            panic!("expected built ins to be borrowed");
        }
    }

    #[test]
    fn test_built_in_after_vec() {
        let vec_type = Type::Vec(Box::new(Type::Int));
        let built_ins_after = get_all_built_ins_after_type(&vec_type);
        assert!(built_ins_after.is_some());
        let built_ins_after = built_ins_after.unwrap();
        assert!(matches!(built_ins_after, Owner::Owned(_)));
        if let Owner::Owned(actual) = built_ins_after {
            // see if we have some of the items
            let found = actual.iter().find(|elt| elt.identifier == "contains");
            assert!(found.is_some());
            let found = found.unwrap();
            assert!(found.parent_type.is_some());
            // LS: enable this line if we want the provided built in to indicate that
            // the parent type is non-generic
            // assert_eq!(*found.parent_type.as_ref().unwrap(), Type::Vec(Box::new(Type::Int)));
            assert_eq!(
                found.typ,
                Type::Function {
                    params: vec![Type::Int],
                    return_type: Box::new(Type::Bool)
                }
            );

            let found = actual.iter().find(|elt| elt.identifier == "pop");
            assert!(found.is_some());
            let found = found.unwrap();
            assert!(found.parent_type.is_some());
            // LS: enable this line if we want the provided built in to indicate that
            // the parent type is non-generic
            // assert_eq!(*found.parent_type.as_ref().unwrap(), Type::Vec(Box::new(Type::Int)));
            assert_eq!(
                found.typ,
                Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Option(Box::new(Type::Int)))
                }
            );
        }
    }
}
