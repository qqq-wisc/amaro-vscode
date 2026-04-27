use super::symbols::*;
use crate::{
    ast::*,
    info::{blocks::BlockName, builtins, fields},
};
use std::{
    collections::{HashMap, HashSet},
    ops::Add,
};
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemLabelDetails, Diagnostic, DiagnosticRelatedInformation,
    DiagnosticSeverity, Location, Position, Range, Url,
};

pub struct SemanticResult {
    /// Diagnostic information
    ///
    /// Diagnostics contain errors, warnings, and info that is passed to the
    /// editor to be reported to the user.
    pub diagnostics: Vec<Diagnostic>,

    /// Mappings from node IDs to their determined types.
    pub type_map: HashMap<NodeId, Type>,

    /// Table for information about user-defined structs like GateRealization
    /// and Transition.
    pub user_def_table: UserDefTable,

    /// Information for where to place the "Rust analyzer" style gray type
    /// inferences. Like when typing "let x = 5;", the Rust analyzer extension
    /// shows "let x : i32 = 5;" with the inferred type.
    ///
    /// This can be used to label any strings, but also field names. It can also
    /// be used to provide hover information if needed.
    pub string_labels: StringLabels,
}

/// Maps from expressions to their types (using expression IDs).
/// Also stores whether an expression is "resolved", which is most useful during
/// intermediary semantic checking steps.
#[derive(Default)]
pub struct TypeMap {
    map: HashMap<NodeId, Type>,
    /// Stores whether or not the expression is resolved.
    /// An expression is resolved iff its type and the types of all
    /// subexpressions contain no generics nor unknowns.
    /// If an expression is resolved, then we never need to infer its type ever
    /// again, and can just retrieve the type from the above map.
    resolved: HashSet<NodeId>,
}

impl TypeMap {
    pub fn new() -> TypeMap {
        Self::default()
    }
    /// Gets the type of the provided node id (expr.id), if exists.
    pub fn get(&self, id: &NodeId) -> Option<&Type> {
        self.map.get(id)
    }

    /// Sets the value in the map.
    /// overlay is preferred, as this will set the value to the new type,
    /// not overlay it.
    /// Only use set in situations where we are confident that it will produce
    /// the same result as overlay.
    pub fn set(&mut self, id: NodeId, typ: Type) -> Option<Type> {
        self.map.insert(id, typ)
    }

    /// Updates the type in the type map
    /// If no type exists, then simply puts the type in there.
    /// If a type does already exist, then attempts to just overlay atop the
    /// generics and unknowns, leaving the remaining structure unchanged.
    /// Essentially becomes "more specific" with the type, changing the type
    /// into one that is strictly "finer" and less ambiguous.
    ///
    /// Note that this naively overlays generics. So, suppose the existing
    /// type was Tuple<T1, T1> and it was overlayed with Tuple<Int, T1>. It will
    /// not recognize that Int corresponds to T1.
    ///
    /// Returns true if anything was changed, false otherwise.
    pub fn overlay(&mut self, id: NodeId, new_type: &Type) -> bool {
        match self.map.get_mut(&id) {
            Some(prev_type) => overlay_type(prev_type, new_type),
            None => {
                self.set(id, new_type.clone());
                true
            }
        }
    }

    pub fn is_resolved(&self, id: &NodeId) -> bool {
        self.resolved.contains(id)
    }

    /// Marks the expression as resolved.
    /// Once an expression has been marked as resolved, it will continue to be
    /// resolved.
    pub fn set_resolved(&mut self, id: NodeId) {
        self.resolved.insert(id);
    }
}

/// Information for where to place the "Rust analyzer" style gray type
/// inferences. Like when typing "let x = 5;", the Rust analyzer extension
/// shows "let x : i32 = 5;" with the inferred type.
///
/// This can be used to label any strings, but also field names. It can also
/// be used to provide hover information if needed.
#[derive(Debug, Default)]
pub struct StringLabels {
    /// Key:
    /// Really just a range. In the format of (startLine, startChar, endLine, endChar).
    /// But, Range doesn't implement Hash, so this is easier.
    /// Value:
    /// The string being labeled and its type.
    map: HashMap<(u32, u32, u32, u32), (String, Type)>,
}

impl StringLabels {
    pub fn new() -> Self {
        Self::default()
    }

    /// Identifies a string in the file with a type. Can re-identify the same
    /// string later with a different type, no problem.
    pub fn identify_label(&mut self, range: &Range, string: String, typ: Type) {
        self.map.insert(
            (
                range.start.line,
                range.start.character,
                range.end.line,
                range.end.character,
            ),
            (string, typ),
        );
    }

    /// Gets the information for the label at a certain position, if a label
    /// exists there.
    pub fn get_label_info(&self, position: &Position) -> Option<(Range, String, Type)> {
        self.map.iter().find_map(|elt| {
            let range = elt.0;
            if range.0 <= position.line
                && range.2 >= position.line
                && range.1 <= position.character
                && range.3 > position.character
            {
                Some((
                    Range::new(
                        Position::new(range.0, range.1),
                        Position::new(range.2, range.3),
                    ),
                    elt.1.0.clone(),
                    elt.1.1.clone(),
                ))
            } else {
                None
            }
        })
    }

    /// Gets all label information available within the provided range.
    ///
    /// Useful because the LSP wants this information in order to display it to
    /// the user.
    pub fn get_all_labels_in_range(&self, range: &Range) -> Vec<(Range, String, Type)> {
        self.map
            .iter()
            .filter(|elt| {
                (elt.0.0 > range.start.line
                    || elt.0.0 == range.start.line && elt.0.1 >= range.start.character)
                    && (elt.0.2 < range.end.line
                        || elt.0.2 == range.end.line && elt.0.3 < range.end.character)
            })
            .map(|elt| {
                (
                    Range::new(
                        Position::new(elt.0.0, elt.0.1),
                        Position::new(elt.0.2, elt.0.3),
                    ),
                    elt.1.0.clone(),
                    elt.1.1.clone(),
                )
            })
            .collect()
    }
}

/// Performs semantic analysis on an entire parsed Amaro file.
///
/// Validates block structure, required fields, and type correctness.
/// Returns diagnostics for LSP clients.
pub fn check_semantics(file: &AmaroFile) -> SemanticResult {
    let mut diagnostics = Vec::new();

    let mut found_blocks: HashMap<BlockName, Range> = HashMap::new();

    let user_def_table = UserDefTable::new(file);
    let mut type_map = TypeMap::new();
    let mut string_labels = StringLabels::new();

    // Block Level Validation
    for block in &file.blocks {
        let block_name = block.kind.as_str();

        let block_name = match BlockName::from_string(block_name) {
            None => {
                // show diagnostic error that this is not a valid block name
                diagnostics.push(Diagnostic {
                    range: block.range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("Semantic".to_string()),
                    message: format!(
                        "Block name '{}' is invalid. Valid block names are as follows: {:?}",
                        block_name,
                        BlockName::get_all_blocks()
                    ),
                    ..Default::default()
                });

                // dont look at anything else in this block
                continue;
            }
            Some(block_name) => block_name,
        };

        // 1. Check for existence of blocks and uniqueness
        if let Some(first_range) = found_blocks.insert(block_name, block.range) {
            diagnostics.push(Diagnostic {
                range: block.range,
                severity: Some(DiagnosticSeverity::ERROR),
                message: format!(
                    "Duplicate definition of '{}' block.",
                    block_name.to_string()
                ),

                related_information: Some(vec![DiagnosticRelatedInformation {
                        location: Location {
                            uri: Url::parse("file:///previous/definition") // ??? is this a valid URL? 
                            // this seems like maybe needs to be replaced with something valid
                                .unwrap_or_else(|_| Url::parse("file:///unknown").unwrap()), // ?????
                            range: first_range,
                        },
                        message: "First defined here".to_string(),
                    }]),
                ..Default::default()
            });
        }

        // 2. Type Check all fields
        let mut sym_table = SymbolTable::new();

        let mut present_keys: Vec<&str> = Vec::new();
        let BlockContent::Fields(items) = &block.content;
        for item in items {
            if let BlockItem::ReturnKeyword { range, key } = item {
                // The field was parsed but its value started with `return`, which is
                // not valid in expression context. Emit a targeted warning and mark
                // the field as present so "missing required field" is not also raised.
                present_keys.push(key.as_str());
                diagnostics.push(Diagnostic {
                    range: *range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    message: format!(
                        "'return' is not valid in field expression context (field '{}').\n\
                         Amaro uses functional style — remove 'return' and write the expression directly.",
                        key
                    ),
                    ..Default::default()
                });
                continue;
            }

            if let BlockItem::Field(field) = item {
                present_keys.push(field.key.as_str());

                // 3.1. Gate Validation in 'routed_gates' fields
                // no longer necessary, should check field types below
                // if block_name == "RouteInfo" && field.key == "routed_gates" {
                //     validate_gates(&field.value, &mut diagnostics);
                // }

                let mut generic_table: GenericTable = GenericTable::new();

                let mut inf_data = InferenceData {
                    sym_table: &mut sym_table,
                    diagnostics: &mut Vec::new(), // do this bc it will be messy in register_field
                    type_map: &mut type_map,
                    user_def_table: &user_def_table,
                    generic_table: &mut generic_table,
                    string_labels: &mut string_labels,
                };

                let mut field_type = register_field(&field.value, &mut inf_data);

                diagnostics.append(inf_data.diagnostics); // append the vec here

                // 3.2. Enforce types on all fields
                // Additionally, use the field type to help inform the expression type.

                // get the expected type of the field based off the field name
                if let Some(field_info) = fields::field_lookup(block_name, &field.key) {
                    // so, we found the type for the field. destructure to
                    // function type since all fields should be this
                    if let Type::Function { return_type, .. } = &field_info.typ {
                        // need that the return type is compatible with the provided
                        if field_type != Type::Unknown {
                            // first, try to overlay the types

                            if overlay_type(&mut field_type, return_type) {
                                // need to go and retype
                                retype(&field.value, field_type.clone(), &mut inf_data);
                            }

                            // SPECIAL EXCEPTION: Put in the case that single elements
                            // can be autocast to a vec.
                            // This is like when we have just 'routed_gates = CX'.
                            // routed_gates expects a list, but with one element, the
                            // parser doesn't understand that the RHS is a list.
                            let mut compatible_flag: bool = false;
                            if let Type::Vec(inner) = &(**return_type) {
                                // wow look at this line haha
                                if types_compatible(inner, &field_type) {
                                    compatible_flag = true;
                                }
                            }

                            if !compatible_flag && types_compatible(return_type, &field_type) {
                                compatible_flag = true;
                            }

                            if !compatible_flag {
                                diagnostics.push(Diagnostic {
                                    range: field.key_range,
                                    severity: Some(DiagnosticSeverity::ERROR),
                                    message: format!(
                                        "Field '{}' in {} must return {}, got '{}'.",
                                        field.key,
                                        block_name.to_string(),
                                        return_type,
                                        field_type
                                    ),
                                    ..Default::default()
                                });
                            }
                        }

                        // add the field to string labels
                        string_labels.identify_label(
                            &field.key_range,
                            field.key.clone(),
                            (**return_type).clone(),
                        );
                    }
                } else {
                    // unknown field
                    // show message to user but don't error, we might just
                    // not have the builtin
                    diagnostics.push(Diagnostic {
                        range: field.key_range,
                        severity: Some(DiagnosticSeverity::INFORMATION),
                        message: format!(
                            "Unknown field '{}' in {}",
                            field.key,
                            block_name.to_string()
                        ),
                        ..Default::default()
                    });
                }
            }
        }

        // ensure block has all the mandatory fields
        fields::get_all_fields_under(block_name) // get all fields for this block
            .filter(|elt| elt.mandatory_if_block_present) // filter to just mandatory
            .map(|elt| &elt.field_name) // map to just the name of the mandatory
            .filter(|name| !present_keys.contains(&name.as_str())) // filter to lacking
            .for_each(|name| {
                diagnostics.push(Diagnostic {
                    range: block.range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("Semantic".to_string()),
                    message: format!(
                        "Block '{}' is missing required field: '{}'",
                        block_name.to_string(),
                        name
                    ),
                    ..Default::default()
                });
            });
    }

    // 4. Mandatory Blocks Check
    for req_block in BlockName::get_mandatory_blocks() {
        if !found_blocks.contains_key(&req_block) {
            diagnostics.push(Diagnostic {
                range: Range::default(),
                severity: Some(DiagnosticSeverity::ERROR),
                message: format!("Missing mandatory block: '{}'.", req_block.to_string()),
                ..Default::default()
            });
        }
    }

    SemanticResult {
        diagnostics,
        type_map: type_map.map,
        user_def_table,
        string_labels,
    }
}

// Validates that gate identifiers are recognized gate types (CX, T, Pauli, PauliMeasurement).
// fn validate_gates(expr: &Expr, diagnostics: &mut Vec<Diagnostic>) {
//     let valid_gates = ["CX", "T", "Pauli", "PauliMeasurement"];

//     match &expr.kind {
//         ExprKind::Identifier(name) => {
//             if !valid_gates.contains(&name.as_str()) {
//                 diagnostics.push(Diagnostic {
//                     range: expr.range,
//                     severity: Some(DiagnosticSeverity::WARNING),
//                     message: format!(
//                         "'{}' is not a recognized standard gate. Expected one of: {}",
//                         name,
//                         valid_gates.join(", ")
//                     ),
//                     ..Default::default()
//                 });
//             }
//         }
//         ExprKind::List(items) | ExprKind::Tuple(items) => {
//             for item in items {
//                 validate_gates(item, diagnostics);
//             }
//         }
//         _ => {}
//     }
// }

/// Given the background_type and foreground_type are correspondent,
/// replaces unknowns in the background type with the values from the foreground type.
///
/// Returns true if anything was changed in the background type, false otherwise.
pub fn overlay_unknowns(background_type: &mut Type, foreground_type: &Type) -> bool {
    match background_type {
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
        | Type::Generic(_) => false,
        Type::Vec(b_inner) => {
            if let Type::Vec(f_inner) = foreground_type {
                overlay_type(b_inner, f_inner)
            } else {
                false
            }
        }
        Type::Tuple(b_items) => {
            if let Type::Tuple(f_items) = foreground_type {
                b_items
                    .iter_mut()
                    .zip(f_items.iter())
                    .fold(false, |acc, elt| overlay_type(elt.0, elt.1) || acc) // NOTE: order of overlay_type before acc in || is important so it is always ran
            } else {
                false
            }
        }
        Type::Option(b_inner) => {
            if let Type::Option(f_inner) = foreground_type {
                overlay_type(b_inner, f_inner)
            } else {
                false
            }
        }
        Type::Function {
            params: b_params,
            return_type: b_ret,
        } => {
            if let Type::Function {
                params: f_params,
                return_type: f_ret,
            } = foreground_type
            {
                let res1 = overlay_type(b_ret, f_ret);

                let res2 = b_params
                    .iter_mut()
                    .zip(f_params.iter())
                    .fold(false, |acc, elt| overlay_type(elt.0, elt.1) || acc); // NOTE: order of overlay_type before acc in || is important so it is always ran

                res1 || res2 // have to do it like this to make sure they both get executed
            } else {
                false
            }
        }

        Type::Unknown => {
            if !matches!(foreground_type, Type::Unknown) {
                *background_type = foreground_type.clone();
                true
            } else {
                false
            }
        }
    }
}

/// Overlays one type onto another.
/// When we "overlay" a type, we are providing more specific information about
/// the background type. If the background type contains a generic or unknown
/// in the same place that the foreground type contains something real, then
/// the background type has its values replaced with those in the foreground
/// type, so that we produce a strictly "finer" type.
///
/// Returns true if any modification was made to the background type.
pub fn overlay_type(background_type: &mut Type, foreground_type: &Type) -> bool {
    match background_type {
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
        Type::Vec(b_inner) => {
            if let Type::Vec(f_inner) = foreground_type {
                overlay_type(b_inner, f_inner)
            } else {
                false
            }
        }
        Type::Tuple(b_items) => {
            if let Type::Tuple(f_items) = foreground_type {
                b_items
                    .iter_mut()
                    .zip(f_items.iter())
                    .fold(false, |acc, elt| overlay_type(elt.0, elt.1) || acc) // NOTE: order of overlay_type before acc in || is important so it is always ran
            } else {
                false
            }
        }
        Type::Option(b_inner) => {
            if let Type::Option(f_inner) = foreground_type {
                overlay_type(b_inner, f_inner)
            } else {
                false
            }
        }
        Type::Function {
            params: b_params,
            return_type: b_ret,
        } => {
            if let Type::Function {
                params: f_params,
                return_type: f_ret,
            } = foreground_type
            {
                let res1 = overlay_type(b_ret, f_ret);

                let res2 = b_params
                    .iter_mut()
                    .zip(f_params.iter())
                    .fold(false, |acc, elt| overlay_type(elt.0, elt.1) || acc); // NOTE: order of overlay_type before acc in || is important so it is always ran

                res1 || res2 // have to do it like this to make sure they both get executed
            } else {
                false
            }
        }
        Type::Generic(_) => {
            if let Type::Generic(_) = foreground_type {
                false
            } else if !matches!(foreground_type, Type::Unknown) {
                *background_type = foreground_type.clone();
                true
            } else {
                false
            }
        }
        Type::Unknown => {
            if !matches!(foreground_type, Type::Unknown) {
                *background_type = foreground_type.clone();
                true
            } else {
                false
            }
        }
    }
}

#[derive(Default)]
pub struct FetchAndAdd<T>
where
    T: Add<Output = T> + Default + From<u8> + Copy,
{
    value: T,
}

impl<T> FetchAndAdd<T>
where
    T: Add<Output = T> + Default + From<u8> + Copy,
{
    pub fn new() -> Self {
        Self::default()
    }
    pub fn fetch_and_add(&mut self) -> T {
        let stored = self.value;
        self.value = self.value + T::from(1u8);
        stored
    }
}

/// A GenericTable tracks generic-relevant information for as long as it lives.
/// It should NOT live for the duration of the file semantic checking.
/// Rather, whenever we want to infer the type of a field's expression, we make
/// ONE GenericTable for usage during the semantic analysis of that single
/// field's expression.
pub struct GenericTable {
    /// The next generic number to assign to a newfound generic.
    pub next_generic_num: FetchAndAdd<u8>,
    /// Mappings from generic numbers to types.
    map: HashMap<u8, Type>,
    /// Set of generics we are currently viewing. Useful in recursive operations
    /// to avoid infinite loops.
    /// I can hypothesize a scenario where we might revisit the same generic and
    /// it is not an infinite loop, but I'm not sure if it is is feasible in Amaro.
    viewing_set: HashSet<u8>,

    /// Set by this whenever the generic map updates.
    /// Can be reset by the user.
    dirty_flag: bool,
}

impl GenericTable {
    pub fn new() -> Self {
        Self {
            next_generic_num: FetchAndAdd::new(),
            map: HashMap::new(),
            viewing_set: HashSet::new(),
            dirty_flag: false,
        }
    }

    pub fn reset_dirty_flag(&mut self) {
        self.dirty_flag = false;
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty_flag
    }

    pub fn next_generic(&mut self) -> u8 {
        self.next_generic_num.fetch_and_add()
    }

    /// Assigns a type to a generic number.
    /// Gives Ok if the generic number did not have an assigned type, or if we
    /// are trying to assign the same type to the generic number.
    /// Errors if we try to overwrite a different type, or also errors if we
    /// try to assign a generic type.
    /// Unknown types are not assigned and are just ignored with an Ok.
    pub fn assign(&mut self, generic_num: u8, typ: Type) -> Result<(), Type> {
        if let Type::Generic(c) = typ
            && c <= generic_num
        {
            return Err(typ);
        }
        if matches!(typ, Type::Unknown) {
            return Ok(());
        }
        match self.map.get(&generic_num).cloned() {
            Some(existing_type) => {
                // need to check if types can be overlayed or something?
                if self.viewing_set.contains(&generic_num) {
                    Err(existing_type)
                } else if existing_type != typ {
                    self.viewing_set.insert(generic_num);
                    let res = self.find_generics_in_correspondent_types(&existing_type, &typ);
                    self.viewing_set.remove(&generic_num);
                    match res {
                        Ok(_) => Ok(()),
                        Err(_) => Err(existing_type),
                    }
                } else {
                    Ok(())
                }
            }
            None => {
                self.map.insert(generic_num, typ);
                self.dirty_flag = true;
                Ok(())
            }
        }
    }

    /// Gets the type referred to by the generic.
    pub fn get(&self, generic_num: &u8) -> Option<&Type> {
        self.map.get(generic_num)
    }

    /// Replaces generic types within the given type with their assigned values
    pub fn try_degenerisize(&self, t1: &Type) -> Type {
        match t1 {
            Type::Generic(a) => {
                match self.get(a) {
                    Some(t) => t.clone(),
                    None => t1.clone(), // no change
                }
            }
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
            | Type::Unknown => t1.clone(),
            Type::Vec(inner) => Type::Vec(Box::new(self.try_degenerisize(inner))),
            Type::Tuple(items) => {
                Type::Tuple(items.iter().map(|f| self.try_degenerisize(f)).collect())
            }
            Type::Option(inner) => Type::Option(Box::new(self.try_degenerisize(inner))),
            Type::Function {
                params,
                return_type,
            } => Type::Function {
                params: params.iter().map(|f| self.try_degenerisize(f)).collect(),
                return_type: Box::new(self.try_degenerisize(return_type)),
            },
        }
    }

    /// Resolves generics that reference other generics within the table.
    /// Uses measures to avoid infinite loops, though it feels impossible
    /// to encounter one in practice.
    pub fn tighten_up(&mut self) -> bool {
        self.viewing_set = HashSet::new();
        let assignments_vec: Vec<(u8, Type)> = self
            .map
            .iter()
            .map(|pair| (*pair.0, pair.1.clone()))
            .collect();
        for pair in assignments_vec {
            if !self.viewing_set.insert(pair.0) {
                self.viewing_set = HashSet::new();
                return false; // infinite loop, backing out.
            }
            let new_type = self.try_degenerisize(&pair.1);
            if new_type != pair.1 {
                self.map.insert(pair.0, new_type);
                self.dirty_flag = true;
            }
        }

        true
    }

    /// Two types are called "correspondent" if they are supposed to correspond to
    /// the same thing. Notice that sometimes correspondent types might be disequal
    /// due to generics and unknowns.
    ///
    /// Identifies the generics.
    ///
    /// Errors are emitted in the string vec if the types have some inherent incompatability.
    pub fn find_generics_in_correspondent_types(
        &mut self,
        t1: &Type,
        t2: &Type,
    ) -> Result<(), Vec<String>> {
        match (t1, t2) {
            // both are generic
            (Type::Generic(a), Type::Generic(b)) => {
                // if they're different types, we can make an association btwn them
                // higher to lower
                if a != b {
                    let smaller = *a.min(b);
                    let larger = *a.max(b);
                    self.assign(smaller, Type::Generic(larger)).map_err(|elt| {
                        vec![format!(
                            "Tried to assign type {} to generic {} when already had {}.",
                            Type::Generic(*a.max(b)),
                            Type::Generic(*a.min(b)),
                            elt
                        )]
                    })
                } else {
                    Ok(())
                }
            }
            // first is generic
            (Type::Generic(a), _) => {
                // then, assign t2 onto generic a
                self.assign(*a, t2.clone()).map_err(|elt| {
                    vec![format!(
                        "Tried to assign type {} to generic {} when already had {}.",
                        t2, t1, elt
                    )]
                })
            }
            // second is generic
            (_, Type::Generic(a)) => {
                // then, assign t2 onto generic a
                self.assign(*a, t1.clone()).map_err(|elt| {
                    vec![format!(
                        "Tried to assign type {} to generic {} when already had {}.",
                        t1, t2, elt
                    )]
                })
            }
            // first is unknown
            (Type::Unknown, _) => Ok(()),
            // second is unknown
            (_, Type::Unknown) => Ok(()),
            (Type::Vec(a_inner), Type::Vec(b_inner)) => {
                self.find_generics_in_correspondent_types(a_inner, b_inner)
            }
            (Type::Option(a_inner), Type::Option(b_inner)) => {
                self.find_generics_in_correspondent_types(a_inner, b_inner)
            }
            (Type::Tuple(a_items), Type::Tuple(b_items)) => {
                let mut issues: Option<Vec<String>> = None;
                if a_items.len() != b_items.len() {
                    let issue_message = format!(
                        "Tuples don't have matching size: {} and {}",
                        a_items.len(),
                        b_items.len()
                    );
                    match issues.as_mut() {
                        None => issues = Some(vec![issue_message]),
                        Some(vec) => vec.push(issue_message),
                    }
                }
                a_items.iter().zip(b_items.iter()).for_each(|pair| {
                    if let Err(elt) = self.find_generics_in_correspondent_types(pair.0, pair.1) {
                        match issues.as_mut() {
                            None => issues = Some(elt),
                            Some(vec) => vec.extend(elt),
                        }
                    }
                });
                match issues {
                    None => Ok(()),
                    Some(vec) => Err(vec),
                }
            }
            (
                Type::Function {
                    params: a_params,
                    return_type: a_return_type,
                },
                Type::Function {
                    params: b_params,
                    return_type: b_return_type,
                },
            ) => {
                let mut issues: Option<Vec<String>> = None;
                if a_params.len() != b_params.len() {
                    let issue_message = format!(
                        "Tuples don't have matching size: {} and {}",
                        a_params.len(),
                        b_params.len()
                    );
                    match issues.as_mut() {
                        None => issues = Some(vec![issue_message]),
                        Some(vec) => vec.push(issue_message),
                    }
                }
                a_params.iter().zip(b_params.iter()).for_each(|pair| {
                    if let Err(elt) = self.find_generics_in_correspondent_types(pair.0, pair.1) {
                        match issues.as_mut() {
                            None => issues = Some(elt),
                            Some(vec) => vec.extend(elt),
                        }
                    }
                });

                if let Err(elt) =
                    self.find_generics_in_correspondent_types(a_return_type, b_return_type)
                {
                    match issues.as_mut() {
                        None => issues = Some(elt),
                        Some(vec) => vec.extend(elt),
                    }
                }

                match issues {
                    None => Ok(()),
                    Some(vec) => Err(vec),
                }
            }
            _ => {
                if *t1 != *t2 {
                    Err(vec![format!(
                        "Cannot find generic types in {} and {}, types don't match.",
                        *t1, *t2
                    )])
                } else {
                    Ok(())
                }
            }
        }
    }
}

impl Default for GenericTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Aggregate of the args passed to infer_expr_type,
/// so we can easily change these without having to change 10000 call signatures
pub struct InferenceData<'a> {
    pub sym_table: &'a mut SymbolTable,
    pub diagnostics: &'a mut Vec<Diagnostic>,
    pub type_map: &'a mut TypeMap,
    pub user_def_table: &'a UserDefTable,
    pub generic_table: &'a mut GenericTable,
    pub string_labels: &'a mut StringLabels,
}

/// Precondition: Expr and all subexpressions have a type (even if unknown) in
/// the type map.
/// Recursively descends into the expression, replacing generic types wherever
/// found.
/// Best to use this on the field's expression
pub fn apply_generic_info(expr: &Expr, inference_data: &mut InferenceData) {
    let original_type = inference_data.type_map.get(&expr.id).unwrap();
    let new_type = inference_data.generic_table.try_degenerisize(original_type);
    inference_data.type_map.set(expr.id, new_type); // fine to do set instead of overlay,
    // because set is less expensive and the outcome will be the same

    // now, recursively descend.
    match &expr.kind {
        ExprKind::Identifier(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_) => {
            // terminals, do nothing
        }
        ExprKind::List(exprs) => exprs
            .iter()
            .for_each(|elt| apply_generic_info(elt, inference_data)),
        ExprKind::Tuple(exprs) => exprs
            .iter()
            .for_each(|elt| apply_generic_info(elt, inference_data)),
        ExprKind::StructLiteral { fields, .. } => fields
            .iter()
            .for_each(|elt| apply_generic_info(&elt.1, inference_data)),
        ExprKind::FunctionCall { function, args } => {
            apply_generic_info(function, inference_data);
            args.iter()
                .for_each(|elt| apply_generic_info(elt, inference_data));
        }
        ExprKind::FieldAccess { object, .. } => apply_generic_info(object, inference_data),
        ExprKind::IndexAccess { object, index } => {
            apply_generic_info(object, inference_data);
            apply_generic_info(index, inference_data);
        }
        ExprKind::Lambda { body, .. } => {
            apply_generic_info(body, inference_data);
        }
        ExprKind::IfThenElse {
            condition,
            then_branch,
            else_branch,
        } => {
            apply_generic_info(condition, inference_data);
            apply_generic_info(then_branch, inference_data);
            apply_generic_info(else_branch, inference_data);
        }
        ExprKind::LetBinding { value, body, .. } => {
            apply_generic_info(value, inference_data);
            apply_generic_info(body, inference_data);
        }
        ExprKind::BinaryOp { left, right, .. } => {
            apply_generic_info(left, inference_data);
            apply_generic_info(right, inference_data);
        }
        ExprKind::UnaryOp { operand, .. } => {
            apply_generic_info(operand, inference_data);
        }
        ExprKind::Some(expr) => apply_generic_info(expr, inference_data),
        ExprKind::None => {}
        ExprKind::Match { scrutinee, arms } => {
            apply_generic_info(scrutinee, inference_data);
            arms.iter().for_each(|elt| {
                apply_generic_info(&elt.body, inference_data);
            });
        }
        ExprKind::TensorProduct { left, right } => {
            apply_generic_info(left, inference_data);
            apply_generic_info(right, inference_data);
        }
        ExprKind::Projection { tuple, .. } => {
            apply_generic_info(tuple, inference_data);
        }
    }
}

/// Given a field, registers all its info. Recursively and repeatedly does type
/// inference until there is nothing else to gather.
///
/// This is needed because generics mean we might have to traverse the same parts
/// multiple times.
pub fn register_field(field: &Expr, inference_data: &mut InferenceData) -> Type {
    inference_data.generic_table.reset_dirty_flag();
    // first, infer
    infer_expr_type(field, inference_data);

    let mut num_loops = 0;

    while inference_data.generic_table.is_dirty() {
        inference_data.diagnostics.clear();
        if num_loops > 30 {
            // very likely something has gone horribly wrong.
            // this should never happen, but it's good to have this here anyway.
            inference_data.diagnostics.push(Diagnostic {
                range: field.range,
                severity: Some(DiagnosticSeverity::ERROR),
                message:
                    "Performing semantic checking on this field resulted in an infinite loop due to generics resolution."
                        .to_string(),
                ..Default::default()
            });
            break;
        }
        inference_data.generic_table.reset_dirty_flag();
        // make table nicer, meaning deals with generics that reference others
        inference_data.generic_table.tighten_up();

        apply_generic_info(field, inference_data);
        // then, try again with inference
        infer_expr_type(field, inference_data);
        num_loops += 1;
    }
    // eventually, the generic table should stop being dirty. if not, then we
    // are always making progress to removing generics.

    inference_data.type_map.get(&field.id).unwrap().clone() // always present
}

/// Infers the type of an expression using the current symbol table.
/// (Type Inference Engine)
/// Recursively walks the AST and emits type errors for incompatibilities.
/// Uses `Unknown` for leniency to avoid false positives.
pub fn infer_expr_type(expr: &Expr, inference_data: &mut InferenceData) -> Type {
    // generics can only be introduced through built-ins.
    // first, check if already resolved.
    // an expression is resolved if it contains no generics or unknowns in its
    // type, or in the types of any subexpressions
    // resolved expressions can just use the type map and don't need to redo
    // inference.
    if inference_data.type_map.is_resolved(&expr.id) {
        return inference_data.type_map.get(&expr.id).unwrap().clone();
    }

    // otherwise, need to resolve it

    // found_type -> type to assign to this expr
    // resolved -> determiner for whether the expr will be marked as resolved
    let (found_type, resolved): (Type, bool) = match &expr.kind {
        ExprKind::IntLiteral(_) => (Type::Int, true),
        ExprKind::FloatLiteral(_) => (Type::Float, true),
        ExprKind::BoolLiteral(_) => (Type::Bool, true),
        ExprKind::StringLiteral(_) => (Type::String, true),
        ExprKind::None => (
            Type::Option(Box::new(Type::Generic(
                inference_data.generic_table.next_generic(),
            ))),
            false,
        ),
        ExprKind::Identifier(name) => {
            // always first look up in symbol table
            let resultant_type = inference_data
                .sym_table
                .lookup(name)
                .cloned()
                .unwrap_or_else(|| {
                    // if not in symbol table, check if we already have given this
                    // expression a type. if we have, then don't recalculate at all.
                    // we need to do this because we don't want to re-pull from the
                    // built-ins every time, since generic resolution takes multiple
                    // passes.
                    inference_data
                        .type_map
                        .get(&expr.id)
                        .cloned()
                        .unwrap_or_else(|| {
                            // no type found, so check built-ins
                            builtins::get_raw_built_in(name.as_str())
                                .map(|elt| {
                                    // shift the generics so they are unique
                                    let mut new_type = elt.typ.clone();
                                    new_type.make_generics_unique(
                                        &mut inference_data.generic_table.next_generic_num,
                                    );
                                    new_type
                                })
                                .unwrap_or_else(|| {
                                    // not in built ins, so check if userdef struct
                                    match inference_data.user_def_table.get_fields(name) {
                                        Some(_) => Type::UserDef(name.clone()),
                                        None => {
                                            inference_data.diagnostics.push(Diagnostic {
                                                range: expr.range,
                                                severity: Some(DiagnosticSeverity::ERROR),
                                                message: format!("Undefined variable '{}'.", name),
                                                ..Default::default()
                                            });
                                            // not anything
                                            Type::Unknown
                                        }
                                    }
                                })
                        })
                });

            let resolved = !resultant_type.contains_generic_or_unknown();
            (resultant_type, resolved)
        }
        ExprKind::List(items) => {
            if items.is_empty() {
                (
                    Type::Vec(Box::new(Type::Generic(
                        inference_data.generic_table.next_generic(),
                    ))),
                    false,
                )
            } else {
                let first_type = infer_expr_type(&items[0], inference_data);
                let problems: Vec<(Range, Type)> = items[1..]
                    .iter()
                    .filter_map(|item| {
                        let item_type = infer_expr_type(item, inference_data);

                        if item_type != first_type {
                            Some((item.range, item_type))
                        } else {
                            None
                        }
                    })
                    .collect();

                problems.iter().for_each(|elt| {
                    inference_data.diagnostics.push(Diagnostic {
                        range: elt.0,
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: format!("Inconsistent types in list literal. Had {}", elt.1),
                        ..Default::default()
                    });
                });

                if !problems.is_empty() {
                    (Type::Vec(Box::new(Type::Unknown)), false)
                } else {
                    (
                        Type::Vec(Box::new(first_type)),
                        inference_data.type_map.is_resolved(&items[0].id),
                    )
                }
            }
        }
        ExprKind::Tuple(items) => {
            let new_inner_type = Type::Tuple(
                items
                    .iter()
                    .map(|e| infer_expr_type(e, inference_data))
                    .collect(),
            );

            let resolved = items
                .iter()
                .all(|elt| inference_data.type_map.is_resolved(&elt.id));
            (new_inner_type, resolved)
        }
        ExprKind::Some(inner) => {
            let inner_type = infer_expr_type(inner, inference_data);
            let inner_resolved = inference_data.type_map.is_resolved(&inner.id);
            (Type::Option(Box::new(inner_type)), inner_resolved)
        }
        ExprKind::Lambda { params, body } => {
            // with lambda:
            // sometimes we will be rerunning this function to determine generics
            // if we do, then we provide some information about the lambda and use this
            // to gain additional information about the lambda.

            inference_data.sym_table.enter_scope();
            let mut param_types = Vec::new();

            let mut resolved: bool;

            if let Some(Type::Function {
                params: fcn_params, ..
            }) = inference_data.type_map.get(&expr.id)
            {
                // If a type exists, use this.
                for (param_name, param_type) in params.iter().zip(fcn_params) {
                    inference_data
                        .sym_table
                        .bind(param_name.string.clone(), param_type.clone());
                    inference_data.string_labels.identify_label(
                        &param_name.range,
                        param_name.string.clone(),
                        param_type.clone(),
                    );
                    param_types.push(param_type.clone());
                }

                // resolved is true if each param does not contain generics and does not contain unknowns.
                resolved = fcn_params
                    .iter()
                    .all(|elt| !elt.contains_generic_or_unknown());
            } else {
                // If no type exists, then we just put unknown for all of them.
                for param in params {
                    inference_data
                        .sym_table
                        .bind(param.string.clone(), Type::Unknown);
                    inference_data.string_labels.identify_label(
                        &param.range,
                        param.string.clone(),
                        Type::Unknown,
                    );
                    param_types.push(Type::Unknown);
                }

                resolved = params.is_empty(); // only can be resolved if there
                // are no params, cus all params default to unknown here
            }

            let return_type = infer_expr_type(body, inference_data);
            resolved &= inference_data.type_map.is_resolved(&body.id);
            inference_data.sym_table.exit_scope();

            (
                Type::Function {
                    params: param_types,
                    return_type: Box::new(return_type),
                },
                resolved,
            )
        }
        ExprKind::LetBinding { name, value, body } => {
            inference_data.sym_table.enter_scope();
            let value_type = infer_expr_type(value, inference_data);
            inference_data
                .sym_table
                .bind(name.string.clone(), value_type.clone());
            inference_data.string_labels.identify_label(
                &name.range,
                name.string.clone(),
                value_type,
            );
            let body_type = infer_expr_type(body, inference_data);
            inference_data.sym_table.exit_scope();

            let resolved = inference_data.type_map.is_resolved(&value.id)
                && inference_data.type_map.is_resolved(&body.id);
            (body_type, resolved)
        }
        ExprKind::IfThenElse {
            condition,
            then_branch,
            else_branch,
        } => {
            let cond_type = infer_expr_type(condition, inference_data);
            if !types_compatible(&cond_type, &Type::Bool) {
                inference_data.diagnostics.push(Diagnostic {
                    range: condition.range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: "Condition in if-then-else must be of type 'Bool'.".to_string(),
                    ..Default::default()
                });
            }

            infer_expr_type(then_branch, inference_data);
            infer_expr_type(else_branch, inference_data);
            let mut then_type = inference_data
                .type_map
                .get(&then_branch.id)
                .unwrap()
                .clone();
            let mut else_type = inference_data
                .type_map
                .get(&else_branch.id)
                .unwrap()
                .clone();

            // TODO this generally works, but risks losing information with the
            // overlay, as we might accidentally remove generic info.
            // TODO uncomment out the line below, and test this, instead of doing any overlay stuff.
            // additionally, change overlay_type to overlay_unknowns, not generics. can overlay generics later.
            // inference_data.generic_table.find_generics_in_correspondent_types(&then_type, &else_type);
            let overlayed_dir_1 = overlay_unknowns(&mut then_type, &else_type);
            let overlayed_dir_2 = overlay_unknowns(&mut else_type, &then_type);
            match inference_data
                .generic_table
                .find_generics_in_correspondent_types(&then_type, &else_type)
            {
                Ok(_) => {}
                Err(incompatibilities) => {
                    inference_data.diagnostics.push(Diagnostic {
                            range: expr.range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            message: format!("Then and else branches of if-then-else must have compatible types. Then: {}, Else: {}. There were this incompatibilities: {:?}", then_type, else_type, incompatibilities),
                            ..Default::default()
                        });
                }
            }

            if overlayed_dir_1 {
                retype(then_branch, then_type.clone(), inference_data);
            }
            if overlayed_dir_2 {
                retype(else_branch, else_type.clone(), inference_data);
            }

            if overlayed_dir_1 || overlayed_dir_2 {
                // we need to rerun inference, because things have changed on this expr
                return infer_expr_type(expr, inference_data);
            } else {
                if !types_compatible(&then_type, &else_type) {
                    inference_data.diagnostics.push(Diagnostic {
                        range: expr.range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: format!("Then and else branches of if-then-else must have compatible types. Then: {}, Else: {}", then_type, else_type),
                        ..Default::default()
                    });
                }
                let resolved = inference_data.type_map.is_resolved(&condition.id)
                    && inference_data.type_map.is_resolved(&then_branch.id)
                    && inference_data.type_map.is_resolved(&else_branch.id);
                (then_type, resolved)
            }
        }
        ExprKind::FunctionCall { function, args } => {
            let fcn_type = infer_expr_type(function, inference_data);

            match &fcn_type {
                Type::Unknown => {
                    args.iter().for_each(|elt| {
                        infer_expr_type(elt, inference_data);
                    }); // still need to do this to assign them to something
                    (Type::Unknown, false)
                }
                Type::Function {
                    params,
                    return_type,
                } => {
                    if params.len() != args.len() {
                        inference_data.diagnostics.push(Diagnostic {
                            range: expr.range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            message: format!(
                                "Expected {} arguments but got {}.",
                                params.len(),
                                args.len()
                            ),
                            ..Default::default()
                        });
                        args.iter().for_each(|elt| {
                            infer_expr_type(elt, inference_data);
                        }); // still need to do this to assign them to something
                        ((**return_type).clone(), false)
                    } else if fcn_type.contains_generic() {
                        // need to do some generic stuff

                        // 1. get inferred types of all args and return type
                        // 2. using inferred types, try to build generic map as best as can
                        // 3. retype all the expressions using generic map as best as can
                        // 4. if still contains generic type, return to 1

                        // TODOGEN

                        let mut inferred_args: Vec<(&Expr, Type)> = args
                            .iter()
                            .map(|elt| (elt, infer_expr_type(elt, inference_data)))
                            .collect();
                        let outcome = inferred_args
                            .iter_mut()
                            .zip(params)
                            .map(|(actual_arg, generic_param)| {
                                // overlay the unknowns
                                if overlay_unknowns(&mut actual_arg.1, generic_param) {
                                    retype(actual_arg.0, actual_arg.1.clone(), inference_data);
                                }

                                inference_data
                                    .generic_table
                                    .find_generics_in_correspondent_types(
                                        generic_param,
                                        &actual_arg.1,
                                    )
                                // infer_generic_type(generic_param, actual_arg, &mut generic_map)
                            })
                            .collect::<Result<Vec<_>, _>>();

                        if outcome.is_err() {
                            // don't output error for generic failures

                            // inference_data.diagnostics.push(Diagnostic {
                            //     range: expr.range,
                            //     severity: Some(DiagnosticSeverity::ERROR),
                            //     message: format!(
                            //         "Issue when inferring generics of function call: {}",
                            //         t
                            //     ),
                            //     ..Default::default()
                            // });
                            inference_data.type_map.overlay(expr.id, &Type::Unknown);
                            return Type::Unknown;
                        } else {
                            ((**return_type).clone(), false)
                        }

                        // if generic_map.is_empty() {
                        //     // cannot resolve any more generics
                        //     // end normally
                        //     (**return_type).clone()
                        // } else {
                        //     // degenerisize now
                        //     for (param_type, arg) in params.iter().zip(args) {
                        //         let (_, t) = degenerisize(param_type, &generic_map);
                        //         retype(arg, t, inference_data);
                        //     }

                        //     // degenerisize whole function
                        //     let (_, t) = degenerisize(&fcn_type, &generic_map);
                        //     retype(function, t.clone(), inference_data);

                        //     // then, infer again on this expr to see if we're done
                        //     // should be slowly removing generic types i think

                        //     return infer_expr_type(expr, inference_data);
                        // }
                    } else {
                        for (i, (param_type, arg)) in params.iter().zip(args).enumerate() {
                            infer_expr_type(arg, inference_data);

                            // try to retype to the parameter type, if possible
                            // this deals with generic situations.
                            // for example, sps we have the function |Vec<Transition>| -> float
                            // if we pass Vec() into the function, this will determine that
                            // the Vec() is type Vec<Transition>.
                            retype(arg, param_type.clone(), inference_data);

                            let arg_type = inference_data.type_map.get(&arg.id).unwrap();

                            // Following Logic
                            // 1. If param_type Unknown, Accept
                            // 2. If arg_type Unknown, Accept (Avoid Cascading Errors)
                            // 3. Otherwise, Check Compatibility
                            if *param_type != Type::Unknown
                                && *arg_type != Type::Unknown
                                && !types_compatible(param_type, arg_type)
                            {
                                inference_data.diagnostics.push(Diagnostic {
                                    range: arg.range,
                                    severity: Some(DiagnosticSeverity::ERROR),
                                    message: format!(
                                        "Argument {} expected type '{:?}' but got '{:?}'.",
                                        i + 1,
                                        param_type,
                                        arg_type
                                    ),
                                    ..Default::default()
                                });
                            }
                        }

                        let resolved = inference_data.type_map.is_resolved(&function.id)
                            && args
                                .iter()
                                .all(|elt| inference_data.type_map.is_resolved(&elt.id));
                        // TODO check that just bc smth doesnt have generics, doesnt mean
                        // it's fine. could have unknowns
                        ((**return_type).clone(), resolved)
                    }
                }
                _ => {
                    args.iter().for_each(|elt| {
                        infer_expr_type(elt, inference_data);
                    }); // still need to do this to assign them to something
                    inference_data.diagnostics.push(Diagnostic {
                        range: expr.range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: "Could not determine type of function.".to_string(),

                        ..Default::default()
                    });
                    (Type::Unknown, false)
                }
            }
        }
        ExprKind::FieldAccess { object, field } => {
            let obj_type = infer_expr_type(object, inference_data);

            if matches!(obj_type, Type::Unknown) {
                (Type::Unknown, false)
            } else {
                let type_from_user_def: Option<&Type> = if let Type::UserDef(name) = &obj_type {
                    if let Some(fields) = inference_data.user_def_table.get_fields(name.as_str()) {
                        fields.get(field)
                    } else {
                        None
                    }
                } else {
                    None
                };

                let outcome_type = match type_from_user_def {
                    Some(t) => t.clone(),
                    None => match builtins::check_built_in_after_type(&obj_type, field) {
                        Some(builtins::Owner::Owned(built_in)) => built_in.typ.clone(),
                        Some(builtins::Owner::Borrowed(built_in)) => built_in.typ.clone(),
                        None => {
                            inference_data.diagnostics.push(Diagnostic {
                                range: expr.range,
                                severity: Some(DiagnosticSeverity::ERROR),
                                message: format!(
                                    "Field {} cannot be accessed on type {}. Does it exist?",
                                    field, obj_type
                                ),
                                ..Default::default()
                            });
                            Type::Unknown
                        }
                    },
                };

                let resolved = inference_data.type_map.is_resolved(&object.id)
                    && !outcome_type.contains_generic_or_unknown();

                (outcome_type, resolved)
            }
        }
        ExprKind::StructLiteral { name, fields } => {
            // the only valid struct literals are those of UserDef, right?
            // LS: Determine if that is accurate
            for (_, value) in fields {
                infer_expr_type(value, inference_data);
            }

            if inference_data.user_def_table.get_fields(name).is_some() {
                (Type::UserDef(name.clone()), true)
            } else {
                inference_data.diagnostics.push(Diagnostic {
                    range: expr.range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: "This struct literal does not match any defined struct types."
                        .to_string(),
                    ..Default::default()
                });
                (Type::Unknown, false)
            }
        }
        ExprKind::IndexAccess { object, index } => {
            let obj_type = infer_expr_type(object, inference_data);
            let idx_type = infer_expr_type(index, inference_data);

            // Skip validation for Unknown types to avoid false positives.
            // Example: x.implementation.(path()) where x is Unknown.
            if obj_type == Type::Unknown {
                (Type::Unknown, false)
            } else {
                let expected_idx_type = match &obj_type {
                    Type::Vec(_) => Type::Int,
                    Type::QubitMap => Type::Qubit,
                    Type::Function {
                        params,
                        return_type,
                    } if params.is_empty() => match return_type.as_ref() {
                        Type::QubitMap => Type::Qubit,
                        _ => Type::Int,
                    },
                    _ => Type::Int,
                };

                if idx_type != Type::Unknown && !types_compatible(&idx_type, &expected_idx_type) {
                    inference_data.diagnostics.push(Diagnostic {
                        range: index.range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: format!(
                            "Index type mismatch. Expected '{}' but got '{}'.",
                            type_display(&expected_idx_type),
                            type_display(&idx_type)
                        ),
                        ..Default::default()
                    });
                }

                // If object is a 0-arg function, auto-call it
                let actual_type = match obj_type {
                    Type::Function {
                        params,
                        return_type,
                    } if params.is_empty() => *return_type,
                    other => other,
                };

                let output_type = match actual_type {
                    Type::Vec(inner) => *inner,
                    Type::QubitMap => Type::Location,
                    Type::Unknown => Type::Unknown,
                    _ => {
                        inference_data.diagnostics.push(Diagnostic {
                            range: object.range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            message: "Attempted to index a non-indexable type.".to_string(),
                            ..Default::default()
                        });
                        Type::Unknown
                    }
                };

                let resolved = inference_data.type_map.is_resolved(&object.id)
                    && inference_data.type_map.is_resolved(&index.id)
                    && !output_type.contains_generic_or_unknown();

                (output_type, resolved)
            }
        }
        ExprKind::BinaryOp { op, left, right } => {
            let left_type = infer_expr_type(left, inference_data);
            let right_type = infer_expr_type(right, inference_data);

            match op {
                BinaryOperator::Add
                | BinaryOperator::Sub
                | BinaryOperator::Mul
                | BinaryOperator::Div
                | BinaryOperator::Mod => match types_math(&left_type, &right_type) {
                    Some(t) => (t, true),
                    None => {
                        inference_data.diagnostics.push(Diagnostic {
                            range: expr.range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            message: format!(
                                "Cannot use operation '{}' on types {} and {}.",
                                op, left_type, right_type
                            ),
                            ..Default::default()
                        });
                        (Type::Unknown, false)
                    }
                },
                BinaryOperator::Ge
                | BinaryOperator::Le
                | BinaryOperator::Gt
                | BinaryOperator::Lt => match types_math(&left_type, &right_type) {
                    Some(_) => (Type::Bool, true),
                    None => {
                        inference_data.diagnostics.push(Diagnostic {
                            range: expr.range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            message: format!(
                                "Cannot use operation '{}' on types {} and {}.",
                                op, left_type, right_type
                            ),
                            ..Default::default()
                        });
                        (Type::Unknown, false)
                    }
                },
                BinaryOperator::Eq | BinaryOperator::Ne => {
                    match types_comparable(&left_type, &right_type) {
                        true => (Type::Bool, true),
                        false => {
                            inference_data.diagnostics.push(Diagnostic {
                                range: expr.range,
                                severity: Some(DiagnosticSeverity::ERROR),
                                message: format!(
                                    "Cannot use equality on types {} and {}.",
                                    left_type, right_type
                                ),
                                ..Default::default()
                            });
                            (Type::Unknown, false)
                        }
                    }
                }

                BinaryOperator::And | BinaryOperator::Or => match (&left_type, &right_type) {
                    (Type::Unknown | Type::Bool, Type::Bool | Type::Unknown) => (Type::Bool, true),
                    (_, _) => {
                        inference_data.diagnostics.push(Diagnostic {
                            range: expr.range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            message: format!(
                                "Cannot use logical operations on types {} and {}.",
                                left_type, right_type
                            ),
                            ..Default::default()
                        });
                        (Type::Unknown, false)
                    }
                },

                BinaryOperator::Tensor => (Type::Unknown, false),
                BinaryOperator::Range => {
                    if matches!(left_type, Type::Int) && matches!(right_type, Type::Int) {
                        (Type::Vec(Box::new(Type::Int)), true)
                    } else {
                        (Type::Unknown, false)
                    }
                }
            }
        }
        ExprKind::UnaryOp { op, operand } => {
            let operand_type = infer_expr_type(operand, inference_data);
            match op {
                UnaryOperator::Not => match operand_type {
                    Type::Bool => (Type::Bool, true),
                    Type::Unknown => (Type::Unknown, false),
                    _ => {
                        inference_data.diagnostics.push(Diagnostic {
                            range: expr.range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            message: "Cannot perform NOT operation on non-bool type.".to_string(),
                            ..Default::default()
                        });
                        (Type::Unknown, false)
                    }
                },
                UnaryOperator::Neg => match operand_type {
                    Type::Int => (Type::Int, true),
                    Type::Float => (Type::Float, true),
                    Type::Unknown => (Type::Unknown, false),
                    _ => {
                        inference_data.diagnostics.push(Diagnostic {
                            range: expr.range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            message: "Cannot perform NEG operation on non-number type.".to_string(),
                            ..Default::default()
                        });
                        (Type::Unknown, false)
                    }
                },
            }
        }
        ExprKind::TensorProduct { .. } => (Type::Unknown, false), // TODO what to do here?
        ExprKind::Match { scrutinee, arms } => {
            let _scrutinee_type = infer_expr_type(scrutinee, inference_data);

            if arms.is_empty() {
                (Type::Unknown, false)
            } else {
                let first_type = infer_expr_type(&arms[0].body, inference_data);
                for arm in &arms[1..] {
                    let arm_type = infer_expr_type(&arm.body, inference_data);
                    if arm_type != Type::Unknown
                        && first_type != Type::Unknown
                        && !types_compatible(&arm_type, &first_type)
                    {
                        inference_data.diagnostics.push(Diagnostic {
                            range: arm.body.range,
                            severity: Some(DiagnosticSeverity::WARNING),
                            message: format!(
                                "Match arm type '{}' is inconsistent with first arm type '{}'.",
                                type_display(&arm_type),
                                type_display(&first_type)
                            ),
                            ..Default::default()
                        });
                    }
                }

                let resolved = inference_data.type_map.is_resolved(&scrutinee.id)
                    && arms
                        .iter()
                        .all(|elt| inference_data.type_map.is_resolved(&elt.body.id));

                (first_type, resolved)
            }
        }
        ExprKind::Projection { index, tuple } => {
            // first, get type of the tuple
            let tuple_type = infer_expr_type(tuple, inference_data);
            match tuple_type {
                Type::Tuple(types) => match types.get(*index) {
                    Some(found_type) => (
                        found_type.clone(),
                        inference_data.type_map.is_resolved(&tuple.id),
                    ),
                    None => {
                        inference_data.diagnostics.push(Diagnostic {
                            range: expr.range,
                            severity: Some(DiagnosticSeverity::ERROR),

                            message: "Index of projection was out-of-bounds for the tuple."
                                .to_string(),
                            ..Default::default()
                        });
                        (Type::Unknown, false)
                    }
                },
                Type::Unknown => (Type::Unknown, false),
                _ => {
                    inference_data.diagnostics.push(Diagnostic {
                        range: expr.range,
                        severity: Some(DiagnosticSeverity::ERROR),

                        message: format!("Cannot perform projection on type {}", tuple_type),
                        ..Default::default()
                    });
                    (Type::Unknown, false)
                }
            }
        }
    };

    // at this point, we have the found_type and resolved values
    // overlay the found_type in the type_map, meaning it will insert if nothing
    // present or strictly add info if something present
    inference_data.type_map.overlay(expr.id, &found_type);
    if resolved {
        inference_data.type_map.set_resolved(expr.id);
    }

    inference_data.type_map.get(&expr.id).unwrap().clone()
}

/// Formats a Type for display in user-facing diagnostic messages.
/// Delegates to the Display impl on Type (defined in symbols.rs).
pub fn type_display(ty: &Type) -> String {
    format!("{}", ty)
}

/// Comparisons are things like == or !=
/// Unknown is treated generously to avoid cascading errors.
fn types_comparable(t1: &Type, t2: &Type) -> bool {
    if types_math(t1, t2).is_some() {
        // math types treated all as just numbers
        true
    } else {
        types_compatible(t1, t2)
    }
}

// If this returns Some, then the types are ones we can use <, >, +, *, etc
// on. If this returns Some, it additionally gives the resulting type of the
// operation GIVEN that it is one of plus, minus, times, divides, or mod.
// If this returns None, then we cannot use math operations on these two.
// Unknown is treated generously to avoid cascading errors.
fn types_math(t1: &Type, t2: &Type) -> Option<Type> {
    match t1 {
        // if first type is int, then we can deal with 'autocast' here
        Type::Int => {
            if matches!(t2, Type::Int | Type::Float | Type::Location | Type::Qubit) {
                Some(t2.clone())
            } else if matches!(t2, Type::Unknown) {
                Some(Type::Int)
            } else {
                None
            }
        }

        // if first type is one of the other math-y types, then we can do a math
        // op if the other type is an int, or just if the two types are equal
        Type::Float | Type::Location | Type::Qubit => {
            if matches!(t2, Type::Int) || t1 == t2 || matches!(t2, Type::Unknown) {
                Some(t1.clone())
            } else {
                None
            }
        }

        // avoid propigating errors
        // if t1 is unknown, permit so long as t2 is a math type (or unknown)
        Type::Unknown => {
            if matches!(
                t2,
                Type::Int | Type::Float | Type::Location | Type::Qubit | Type::Unknown
            ) {
                Some(t2.clone())
            } else {
                None
            }
        }

        // t1 needs to at least be one of the above types
        _ => None,
    }
}

/// Checks if two types are compatible for assignment or comparison.
///
/// - Treats `Unknown` as compatible with all types to avoid cascading errors
/// - Allows numeric leniency (Int ↔ Float)
/// - Allows QubitMap indexing leniency (Qubit ↔ Int)
fn types_compatible(t1: &Type, t2: &Type) -> bool {
    // Avoid Cascading errors
    if matches!(t1, Type::Unknown) || matches!(t2, Type::Unknown) {
        return true;
    }

    match (t1, t2) {
        (Type::Int, Type::Int)
        | (Type::Float, Type::Float)
        | (Type::Bool, Type::Bool)
        | (Type::String, Type::String)
        | (Type::Location, Type::Location)
        | (Type::Qubit, Type::Qubit)
        | (Type::QubitMap, Type::QubitMap)
        | (Type::Gate, Type::Gate) => true,

        // Qubit/Int leniency - Qubit wraps usize, so Int literals are valid shorthand
        (Type::Qubit, Type::Int) => true,
        (Type::Int, Type::Qubit) => true,

        // Location/Int leniency - Location wraps usize, valid as Vec index
        (Type::Location, Type::Int) => true,
        (Type::Int, Type::Location) => true,

        // Numeric leniency
        (Type::Int, Type::Float) => true,
        (Type::Float, Type::Int) => true,

        (Type::ArchT, Type::ArchT) => true,
        (Type::StateT, Type::StateT) => true,

        (Type::Vec(inner1), Type::Vec(inner2)) => types_compatible(inner1, inner2),
        (Type::Tuple(items1), Type::Tuple(items2)) => {
            items1.len() == items2.len()
                && items1
                    .iter()
                    .zip(items2)
                    .all(|(a, b)| types_compatible(a, b))
        }
        (Type::Option(inner1), Type::Option(inner2)) => types_compatible(inner1, inner2),

        (
            Type::Function {
                params: p1,
                return_type: r1,
            },
            Type::Function {
                params: p2,
                return_type: r2,
            },
        ) => {
            p1.len() == p2.len()
                && p1.iter().zip(p2).all(|(a, b)| types_compatible(a, b))
                && types_compatible(r1, r2)
        }
        (Type::UserDef(n1), Type::UserDef(n2)) => n1 == n2,
        (Type::Generic(_), Type::Generic(_)) => true,
        _ => false,
    }
}

/// From a given type, provides all autocomplete suggestions, if they exist.
/// This would be what appears after the user types a '.'
pub fn suggest_next_from_type(
    t1: &Type,
    user_def_table: &UserDefTable,
) -> Option<Vec<CompletionItem>> {
    match t1 {
        Type::Tuple(items) => {
            // show autocomplete for each item
            if items.is_empty() {
                None
            } else {
                Some(
                    items
                        .iter()
                        .enumerate()
                        .map(|(index, item)| CompletionItem {
                            label: format!("({})", index),
                            label_details: Some(CompletionItemLabelDetails {
                                detail: None,
                                description: Some(format!("{}", item)),
                            }),
                            kind: None,
                            detail: None,
                            ..Default::default()
                        })
                        .collect(),
                )
            }
        }
        Type::UserDef(name) => user_def_table.get_fields(name).map(|hashmap| {
            hashmap
                .iter()
                .map(|entry| CompletionItem {
                    label: entry.0.clone(),
                    label_details: Some(CompletionItemLabelDetails {
                        detail: None,
                        description: Some(format!("{}", entry.1)),
                    }),
                    kind: None,
                    detail: None,
                    ..Default::default()
                })
                .collect()
        }),
        _ => {
            // all other types, we need to check if there are any built-ins
            if let Some(built_ins) = builtins::get_all_built_ins_after_type(t1) {
                match built_ins {
                    // TODO: I do not like this ownership structure I created in
                    // fighting the borrow checker.
                    builtins::Owner::Owned(vec) => {
                        Some(vec.iter().map(|elt| elt.to_completion_item(None)).collect())
                    }
                    builtins::Owner::Borrowed(vec) => {
                        Some(vec.iter().map(|elt| elt.to_completion_item(None)).collect())
                    }
                }
            } else {
                None
            }
        }
    }
}

/// Given an expression, PASSIVELY changes its type to a new type. Does so
/// recursively.
///
/// A "passive" change means that only generics and unknowns are effected, like
/// an overlay. It will NOT change any aspects of the type that are non-generics
/// and non-unknowns.
///
/// This is helpful for if we have some type that uses generics where we cannot
/// determine the type, then later discover the generic type.
/// For instance, if we have something with the type "|?| -> Vec<?>", we can
/// come back and tell it that it is actually "|Int| -> Vec<Float>". It will
/// recursively (as best it can) retype all the subexpressions to match this
/// format.
///
/// Doesn't retype identifiers. Additionally, doesn't "identify" the generics.
/// Use this once we have identified the mappings of the generics first, otherwise
/// we risk losing information by losing association between generics and their types.
pub fn retype(expr: &Expr, new_type: Type, inference_data: &mut InferenceData) {
    // retyping starts with a simple overlay.
    inference_data.type_map.overlay(expr.id, &new_type);

    // then, we see what type of expression we're dealing with, and see if
    // consequently any subexpressions need to be retyped too
    match &expr.kind {
        ExprKind::Identifier(_) => {}
        ExprKind::IntLiteral(_) => {}
        ExprKind::FloatLiteral(_) => {}
        ExprKind::StringLiteral(_) => {}
        ExprKind::BoolLiteral(_) => {}
        ExprKind::List(exprs) => {
            // retype all sub expressions
            if let Type::Vec(ref inner) = new_type {
                exprs
                    .iter()
                    .for_each(|elt| retype(elt, *inner.clone(), inference_data));
            }
        }
        ExprKind::Tuple(exprs) => {
            // retype all sub expressions
            if let Type::Tuple(ref inners) = new_type {
                exprs
                    .iter()
                    .zip(inners.iter())
                    .for_each(|pair| retype(pair.0, pair.1.clone(), inference_data));
            }
        }
        ExprKind::StructLiteral { .. } => {
            // nothing special to do here
        }
        ExprKind::FunctionCall { .. } => {
            // consider retyping the function's return type to be whatever this type is
        }
        ExprKind::FieldAccess { .. } => {}
        ExprKind::IndexAccess { object, .. } => {
            // can retype the object as a vec of elements of this type
            retype(
                object,
                Type::Vec(Box::new(new_type.clone())),
                inference_data,
            );
        }
        ExprKind::Lambda { body, .. } => {
            // body of lambda should have this return type
            if let Type::Function {
                ref return_type, ..
            } = new_type
            {
                retype(body, *return_type.clone(), inference_data);
            }
        }
        ExprKind::IfThenElse {
            then_branch,
            else_branch,
            ..
        } => {
            // the type of the expr is just the type of both branches
            retype(then_branch, new_type.clone(), inference_data);
            retype(else_branch, new_type.clone(), inference_data);
        }
        ExprKind::LetBinding { body, .. } => {
            // return type of body is the type of whole let expr
            retype(body, new_type.clone(), inference_data);
        }
        ExprKind::BinaryOp { op, left, right } => {
            // TODO This implementation double-implements some logic that is also
            // elsewhere. Should unify this, but will be challenging to do so.
            match op {
                BinaryOperator::Add
                | BinaryOperator::Sub
                | BinaryOperator::Mul
                | BinaryOperator::Div
                | BinaryOperator::Mod => {
                    // then just say that the left type and right type should be
                    // this too
                    retype(left, new_type.clone(), inference_data);
                    retype(right, new_type.clone(), inference_data);
                }
                BinaryOperator::Eq | BinaryOperator::Ne => {
                    // uhhh, well this is a boolean.
                    // we could at least be able to retype left and right to be
                    // equal types. which we take is unclear. depends on which
                    // has generics.
                }
                BinaryOperator::Lt
                | BinaryOperator::Le
                | BinaryOperator::Gt
                | BinaryOperator::Ge
                | BinaryOperator::And
                | BinaryOperator::Or => {
                    // same as above
                }
                BinaryOperator::Range | BinaryOperator::Tensor => {
                    // unclear what if anything should happen here
                }
            }
        }
        ExprKind::UnaryOp { operand, .. } => {
            // currently, type type of any unary op will always be the same
            // as whatever's beneath
            retype(operand, new_type.clone(), inference_data);
        }
        ExprKind::Some(inner) => {
            // can retype inner too
            if let Type::Option(ref inner_type) = new_type {
                retype(inner, *inner_type.clone(), inference_data);
            }
        }
        ExprKind::None => {
            // nothing special here
        }
        ExprKind::TensorProduct { .. } => {
            // don't know what to do here really
        }
        ExprKind::Projection { .. } => {
            // very non-trivial to retype descent, so this should be OK
        }
        ExprKind::Match { arms, .. } => {
            // the type of the match expr is the type of all the arms
            arms.iter()
                .for_each(|elt| retype(&elt.body, new_type.clone(), inference_data));
        }
    }
}

#[cfg(test)]
mod tests {
    use tower_lsp::lsp_types::Position;

    use super::*;

    #[test]
    fn test_types_compatible_primitives() {
        assert!(types_compatible(&Type::Int, &Type::Int));
        assert!(types_compatible(&Type::Float, &Type::Float));
        assert!(types_compatible(&Type::Bool, &Type::Bool));

        // Int/Float mixing (Leniency)
        assert!(types_compatible(&Type::Int, &Type::Float));
        assert!(types_compatible(&Type::Float, &Type::Int));

        // Mismatches
        assert!(!types_compatible(&Type::Int, &Type::Bool));
    }

    #[test]
    fn test_types_compatible_joker_rule() {
        assert!(types_compatible(&Type::Unknown, &Type::Int));
        assert!(types_compatible(&Type::Int, &Type::Unknown));
        assert!(types_compatible(&Type::Unknown, &Type::Unknown));

        let vec_int = Type::Vec(Box::new(Type::Int));
        assert!(types_compatible(&Type::Unknown, &vec_int));
    }

    #[test]
    fn test_types_compatible_compound() {
        let vec_int = Type::Vec(Box::new(Type::Int));
        let vec_float = Type::Vec(Box::new(Type::Float));

        assert!(types_compatible(&vec_int, &vec_int));
        assert!(types_compatible(&vec_int, &vec_float));

        let tuple1 = Type::Tuple(vec![Type::Int, Type::Float]);
        let tuple2 = Type::Tuple(vec![Type::Int, Type::Float]);
        let tuple3 = Type::Tuple(vec![Type::Int, Type::Bool]);

        assert!(types_compatible(&tuple1, &tuple2));
        assert!(!types_compatible(&tuple1, &tuple3));
    }

    #[test]
    fn test_types_compatible_functions() {
        let fn1 = Type::Function {
            params: vec![Type::Int],
            return_type: Box::new(Type::Bool),
        };
        let fn2 = Type::Function {
            params: vec![Type::Float],
            return_type: Box::new(Type::Bool),
        };
        let fn3 = Type::Function {
            params: vec![Type::Int, Type::Int],
            return_type: Box::new(Type::Bool),
        };
        let fn4 = Type::Function {
            params: vec![Type::Int],
            return_type: Box::new(Type::Int),
        };

        assert!(types_compatible(&fn1, &fn2));
        assert!(!types_compatible(&fn1, &fn3));
        assert!(!types_compatible(&fn1, &fn4));
    }

    #[test]
    fn test_infer_bin_op_math() {
        let mut diags = Vec::new();
        let mut inf_data = InferenceData {
            sym_table: &mut SymbolTable::new(),
            diagnostics: &mut diags,
            type_map: &mut TypeMap::new(),
            user_def_table: &UserDefTable::empty(),
            generic_table: &mut GenericTable::new(),
            string_labels: &mut StringLabels::new(),
        };

        let def_range = Range {
            start: Position::new(0, 0),
            end: Position::new(0, 0),
        };
        let expr_float_add = Expr {
            kind: ExprKind::BinaryOp {
                op: BinaryOperator::Add,
                left: Box::new(Expr {
                    kind: ExprKind::FloatLiteral(3.0),
                    range: def_range.clone(),
                    id: NodeId(1),
                }),
                right: Box::new(Expr {
                    kind: ExprKind::FloatLiteral(5.0),
                    range: def_range.clone(),
                    id: NodeId(2),
                }),
            },
            range: def_range.clone(),
            id: NodeId(0),
        };

        let expr_int_mul = Expr {
            kind: ExprKind::BinaryOp {
                op: BinaryOperator::Mul,
                left: Box::new(Expr {
                    kind: ExprKind::IntLiteral(6),
                    range: def_range.clone(),
                    id: NodeId(6),
                }),
                right: Box::new(Expr {
                    kind: ExprKind::IntLiteral(-3),
                    range: def_range.clone(),
                    id: NodeId(7),
                }),
            },
            range: def_range.clone(),
            id: NodeId(8),
        };

        let expr_loc_autocast = Expr {
            kind: ExprKind::BinaryOp {
                op: BinaryOperator::Div,
                left: Box::new(Expr {
                    kind: ExprKind::IntLiteral(6),
                    range: def_range.clone(),
                    id: NodeId(3),
                }),
                right: Box::new(Expr {
                    kind: ExprKind::FunctionCall {
                        function: Box::new(Expr {
                            kind: ExprKind::Identifier("Location".to_string()),
                            range: def_range.clone(),
                            id: NodeId(50),
                        }),
                        args: vec![Expr {
                            kind: ExprKind::IntLiteral(100),
                            range: def_range.clone(),
                            id: NodeId(51),
                        }],
                    },
                    range: def_range.clone(),
                    id: NodeId(4),
                }),
            },
            range: def_range.clone(),
            id: NodeId(5),
        };

        let expr_qubit_autocast = Expr {
            kind: ExprKind::BinaryOp {
                op: BinaryOperator::Div,
                left: Box::new(Expr {
                    kind: ExprKind::FunctionCall {
                        function: Box::new(Expr {
                            kind: ExprKind::Identifier("Qubit".to_string()),
                            range: def_range.clone(),
                            id: NodeId(60),
                        }),
                        args: vec![Expr {
                            kind: ExprKind::IntLiteral(2),
                            range: def_range.clone(),
                            id: NodeId(61),
                        }],
                    },
                    range: def_range.clone(),
                    id: NodeId(62),
                }),
                right: Box::new(Expr {
                    kind: ExprKind::IntLiteral(3),
                    range: def_range.clone(),
                    id: NodeId(63),
                }),
            },
            range: def_range.clone(),
            id: NodeId(64),
        };

        let expr_odd_types = Expr {
            kind: ExprKind::BinaryOp {
                op: BinaryOperator::Ge,
                left: Box::new(Expr {
                    kind: ExprKind::FunctionCall {
                        function: Box::new(Expr {
                            kind: ExprKind::Identifier("Qubit".to_string()),
                            range: def_range.clone(),
                            id: NodeId(60),
                        }),
                        args: vec![Expr {
                            kind: ExprKind::IntLiteral(2),
                            range: def_range.clone(),
                            id: NodeId(71),
                        }],
                    },
                    range: def_range.clone(),
                    id: NodeId(72),
                }),
                right: Box::new(Expr {
                    kind: ExprKind::FloatLiteral(5.0),
                    range: def_range.clone(),
                    id: NodeId(73),
                }),
            },
            range: def_range.clone(),
            id: NodeId(74),
        };
        assert_eq!(Type::Float, infer_expr_type(&expr_float_add, &mut inf_data));
        assert_eq!(inf_data.diagnostics.len(), 0);
        assert_eq!(Type::Int, infer_expr_type(&expr_int_mul, &mut inf_data));
        assert_eq!(inf_data.diagnostics.len(), 0);
        assert_eq!(
            Type::Location,
            infer_expr_type(&expr_loc_autocast, &mut inf_data)
        );
        assert_eq!(inf_data.diagnostics.len(), 0);
        assert_eq!(
            Type::Qubit,
            infer_expr_type(&expr_qubit_autocast, &mut inf_data)
        );
        assert_eq!(inf_data.diagnostics.len(), 0);

        infer_expr_type(&expr_odd_types, &mut inf_data);
        assert!(inf_data.diagnostics.len() > 0);
    }

    #[test]
    fn test_infer_bin_op_logic() {
        let mut diags = Vec::new();
        let mut inf_data = InferenceData {
            sym_table: &mut SymbolTable::new(),
            diagnostics: &mut diags,
            type_map: &mut TypeMap::new(),
            user_def_table: &UserDefTable::empty(),
            generic_table: &mut GenericTable::new(),
            string_labels: &mut StringLabels::new(),
        };

        let def_range = Range {
            start: Position::new(0, 0),
            end: Position::new(0, 0),
        };
        let expr_valid = Expr {
            kind: ExprKind::BinaryOp {
                op: BinaryOperator::And,
                left: Box::new(Expr {
                    kind: ExprKind::BoolLiteral(true),
                    range: def_range.clone(),
                    id: NodeId(1),
                }),
                right: Box::new(Expr {
                    kind: ExprKind::BoolLiteral(false),
                    range: def_range.clone(),
                    id: NodeId(2),
                }),
            },
            range: def_range.clone(),
            id: NodeId(0),
        };

        let expr_invalid = Expr {
            kind: ExprKind::BinaryOp {
                op: BinaryOperator::And,
                left: Box::new(Expr {
                    kind: ExprKind::BoolLiteral(true),
                    range: def_range.clone(),
                    id: NodeId(10),
                }),
                right: Box::new(Expr {
                    kind: ExprKind::Projection {
                        index: 1,
                        tuple: Box::new(Expr {
                            range: def_range.clone(),
                            id: NodeId(11),
                            kind: ExprKind::Tuple(vec![
                                Expr {
                                    range: def_range.clone(),
                                    id: NodeId(12),
                                    kind: ExprKind::BoolLiteral(true),
                                },
                                Expr {
                                    range: def_range.clone(),
                                    id: NodeId(13),
                                    kind: ExprKind::IntLiteral(1),
                                },
                            ]),
                        }),
                    },
                    range: def_range.clone(),
                    id: NodeId(14),
                }),
            },
            range: def_range.clone(),
            id: NodeId(15),
        };

        assert_eq!(Type::Bool, infer_expr_type(&expr_valid, &mut inf_data));
        assert_eq!(inf_data.diagnostics.len(), 0);

        infer_expr_type(&expr_invalid, &mut inf_data);
        assert!(inf_data.diagnostics.len() > 0);
    }

    #[test]
    fn test_infer_unary() {
        let mut diags = Vec::new();
        let mut inf_data = InferenceData {
            sym_table: &mut SymbolTable::new(),
            diagnostics: &mut diags,
            type_map: &mut TypeMap::new(),
            user_def_table: &UserDefTable::empty(),
            generic_table: &mut GenericTable::new(),
            string_labels: &mut StringLabels::new(),
        };

        let def_range = Range {
            start: Position::new(0, 0),
            end: Position::new(0, 0),
        };
        let not_on_valid = Expr {
            kind: ExprKind::UnaryOp {
                op: UnaryOperator::Not,
                operand: Box::new(Expr {
                    kind: ExprKind::BoolLiteral(true),
                    range: def_range.clone(),
                    id: NodeId(0),
                }),
            },
            range: def_range.clone(),
            id: NodeId(1),
        };

        let not_on_invalid = Expr {
            kind: ExprKind::UnaryOp {
                op: UnaryOperator::Not,
                operand: Box::new(Expr {
                    kind: ExprKind::FloatLiteral(-5.0),
                    range: def_range.clone(),
                    id: NodeId(2),
                }),
            },
            range: def_range.clone(),
            id: NodeId(3),
        };

        let neg_on_valid = Expr {
            kind: ExprKind::UnaryOp {
                op: UnaryOperator::Neg,
                operand: Box::new(Expr {
                    kind: ExprKind::FloatLiteral(-5.0),
                    range: def_range.clone(),
                    id: NodeId(3),
                }),
            },
            range: def_range.clone(),
            id: NodeId(4),
        };

        let neg_on_invalid = Expr {
            kind: ExprKind::UnaryOp {
                op: UnaryOperator::Neg,
                operand: Box::new(Expr {
                    kind: ExprKind::Identifier("vec".to_string()),
                    range: def_range.clone(),
                    id: NodeId(5),
                }),
            },
            range: def_range.clone(),
            id: NodeId(6),
        };

        assert_eq!(Type::Bool, infer_expr_type(&not_on_valid, &mut inf_data));
        assert_eq!(inf_data.diagnostics.len(), 0);

        infer_expr_type(&not_on_invalid, &mut inf_data);
        assert!(inf_data.diagnostics.len() > 0);

        // reset diags
        let mut new_diags = Vec::new();
        inf_data.diagnostics = &mut new_diags;

        assert_eq!(Type::Float, infer_expr_type(&neg_on_valid, &mut inf_data));
        assert_eq!(inf_data.diagnostics.len(), 0);

        infer_expr_type(&neg_on_invalid, &mut inf_data);
        assert!(inf_data.diagnostics.len() > 0);
    }

    #[test]
    fn test_suggest_next_from_type() {
        // make a user-def table
        let user_def_table = UserDefTable::empty();

        assert!(suggest_next_from_type(&Type::Int, &user_def_table).is_none());

        let arch_res = suggest_next_from_type(&Type::ArchT, &user_def_table);
        assert!(arch_res.is_some());
        let arch_res = arch_res.unwrap();

        let height_type_name = format!("{}", Type::Int);

        let edges_type_name = format!(
            "{}",
            Type::Function {
                params: vec![],
                return_type: Box::new(Type::Vec(Box::new(Type::Tuple(vec![
                    Type::Location,
                    Type::Location,
                ])))),
            }
        );

        assert!(
            arch_res
                .iter()
                .find(|elt| if elt.label == "height" {
                    if let Some(CompletionItemLabelDetails { description, .. }) = &elt.label_details
                    {
                        description.is_some()
                            && description.as_ref().unwrap().as_str() == height_type_name
                    } else {
                        false
                    }
                } else {
                    false
                })
                .is_some()
        );

        assert!(
            arch_res
                .iter()
                .find(|elt| if elt.label == "edges" {
                    if let Some(CompletionItemLabelDetails { description, .. }) = &elt.label_details
                    {
                        description.is_some()
                            && description.as_ref().unwrap().as_str() == edges_type_name
                    } else {
                        false
                    }
                } else {
                    false
                })
                .is_some()
        );

        let vec_res = suggest_next_from_type(&Type::Vec(Box::new(Type::QubitMap)), &user_def_table);
        assert!(vec_res.is_some());
        let vec_res = vec_res.unwrap();

        let push_type_name = format!(
            "{}",
            Type::Function {
                params: vec![Type::QubitMap],
                return_type: Box::new(Type::Vec(Box::new(Type::QubitMap))),
            }
        );
        let contains_type_name = format!(
            "{}",
            Type::Function {
                params: vec![Type::QubitMap],
                return_type: Box::new(Type::Bool),
            }
        );

        let len_type_name = format!("{}", Type::Int);

        assert!(
            vec_res
                .iter()
                .find(|elt| if elt.label == "push" {
                    if let Some(CompletionItemLabelDetails { description, .. }) = &elt.label_details
                    {
                        description.is_some()
                            && description.as_ref().unwrap().as_str() == push_type_name
                    } else {
                        false
                    }
                } else {
                    false
                })
                .is_some()
        );

        assert!(
            vec_res
                .iter()
                .find(|elt| if elt.label == "contains" {
                    if let Some(CompletionItemLabelDetails { description, .. }) = &elt.label_details
                    {
                        description.is_some()
                            && description.as_ref().unwrap().as_str() == contains_type_name
                    } else {
                        false
                    }
                } else {
                    false
                })
                .is_some()
        );

        assert!(
            vec_res
                .iter()
                .find(|elt| if elt.label == "len" {
                    if let Some(CompletionItemLabelDetails { description, .. }) = &elt.label_details
                    {
                        description.is_some()
                            && description.as_ref().unwrap().as_str() == len_type_name
                    } else {
                        false
                    }
                } else {
                    false
                })
                .is_some()
        );
    }
}
