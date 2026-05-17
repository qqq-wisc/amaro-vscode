use std::sync::OnceLock;

use crate::{info::blocks::BlockName, parser::symbols::Type};

/// Global location where field information is stored.
/// Initialized once, then used statically during program.
static FIELDS: OnceLock<Vec<FieldInfo>> = OnceLock::new();

/// Struct for recognizing the types of fields, such as that "cost" allows
/// for using "Transition" and requires a value of "Float"
#[derive(Debug)]
pub struct FieldInfo {
    /// The string name of the block that this field should reside in.
    /// For instance, "TransitionInfo".
    pub block_name: BlockName,
    /// The string name of the field. For instance, "cost".
    pub field_name: String,
    /// Human-readable information about the field.
    pub info: String,
    /// The type of the field. ALWAYS a function.
    pub typ: Type,
    /// Indicates whether this field is mandatory given the block is present
    pub mandatory_if_block_present: bool,
}

impl FieldInfo {
    /// Converts to a markdown string for displaying, usually for on-hover
    pub fn show_details(&self) -> String {
        let mut type_to_show: &Type = &self.typ;
        if let Type::Function {
            params,
            return_type,
        } = &self.typ
        {
            // if no params, just put return type
            if params.is_empty() {
                type_to_show = return_type;
            }
        }

        format!(
            "## {}\n *In block {}*\n\n{}\n\n{}",
            self.field_name,
            self.block_name.to_string(),
            type_to_show.to_markdown_display(),
            self.info
        )
    }
}

/// Call with get_or_set to initialize the static fields. Order doesn't matter
/// in this function.
fn init_fields() -> Vec<FieldInfo> {
    // order doesn't matter
    vec![
        // TransitionInfo block
        FieldInfo {
            block_name: BlockName::Transition,
            field_name: "cost".to_string(),
            info: "Cost of transitions".to_string(),
            typ: Type::Function {
                params: vec![Type::UserDef("Transition".to_string())],
                return_type: Box::new(Type::Float),
            },
            mandatory_if_block_present: true,
        },
        FieldInfo {
            block_name: BlockName::Transition,
            field_name: "get_transitions".to_string(),
            info: "".to_string(), // TODO details
            typ: Type::Function {
                params: vec![Type::ArchT, Type::StateT],
                return_type: Box::new(Type::Vec(Box::new(Type::UserDef("Transition".to_string())))),
            },
            mandatory_if_block_present: true,
        },
        FieldInfo {
            block_name: BlockName::Transition,
            field_name: "apply".to_string(),
            info: "".to_string(), // TODO details
            typ: Type::Function {
                params: vec![Type::QubitMap, Type::UserDef("Transition".to_string())],
                return_type: Box::new(Type::QubitMap),
            },
            mandatory_if_block_present: true,
        },
        // RouteInfo Block
        FieldInfo {
            block_name: BlockName::Route,
            field_name: "routed_gates".to_string(),
            info: "".to_string(), // TODO details
            typ: Type::Function {
                params: vec![],
                return_type: Box::new(Type::Vec(Box::new(Type::Gate))),
            },
            mandatory_if_block_present: true,
        },
        FieldInfo {
            block_name: BlockName::Route,
            field_name: "realize_gate".to_string(),
            info: "".to_string(), // TODO details
            typ: Type::Function {
                params: vec![Type::ArchT, Type::StateT, Type::Gate],
                return_type: Box::new(Type::Vec(Box::new(Type::UserDef(
                    "GateRealization".to_string(),
                )))),
            },
            mandatory_if_block_present: true,
        },
        // ArchInfo block
        FieldInfo {
            block_name: BlockName::Arch,
            field_name: "get_locations".to_string(),
            info: "".to_string(), // TODO info for this
            typ: Type::Function {
                params: Vec::new(), // TODO not sure what to put here
                return_type: Box::new(Type::Vec(Box::new(Type::Location))),
            },
            mandatory_if_block_present: false,
        },
        // StateInfo block
        FieldInfo {
            block_name: BlockName::State,
            field_name: "cost".to_string(),
            info: "Cost".to_string(), // TODO info for this
            typ: Type::Function {
                params: vec![Type::ArchT, Type::UserDef("GateRealization".to_string())], // TODO not 100% on these params
                return_type: Box::new(Type::Float),
            },
            mandatory_if_block_present: true,
        },
    ]
}

/// Look up information about a field based on its name and block name
pub fn field_lookup(block_name: BlockName, field_name: &str) -> Option<&'static FieldInfo> {
    let data = FIELDS.get_or_init(init_fields);

    data.iter()
        .find(|elt| elt.block_name == block_name && elt.field_name == field_name)
}

pub fn get_all_fields_under(block_name: BlockName) -> impl Iterator<Item = &'static FieldInfo> {
    let data = FIELDS.get_or_init(init_fields);

    data.iter().filter(move |elt| elt.block_name == block_name)
}

#[cfg(test)]
mod tests {
    use crate::info::blocks::BlockName;
    use crate::{info::fields::field_lookup, parser::symbols::Type};

    #[test]
    fn test_field_lookup() {
        if let Some(e) = field_lookup(BlockName::Transition, "name garbage") {
            panic!(
                "Looking up valid block name but invalid field gave output: {:?}",
                e
            );
        }

        if let Some(e) = field_lookup(BlockName::Arch, "cost") {
            panic!(
                "Looking up valid field name but invalid block gave output: {:?}",
                e
            );
        }

        if let Some(e) = field_lookup(BlockName::Transition, "realize_gate") {
            panic!(
                "Both block and field valid, but not together. Shouldn't have gotten: {:?}",
                e
            );
        }

        {
            let lookup = field_lookup(BlockName::Transition, "cost");
            assert!(lookup.is_some());
            let lookup = lookup.unwrap();
            assert_eq!(
                lookup.typ,
                Type::Function {
                    params: vec![Type::UserDef("Transition".to_string())],
                    return_type: Box::new(Type::Float)
                }
            );
        }

        {
            let lookup = field_lookup(BlockName::Route, "realize_gate");
            assert!(lookup.is_some());
            let lookup = lookup.unwrap();
            assert_eq!(
                lookup.typ,
                Type::Function {
                    params: vec![Type::ArchT, Type::StateT, Type::Gate],
                    return_type: Box::new(Type::Vec(Box::new(Type::UserDef(
                        "GateRealization".to_string(),
                    )))),
                }
            );
        }
    }
}
