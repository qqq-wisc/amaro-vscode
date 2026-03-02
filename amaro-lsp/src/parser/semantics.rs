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
                let field_type = infer_expr_type(&field.value, &mut inf_data);

                // 3.1. Gate Validation in 'routed_gates' fields
                if block_name == "RouteInfo" && field.key == "routed_gates" {
                    validate_gates(&field.value, &mut diagnostics);
                }

                // 3.2. Enforce Float type on 'cost' field
                if field.key == "cost"
                    && field_type != Type::Unknown
                    && !types_compatible(&field_type, &Type::Float)
                {
                    diagnostics.push(Diagnostic {
                        range: field.value.range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: format!(
                            "'cost' must return Float, got '{}'. \
                             Hint: Comparisons return Bool — use arithmetic instead.",
                            field_type
                        ),
                        ..Default::default()
                    });
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
                        "'{}' is not a recognized standard gate. Expected one of: {}",
                        name,
                        valid_gates.join(", ")
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
                                    "Argument {} expected type '{}' but got '{}'.",
                                    i + 1,
                                    type_display(param_type),
                                    type_display(&arg_type)
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
                Type::Struct { fields, .. } => fields.get(field).cloned().unwrap_or(Type::Unknown),
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

                    // Trap topology (ion trap architectures)
                    "trap_positions" => Type::Vec(Box::new(Type::Location)),
                    "trap_vertices" => Type::Function {
                        params: vec![],
                        return_type: Box::new(Type::Vec(Box::new(Type::Location))),
                    },
                    "trap_edges" => Type::Vec(Box::new(Type::Tuple(vec![
                        Type::Location,
                        Type::Location,
                    ]))),
                    "locations" => Type::Function {
                        params: vec![],
                        return_type: Box::new(Type::Vec(Box::new(Type::Location))),
                    },
                    "edges_between" => Type::Function {
                        params: vec![
                            Type::Vec(Box::new(Type::Location)),
                            Type::Vec(Box::new(Type::Location)),
                        ],
                        return_type: Box::new(Type::Vec(Box::new(Type::Tuple(vec![
                            Type::Location,
                            Type::Location,
                        ])))),
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
                    Some(t) => t,
                    None => {
                        inference_data.diagnostics.push(Diagnostic {
                            range: expr.range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            
                            message: format!(
                                "Cannot use operation {:?} on types {} and {}.",
                                op, left_type, right_type
                            ),
                            ..Default::default()
                        });
                        Type::Unknown
                    }
                },
                BinaryOperator::Ge
                | BinaryOperator::Le
                | BinaryOperator::Gt
                | BinaryOperator::Lt => match types_math(&left_type, &right_type) {
                    Some(_) => Type::Bool,
                    None => {
                        inference_data.diagnostics.push(Diagnostic {
                            range: expr.range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            
                            message: format!(
                                "Cannot use operation {:?} on types {} and {}.",
                                op, left_type, right_type
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

                BinaryOperator::And | BinaryOperator::Or => match (&left_type, &right_type) {
                    (Type::Unknown | Type::Bool, Type::Bool | Type::Unknown) => Type::Bool,
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
                        Type::Unknown
                    }
                },

                BinaryOperator::Tensor => Type::Unknown,
                BinaryOperator::Range => Type::Unknown,
            }
        }
        ExprKind::UnaryOp { op, operand } => {
            let operand_type = infer_expr_type(operand, inference_data);
            match op {
                UnaryOperator::Not => match operand_type {
                    Type::Bool => Type::Bool,
                    Type::Unknown => Type::Unknown,
                    _ => {
                        inference_data.diagnostics.push(Diagnostic {
                            range: expr.range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            message: "Cannot perform NOT operation on non-bool type.".to_string(),
                            ..Default::default()
                        });
                        Type::Unknown
                    }
                },
                UnaryOperator::Neg => match operand_type {
                    Type::Int => Type::Int,
                    Type::Float => Type::Float,
                    Type::Unknown => Type::Unknown,
                    _ => {
                        inference_data.diagnostics.push(Diagnostic {
                            range: expr.range,
                            severity: Some(DiagnosticSeverity::ERROR),
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
                                
                                message: "Index of projection was out-of-bounds for the tuple."
                                    .to_string(),
                                ..Default::default()
                            });
                            Type::Unknown
                        }
                    }
                },
                Type::Unknown => Type::Unknown,
                _ => {
                    inference_data.diagnostics.push(Diagnostic {
                        range: expr.range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        
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
            if matches!(t2, Type::Int) || t1 == t2 {
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
#[derive(PartialEq, Debug)]
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
            ..Default::default()
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
            // TODO identify additional functions that we want for Option
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
            })
        }
        Type::Unknown => None,
        Type::Generic(_) => None,
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
            type_map: &mut HashMap::new(),
            user_def_table: &UserDefTable::empty(),
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
        println!("{:?}", inf_data.diagnostics);
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
            type_map: &mut HashMap::new(),
            user_def_table: &UserDefTable::empty(),
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
            type_map: &mut HashMap::new(),
            user_def_table: &UserDefTable::empty(),
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
        assert_eq!(
            arch_res,
            vec![
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
                }
            ]
        );

        let vec_res = suggest_next_from_type(&Type::Vec(Box::new(Type::QubitMap)), &user_def_table);
        assert!(vec_res.is_some());
        let vec_res = vec_res.unwrap();
        assert_eq!(
            vec_res,
            vec![
                Suggestion {
                    completion_text: "push".to_string(),
                    completion_type: Type::Function {
                        params: vec![Type::QubitMap],
                        return_type: Box::new(Type::Vec(Box::new(Type::QubitMap))),
                    },
                },
                Suggestion {
                    completion_text: "pop".to_string(),
                    completion_type: Type::Function {
                        params: vec![],
                        return_type: Box::new(Type::Option(Box::new(Type::QubitMap))),
                    },
                },
                Suggestion {
                    completion_text: "extend".to_string(),
                    completion_type: Type::Function {
                        params: vec![Type::Vec(Box::new(Type::QubitMap))],
                        return_type: Box::new(Type::Vec(Box::new(Type::QubitMap))),
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
                        params: vec![Type::QubitMap],
                        return_type: Box::new(Type::Bool),
                    },
                },
                Suggestion {
                    completion_text: "len".to_string(),
                    completion_type: Type::Int,
                }
            ]
        );
    }
}
