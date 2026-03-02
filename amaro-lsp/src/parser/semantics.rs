use super::symbols::*;
use crate::ast::*;
use std::collections::HashMap;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, Diagnostic,
    DiagnosticRelatedInformation, DiagnosticSeverity, Location, Range, Url,
};

/// Performs semantic analysis on a parsed Amaro file.
///
/// Validates block structure, required fields, and type correctness.
/// Returns diagnostics for LSP clients.
pub fn check_semantics(file: &AmaroFile) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let known_blocks = [
        "GateRealization",
        "Transition",
        "Architecture",
        "Arch",
        "Step",
        "RouteInfo",
        "TransitionInfo",
        "ArchInfo",
        "StateInfo",
    ];

    let mut required_keys: HashMap<&str, Vec<&str>> = HashMap::new();
    required_keys.insert("RouteInfo", vec!["routed_gates", "realize_gate"]);
    required_keys.insert("TransitionInfo", vec!["get_transitions", "apply", "cost"]);
    required_keys.insert("ArchInfo", vec![]);
    required_keys.insert("StateInfo", vec![]);

    let mut found_blocks: HashMap<String, Range> = HashMap::new();

    let user_def_table = UserDefTable::new(file);

    // Block Level Validation
    for block in &file.blocks {
        let block_name = block.kind.as_str();
        let lower_name = block_name.to_lowercase();

        // 1. Capitalization Check
        if let Some(correct_name) = known_blocks
            .iter()
            .find(|&&kb| kb.eq_ignore_ascii_case(block_name))
            && block_name != *correct_name
        {
            diagnostics.push(Diagnostic {
                range: block.range,
                severity: Some(DiagnosticSeverity::WARNING),
                message: format!(
                    "Block '{}' should be Capitalized (e.g., '{}').",
                    block_name, correct_name
                ),
                ..Default::default()
            });
        }

        // 2. Uniqueness Check
        if let Some(first_range) = found_blocks.get(&lower_name) {
            diagnostics.push(Diagnostic {
                range: block.range,
                severity: Some(DiagnosticSeverity::ERROR),
                message: format!("Duplicate definition of '{}' block.", block_name),
                related_information: Some(vec![DiagnosticRelatedInformation {
                    location: Location {
                        uri: Url::parse("file:///previous/definition")
                            .unwrap_or_else(|_| Url::parse("file:///unknown").unwrap()),
                        range: *first_range,
                    },
                    message: "First defined here".to_string(),
                }]),
                ..Default::default()
            });
        } else {
            found_blocks.insert(lower_name, block.range);
        }

        // 3. Type Check all fields
        let mut sym_table = SymbolTable::new();
        let mut type_map: HashMap<NodeId, Type> = HashMap::new();
        let mut present_keys: Vec<&str> = Vec::new();
        let BlockContent::Fields(items) = &block.content;
        for item in items {
            if let BlockItem::Field(field) = item {
                present_keys.push(field.key.as_str());

                let mut inf_data = InferenceData {
                    sym_table: &mut sym_table,
                    diagnostics: &mut diagnostics,
                    type_map: &mut type_map,
                    user_def_table: &user_def_table,
                };
                infer_expr_type(&field.value, &mut inf_data);

                // 3.1. Gate Validation in 'routed_gates' fields
                if block_name == "RouteInfo" && field.key == "routed_gates" {
                    validate_gates(&field.value, &mut diagnostics);
                }
            }
        }

        // 4. Required Keys Check
        if let Some(reqs) = required_keys.get(block_name) {
            for req in reqs {
                if !present_keys.contains(req) {
                    diagnostics.push(Diagnostic {
                        range: block.range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: format!(
                            "Block '{}' is missing required field: '{}'",
                            block_name, req
                        ),
                        ..Default::default()
                    });
                }
            }
        }
    }

    // 5. Mandatory Blocks Check
    let required_blocks = ["RouteInfo", "TransitionInfo"];
    for req in required_blocks {
        if !found_blocks.contains_key(&req.to_lowercase()) {
            diagnostics.push(Diagnostic {
                range: Range::default(),
                severity: Some(DiagnosticSeverity::ERROR),
                message: format!("Missing mandatory block: '{}'.", req),
                ..Default::default()
            });
        }
    }

    diagnostics
}

/// Validates that gate identifiers are recognized gate types (CX, T, Pauli, PauliMeasurement).
fn validate_gates(expr: &Expr, diagnostics: &mut Vec<Diagnostic>) {
    let valid_gates = ["CX", "T", "Pauli", "PauliMeasurement"];

    match &expr.kind {
        ExprKind::Identifier(name) => {
            if !valid_gates.contains(&name.as_str()) {
                diagnostics.push(Diagnostic {
                    range: expr.range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    message: format!(
                        "'{}' is not a recognized standard gate. Expected one of: {:?}",
                        name, valid_gates
                    ),
                    ..Default::default()
                });
            }
        }
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            for item in items {
                validate_gates(item, diagnostics);
            }
        }
        _ => {}
    }
}

/// Aggregate of the args passed to infer_expr_type,
/// so we can easily change these without having to change 10000 call signatures
pub struct InferenceData<'a> {
    pub sym_table: &'a mut SymbolTable,
    pub diagnostics: &'a mut Vec<Diagnostic>,
    pub type_map: &'a mut HashMap<NodeId, Type>,
    pub user_def_table: &'a UserDefTable,
}

/// Infers the type of an expression using the current symbol table.
/// (Type Inference Engine)
/// Recursively walks the AST and emits type errors for incompatibilities.
/// Uses `Unknown` for leniency to avoid false positives.
pub fn infer_expr_type(expr: &Expr, inference_data: &mut InferenceData) -> Type {
    let found_type = match &expr.kind {
        ExprKind::IntLiteral(_) => Type::Int,
        ExprKind::FloatLiteral(_) => Type::Float,
        ExprKind::BoolLiteral(_) => Type::Bool,
        ExprKind::StringLiteral(_) => Type::String,
        ExprKind::None => Type::Option(Box::new(Type::Unknown)),
        ExprKind::Identifier(name) => {
            if matches!(name.as_str(), "CX" | "T" | "Pauli" | "PauliMeasurement") {
                Type::Gate
            } else {
                inference_data
                    .sym_table
                    .lookup(name)
                    .cloned()
                    .unwrap_or_else(|| {
                        // check for user defined type
                        match inference_data.user_def_table.get_fields(name) {
                            Some(_) => Type::UserDef(name.clone()),
                            None => {
                                inference_data.diagnostics.push(Diagnostic {
                                    range: expr.range,
                                    severity: Some(DiagnosticSeverity::ERROR),
                                    message: format!("Undefined variable '{}'.", name),
                                    ..Default::default()
                                });
                                Type::Unknown
                            }
                        }
                    })
            }
        }
        ExprKind::List(items) => {
            if items.is_empty() {
                Type::Vec(Box::new(Type::Unknown))
            } else {
                let first_type = infer_expr_type(&items[0], inference_data);
                for item in &items[1..] {
                    let item_type = infer_expr_type(item, inference_data);
                    if item_type != first_type {
                        inference_data.diagnostics.push(Diagnostic {
                            range: item.range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            message: "Inconsistent types in list literal.".to_string(),
                            ..Default::default()
                        });
                        return Type::Vec(Box::new(Type::Unknown));
                    }
                }
                Type::Vec(Box::new(first_type))
            }
        }
        ExprKind::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|e| infer_expr_type(e, inference_data))
                .collect(),
        ),
        ExprKind::Some(inner) => {
            let inner_type = infer_expr_type(inner, inference_data);
            Type::Option(Box::new(inner_type))
        }
        ExprKind::Lambda { params, body } => {
            inference_data.sym_table.enter_scope();
            let mut param_types = Vec::new();
            for param in params {
                inference_data.sym_table.bind(param.clone(), Type::Unknown);
                param_types.push(Type::Unknown);
            }
            let return_type = infer_expr_type(body, inference_data);
            inference_data.sym_table.exit_scope();

            Type::Function {
                params: param_types,
                return_type: Box::new(return_type),
            }
        }
        ExprKind::LetBinding { name, value, body } => {
            inference_data.sym_table.enter_scope();
            let value_type = infer_expr_type(value, inference_data);
            inference_data.sym_table.bind(name.clone(), value_type);
            let body_type = infer_expr_type(body, inference_data);
            inference_data.sym_table.exit_scope();
            body_type
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

            let then_type = infer_expr_type(then_branch, inference_data);
            let else_type = infer_expr_type(else_branch, inference_data);

            if !types_compatible(&then_type, &else_type) {
                inference_data.diagnostics.push(Diagnostic {
                    range: expr.range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: "Then and else branches of if-then-else must have compatible types."
                        .to_string(),
                    ..Default::default()
                });
            }
            then_type
        }
        ExprKind::FunctionCall { function, args } => {
            let func_type = infer_expr_type(function, inference_data);
            match func_type {
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
                        return *return_type;
                    }
                    for (i, (param_type, arg)) in params.iter().zip(args).enumerate() {
                        let arg_type = infer_expr_type(arg, inference_data);

                        // Following Logic
                        // 1. If param_type Unknown, Accept
                        // 2. If arg_type Unknown, Accept (Avoid Cascading Errors)
                        // 3. Otherwise, Check Compatibility
                        if *param_type != Type::Unknown
                            && arg_type != Type::Unknown
                            && !types_compatible(param_type, &arg_type)
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
                    *return_type
                }
                Type::Unknown => Type::Unknown, // Avoid Cascading Errors
                _ => {
                    inference_data.diagnostics.push(Diagnostic {
                        range: function.range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: "Attempted to call a non-function value.".to_string(),
                        ..Default::default()
                    });
                    Type::Unknown
                }
            }
        }
        ExprKind::FieldAccess { object, field } => {
            let obj_type = infer_expr_type(object, inference_data);
            match obj_type {
                Type::Vec(inner) => {
                    if field == "push" {
                        Type::Function {
                            params: vec![*inner.clone()],
                            return_type: Box::new(Type::Vec(inner.clone())),
                        }
                    } else if field == "pop" {
                        Type::Function {
                            params: vec![],
                            return_type: Box::new(Type::Option(inner.clone())),
                        }
                    } else if field == "extend" {
                        Type::Function {
                            params: vec![Type::Vec(inner.clone())],
                            return_type: Box::new(Type::Vec(inner.clone())),
                        }
                    } else if field == "is_empty" {
                        Type::Function {
                            params: vec![],
                            return_type: Box::new(Type::Bool),
                        }
                    } else if field == "contains" {
                        Type::Function {
                            params: vec![*inner.clone()],
                            return_type: Box::new(Type::Bool),
                        }
                    } else if field == "len" {
                        Type::Int
                    } else {
                        Type::Unknown
                    }
                }
                // Type::UserDef(name) => inference_data.user_def_table // TODO
                //Type::Struct { fields, .. } => fields.get(field).cloned().unwrap_or(Type::Unknown),
                Type::Tuple(elements) => {
                    if let Ok(idx) = field.parse::<usize>() {
                        elements.get(idx).cloned().unwrap_or(Type::Unknown)
                    } else {
                        Type::Unknown
                    }
                }

                // Built-in Types
                Type::ArchT => match field.as_str() {
                    "width" | "height" | "stack_size" => Type::Int,
                    "edges" => Type::Function {
                        params: vec![],
                        return_type: Box::new(Type::Vec(Box::new(Type::Tuple(vec![
                            Type::Location,
                            Type::Location,
                        ])))),
                    },
                    "succ_rates" => Type::Vec(Box::new(Type::Vec(Box::new(Type::Float)))),
                    "contains_edge" => Type::Function {
                        params: vec![Type::Tuple(vec![Type::Location, Type::Location])],
                        return_type: Box::new(Type::Bool),
                    },
                    "magic_state_qubits" | "alg_qubits" => Type::Function {
                        params: vec![],
                        return_type: Box::new(Type::Vec(Box::new(Type::Location))),
                    },
                    _ => Type::Unknown,
                },
                Type::StateT => {
                    match field.as_str() {
                        // "map" => Type::QubitMap,
                        "map" => Type::Function {
                            params: vec![],
                            return_type: Box::new(Type::QubitMap),
                        },
                        "gates" => Type::Function {
                            params: vec![],
                            return_type: Box::new(Type::Vec(Box::new(Type::Gate))),
                        },
                        "implemented_gates" => Type::Unknown,
                        _ => Type::Unknown,
                    }
                }
                Type::Gate => match field.as_str() {
                    "qubits" => Type::Vec(Box::new(Type::Qubit)),
                    "gate_type" => Type::Function {
                        params: vec![],
                        return_type: Box::new(Type::Gate),
                    },
                    "implementation" => Type::Unknown,
                    "x_indices" | "y_indices" | "z_indices" => Type::Function {
                        params: vec![],
                        return_type: Box::new(Type::Vec(Box::new(Type::Qubit))),
                    },
                    _ => Type::Unknown,
                },
                Type::UserDef(name) => {
                    // this means that we are indexing into a user-defined type
                    // for instance, Transition.edge
                    inference_data.user_def_table.get_fields(&name).map_or(
                        Type::Unknown,
                        |fields_map| match fields_map.get(field) {
                            Some(t) => t.clone(),
                            None => Type::Unknown,
                        },
                    )
                }
                Type::Unknown => Type::Unknown,
                _ => Type::Unknown,
            }
        }
        ExprKind::StructLiteral { name, fields } => {
            let mut field_types = HashMap::new();
            for (key, value) in fields {
                let val_type = infer_expr_type(value, inference_data);
                field_types.insert(key.clone(), val_type);
            }
            Type::Struct {
                name: name.clone(),
                fields: field_types,
            }
        }
        ExprKind::IndexAccess { object, index } => {
            let obj_type = infer_expr_type(object, inference_data);
            let idx_type = infer_expr_type(index, inference_data);

            // Skip validation for Unknown types to avoid false positives.
            // Example: x.implementation.(path()) where x is Unknown.
            if obj_type == Type::Unknown {
                return Type::Unknown;
            }

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
                        "Index type mismatch. Expected '{:?}' but got '{:?}'.",
                        expected_idx_type, idx_type
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

            match actual_type {
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
                    true => left_type.clone(), // TODO which type to use...?!
                    false => {
                        inference_data.diagnostics.push(Diagnostic {
                            range: expr.range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            // TODO deal with auto-casting of Location
                            message: format!(
                                "Cannot use math operations on types {} and {}.",
                                left_type, right_type
                            ),
                            ..Default::default()
                        });
                        Type::Unknown
                    }
                },
                BinaryOperator::Eq | BinaryOperator::Ne => {
                    match types_comparable(&left_type, &right_type) {
                        true => Type::Bool,
                        false => {
                            {
                                inference_data.diagnostics.push(Diagnostic {
                                    range: expr.range,
                                    severity: Some(DiagnosticSeverity::ERROR),
                                    // TODO deal with auto-casting of Location
                                    message: format!(
                                        "Cannot use equality on types {} and {}.",
                                        left_type, right_type
                                    ),
                                    ..Default::default()
                                });
                                Type::Unknown
                            }
                        }
                    }
                }
                BinaryOperator::Lt
                | BinaryOperator::Le
                | BinaryOperator::Gt
                | BinaryOperator::Ge => match types_math(&left_type, &right_type) {
                    true => Type::Bool,
                    false => {
                        inference_data.diagnostics.push(Diagnostic {
                            range: expr.range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            // TODO deal with auto-casting of Location
                            message: format!(
                                "Cannot use math comparison operations on types {} and {}.",
                                left_type, right_type
                            ),
                            ..Default::default()
                        });
                        Type::Unknown
                    }
                },

                BinaryOperator::And | BinaryOperator::Or => match (&left_type, &right_type) {
                    (Type::Bool, Type::Bool) => Type::Bool,
                    (_, _) => {
                        inference_data.diagnostics.push(Diagnostic {
                            range: expr.range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            // TODO deal with auto-casting of Location
                            message: format!(
                                "Cannot use logical operations on types {} and {}.",
                                left_type, right_type
                            ),
                            ..Default::default()
                        });
                        Type::Unknown
                    }
                },
                _ => {
                    inference_data.diagnostics.push(Diagnostic {
                        range: expr.range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        // TODO deal with auto-casting of Location
                        message: format!("Operator {:?} not yet implemented.", op),
                        ..Default::default()
                    });
                    Type::Unknown
                }
            }
        }
        ExprKind::UnaryOp { op, operand } => {
            let operand_type = infer_expr_type(operand, inference_data);
            match op {
                UnaryOperator::Not => match operand_type {
                    Type::Bool => Type::Bool,
                    _ => {
                        inference_data.diagnostics.push(Diagnostic {
                            range: expr.range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            // TODO is this really all of them? Surely other types may participate.
                            message: "Cannot perform NOT operation on non-bool type.".to_string(),
                            ..Default::default()
                        });
                        Type::Unknown
                    }
                },
                UnaryOperator::Neg => match operand_type {
                    Type::Int => Type::Int,
                    Type::Float => Type::Float,
                    _ => {
                        inference_data.diagnostics.push(Diagnostic {
                            range: expr.range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            // TODO is this really all of them? Surely other types may participate.
                            message: "Cannot perform NEG operation on non-number type.".to_string(),
                            ..Default::default()
                        });
                        Type::Unknown
                    }
                },
            }
        }
        ExprKind::TensorProduct { .. } => Type::Unknown, // TODO what to do here?
        ExprKind::Projection { index, tuple } => {
            // first, get type of the tuple
            let tuple_type = infer_expr_type(tuple, inference_data);
            match tuple_type {
                Type::Tuple(types) => {
                    match types.get(*index) {
                        Some(found_type) => found_type.clone(),
                        None => {
                            inference_data.diagnostics.push(Diagnostic {
                                range: expr.range,
                                severity: Some(DiagnosticSeverity::ERROR),
                                // TODO is this really all of them? Surely other types may participate.
                                message: "Index of projection was out-of-bounds for the tuple."
                                    .to_string(),
                                ..Default::default()
                            });
                            Type::Unknown
                        }
                    }
                }
                _ => {
                    inference_data.diagnostics.push(Diagnostic {
                        range: expr.range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        // TODO is this really all of them? Surely other types may participate.
                        message: format!("Cannot perform projection on type {}", tuple_type),
                        ..Default::default()
                    });
                    Type::Unknown
                }
            }
        }
    };
    inference_data.type_map.insert(expr.id, found_type.clone());

    found_type
}

/// Comparisons are things like == or !=
/// Unknown is treated generously to avoid cascading errors.
fn types_comparable(t1: &Type, t2: &Type) -> bool {
    if types_math(t1, t2) {
        // math types treated all as just numbers
        true
    } else {
        types_compatible(t1, t2)
    }
}

// Math matching means that we can use >, <, +, -, etc.
// Unknown is treated generously to avoid cascading errors.
fn types_math(t1: &Type, t2: &Type) -> bool {
    matches!(
        t1,
        Type::Int | Type::Float | Type::Location | Type::Qubit | Type::Unknown
    ) && matches!(
        t2,
        Type::Int | Type::Float | Type::Location | Type::Qubit | Type::Unknown
    )
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

        _ => false,
    }
}

/// An autocomplete suggestion to show.
/// Contains the replacement text along with the completion type.
/// Modify this in order to most easily change the information provided in the
/// autocomplete window.
pub struct Suggestion {
    pub completion_text: String,
    pub completion_type: Type,
}

impl Suggestion {
    fn get_completion_item_kind(&self) -> CompletionItemKind {
        match &self.completion_type {
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

    fn get_completion_item_label_details(&self) -> CompletionItemLabelDetails {
        CompletionItemLabelDetails {
            detail: match &self.completion_type {
                Type::Vec(..) => Some(": Array".to_string()),
                Type::Tuple(..) => Some(": Tuple".to_string()),
                Type::Function { .. } => Some(": Function".to_string()),
                Type::Struct { .. } => Some(": Struct".to_string()),
                _ => None,
            },
            description: Some(format!("{}", self.completion_type)),
        }
    }

    fn get_completion_item_detail(&self) -> String {
        format!("{}", self.completion_type)
    }

    /// Converts from a Suggestion to a CompletionItem.
    /// The LSP uses CompletionItems.
    pub fn to_completion_item(&self) -> CompletionItem {
        CompletionItem {
            label: self.completion_text.clone(),
            label_details: Some(self.get_completion_item_label_details()),
            kind: Some(self.get_completion_item_kind()),
            detail: Some(self.get_completion_item_detail()),
            documentation: None,
            deprecated: Some(false),
            preselect: Some(false),
            sort_text: None,
            filter_text: None,
            insert_text: None,
            insert_text_format: None,
            insert_text_mode: None,
            text_edit: None,
            additional_text_edits: None,
            command: None,
            commit_characters: None,
            data: None,
            tags: None,
        }
    }
}

/// From a given type, provides all autocomplete suggestions, if they exist.
/// This would be what appears after the user types a '.'
pub fn suggest_next_from_type(t1: &Type, user_def_table: &UserDefTable) -> Option<Vec<Suggestion>> {
    match t1 {
        Type::Int | Type::Float | Type::Bool => None,
        Type::String => None,
        Type::Location => None,
        Type::Qubit => None,
        Type::QubitMap => None,
        Type::Gate => Some(vec![
            Suggestion {
                completion_text: "qubits".to_string(),
                completion_type: Type::Vec(Box::new(Type::Qubit)),
            },
            Suggestion {
                completion_text: "gate_type".to_string(),
                completion_type: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Gate),
                },
            },
            Suggestion {
                completion_text: "implementation".to_string(),
                completion_type: Type::Unknown,
            },
            Suggestion {
                completion_text: "x_indices".to_string(),
                completion_type: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Qubit))),
                },
            },
            Suggestion {
                completion_text: "y_indices".to_string(),
                completion_type: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Qubit))),
                },
            },
            Suggestion {
                completion_text: "z_indices".to_string(),
                completion_type: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Qubit))),
                },
            },
        ]),
        Type::ArchT => Some(vec![
            Suggestion {
                completion_text: "width".to_string(),
                completion_type: Type::Int,
            },
            Suggestion {
                completion_text: "height".to_string(),
                completion_type: Type::Int,
            },
            Suggestion {
                completion_text: "stack_size".to_string(),
                completion_type: Type::Int,
            },
            Suggestion {
                completion_text: "edges".to_string(),
                completion_type: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Tuple(vec![
                        Type::Location,
                        Type::Location,
                    ])))),
                },
            },
            Suggestion {
                completion_text: "succ_rates".to_string(),
                completion_type: Type::Vec(Box::new(Type::Vec(Box::new(Type::Float)))),
            },
            Suggestion {
                completion_text: "contains_edge".to_string(),
                completion_type: Type::Function {
                    params: vec![Type::Tuple(vec![Type::Location, Type::Location])],
                    return_type: Box::new(Type::Bool),
                },
            },
            Suggestion {
                completion_text: "magic_state_qubits".to_string(),
                completion_type: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Location))),
                },
            },
            Suggestion {
                completion_text: "alg_qubits".to_string(),
                completion_type: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Location))),
                },
            },
        ]),
        Type::StateT => Some(vec![
            Suggestion {
                completion_text: "map".to_string(),
                completion_type: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::QubitMap),
                },
            },
            Suggestion {
                completion_text: "gates".to_string(),
                completion_type: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Vec(Box::new(Type::Gate))),
                },
            },
            Suggestion {
                completion_text: "implemented_gates".to_string(),
                completion_type: Type::Unknown,
            },
        ]),
        Type::InstrT => None,
        Type::Vec(inner) => Some(vec![
            Suggestion {
                completion_text: "push".to_string(),
                completion_type: Type::Function {
                    params: vec![*inner.clone()],
                    return_type: Box::new(Type::Vec(inner.clone())),
                },
            },
            Suggestion {
                completion_text: "pop".to_string(),
                completion_type: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Option(inner.clone())),
                },
            },
            Suggestion {
                completion_text: "extend".to_string(),
                completion_type: Type::Function {
                    params: vec![Type::Vec(inner.clone())],
                    return_type: Box::new(Type::Vec(inner.clone())),
                },
            },
            Suggestion {
                completion_text: "is_empty".to_string(),
                completion_type: Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Bool),
                },
            },
            Suggestion {
                completion_text: "contains".to_string(),
                completion_type: Type::Function {
                    params: vec![*inner.clone()],
                    return_type: Box::new(Type::Bool),
                },
            },
            Suggestion {
                completion_text: "len".to_string(),
                completion_type: Type::Int,
            },
        ]),
        Type::Tuple(items) => {
            // show autocomplete for each item
            if items.is_empty() {
                None
            } else {
                Some(
                    items
                        .iter()
                        .enumerate()
                        .map(|(index, item)| Suggestion {
                            completion_text: format!("{}", index),
                            completion_type: item.clone(),
                        })
                        .collect(),
                )
            }
        }
        Type::Option(nested) => Some(vec![Suggestion {
            // TODO to be clear, I don't know that this is what we want.
            // I'm assuming we want an unwrap for options.
            // What else hould options have?
            completion_text: "unwrap".to_string(),
            completion_type: Type::Function {
                params: vec![],
                return_type: nested.clone(),
            },
        }]),
        Type::Function { .. } => None,
        Type::Struct { fields, .. } => Some(
            fields
                .iter()
                .map(|entry| Suggestion {
                    completion_text: entry.0.clone(),
                    completion_type: entry.1.clone(),
                })
                .collect(),
        ),
        Type::UserDef(name) => {
            user_def_table.get_fields(name).map(|hashmap| {
                hashmap
                    .iter()
                    .map(|entry| Suggestion {
                        completion_text: entry.0.clone(),
                        completion_type: entry.1.clone(),
                    })
                    .collect()
            }) // TODO
        }
        Type::Unknown => None,
        Type::Generic(_) => None,
    }
}

/// Suppose we have a type which includes some generics. For instance,
/// map<I,O>(|O| -> I, Vec<I>) has two generics. We want to be able to infer
/// what I and O are by looking at the generic type and the actual type.
/// 
/// Stops at first error. Could aggregate them if we wanted, but this should
/// be sufficient for now.
pub fn infer_generic_type(type_with_generics: &Type, actual_type: &Type, map: &mut HashMap<u8, Type>) -> Result<(), String> {
    match type_with_generics {
        Type::Generic(n) => {
            // then, whatever actual_type is, that's what we put in as the
            // mapping!
            // ... unless it is unknown.
            if let Type::Unknown = actual_type {
                // don't add the mapping!
                Ok(())
            } else {

                
                match map.insert(*n, actual_type.clone()) {
                    None => Ok(()),
                    Some(t) => if t == *actual_type {
                        Ok(())
                    } else {
                        Err("Multiple definitions for the generic.".to_string())
                    }
                }
            }
        },
        Type::Vec(generic_inner) => {
            if let Type::Vec(actual_inner) = actual_type {
                infer_generic_type(generic_inner, actual_inner, map)
            } else {
                // TODO error?! need diagnostics too for error reporting
                Err(format!("Generic expects Vec: {}, but actual did not have Vec and instead had: {}", type_with_generics, actual_type))
            }
        },
        Type::Tuple(generic_items) => {
            if let Type::Tuple(actual_items) = actual_type {
                // TODO error if sizes are different
                if generic_items.len() != actual_items.len() {
                    Err(format!("Generic expects Tuple of size {}, but got Tuple of size {}.", generic_items.len(), actual_items.len()))
                } else {
                    generic_items.iter().zip(actual_items.iter()).try_fold((), |_, (generic, actual)| infer_generic_type(generic, actual, map))
                }
            } else {
                // TODO error?! need diagnostics too for error reporting
                Err(format!("Generic expects Tuple: {}, but actual did not have Tuple and instead had: {}", type_with_generics, actual_type))
            }
        },
        Type::Option(generic_inner) => 
            if let Type::Option(actual_inner) = actual_type {
                infer_generic_type(generic_inner, actual_inner, map)
            } else {
                // TODO error?! need diagnostics too for error reporting
                Err(format!("Generic expects Option: {}, but actual did not have Option and instead had: {}", type_with_generics, actual_type))
            },
        Type::Function { params: generic_params, return_type: generic_return } => 
            if let Type::Function { params: actual_params, return_type: actual_return } = actual_type {
                if generic_params.len() != actual_params.len() {
                    Err(format!("Generic expects params to function of length {}, but got params to function of length {}.", generic_params.len(), actual_params.len()))
                } else {
                    generic_params.iter().zip(actual_params.iter()).try_fold((), |_, (generic, actual)| infer_generic_type(generic, actual, map)).and(infer_generic_type(generic_return, actual_return, map))
                }
                
            } else {
                Err(format!("Generic expects Function: {}, but actual did not have Function and instead had: {}", type_with_generics, actual_type))
            }
        ,
        _ => Ok(())
    }
}


#[cfg(test)]
mod tests {
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
}
