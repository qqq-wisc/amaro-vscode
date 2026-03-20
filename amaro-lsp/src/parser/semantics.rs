use super::symbols::*;
use crate::{ast::*, info::builtins};
use std::collections::HashMap;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemLabelDetails, Diagnostic, DiagnosticRelatedInformation,
    DiagnosticSeverity, Location, Range, Url,
};

/// Performs semantic analysis on a parsed Amaro file.
///
/// Validates block structure, required fields, and type correctness.
/// Returns diagnostics for LSP clients.
pub fn check_semantics(file: &AmaroFile) -> (Vec<Diagnostic>, HashMap<NodeId, Type>, UserDefTable) {
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
    let mut type_map = TypeMap::new();

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

    (diagnostics, type_map.map, user_def_table)
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

pub struct TypeMap {
    map: HashMap<NodeId, Type>,
}

impl Default for TypeMap {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeMap {
    pub fn new() -> TypeMap {
        TypeMap {
            map: HashMap::new(),
        }
    }
    /// HashMap get for compatibility
    pub fn get(&self, id: &NodeId) -> Option<&Type> {
        self.map.get(id)
    }

    /// Sets the value in the map.
    /// overlay is preferred, as this will set the value to the new type,
    /// not overlay it.
    pub fn set(&mut self, id: NodeId, typ: Type) -> Option<Type> {
        self.map.insert(id, typ)
    }

    /// Updates the type in the type map
    /// If no type exists, then simply puts the type in there.
    /// If a type does already exist, then attempts to just overlay the generics
    /// and unknowns, leaving the remaining structure unchanged, in an attempt
    /// to make a more specific type.
    pub fn overlay(&mut self, id: NodeId, new_type: &Type) {
        match self.map.get_mut(&id) {
            Some(prev_type) => overlay_type(prev_type, new_type),
            None => {
                self.set(id, new_type.clone());
            }
        }
    }
}

/// Overlays one type onto another.
/// When we "overlay" a type, we are providing more specific information about
/// the background type. If the background type contains a generic or unknown
/// in the same place that the foreground type contains something real, then
/// the background type has its values replaced with those in the foreground
/// type, so that we produce a strictly more-specific type.
///
/// Think of this like in image editing, where we overlay a foreground onto
/// a background. Generics and unknowns are transparent pixels.
pub fn overlay_type(background_type: &mut Type, foreground_type: &Type) {
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
        | Type::UserDef(_) => {}
        Type::Vec(b_inner) => {
            if let Type::Vec(f_inner) = foreground_type {
                overlay_type(b_inner, f_inner);
            }
        }
        Type::Tuple(b_items) => {
            if let Type::Tuple(f_items) = foreground_type {
                b_items
                    .iter_mut()
                    .zip(f_items.iter())
                    .for_each(|elt| overlay_type(elt.0, elt.1));
            }
        }
        Type::Option(b_inner) => {
            if let Type::Option(f_inner) = foreground_type {
                overlay_type(b_inner, f_inner);
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
                b_params
                    .iter_mut()
                    .zip(f_params.iter())
                    .for_each(|elt| overlay_type(elt.0, elt.1));
                overlay_type(b_ret, f_ret);
            }
        }
        Type::Generic(_) => {
            if !matches!(foreground_type, Type::Unknown) {
                *background_type = foreground_type.clone();
            }
        }
        Type::Unknown => {
            *background_type = foreground_type.clone();
        }
    }
}

/// Aggregate of the args passed to infer_expr_type,
/// so we can easily change these without having to change 10000 call signatures
pub struct InferenceData<'a> {
    pub sym_table: &'a mut SymbolTable,
    pub diagnostics: &'a mut Vec<Diagnostic>,
    pub type_map: &'a mut TypeMap,
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
            // always first look up in symbol table
            inference_data
                .sym_table
                .lookup(name)
                .cloned()
                .unwrap_or_else(|| {
                    // if not in symbol table, check if we already have given this
                    // expression a type
                    inference_data
                        .type_map
                        .get(&expr.id)
                        .cloned()
                        .unwrap_or_else(|| {
                            // check built-ins

                            builtins::get_raw_built_in(name.as_str())
                                .map(|elt| elt.typ.clone())
                                .unwrap_or_else(|| {
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
                        })
                })
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
            // with lambda:
            // sometimes we will be rerunning this function to determine generics
            // if we do, then we provide some information about the lambda and use this
            // to gain additional information about the lambda.

            // LS TODO: Repeat code. Make it cleaner
            if let Some(Type::Function {
                params: fcn_params, ..
            }) = inference_data.type_map.get(&expr.id)
            {
                inference_data.sym_table.enter_scope();

                let mut param_types = Vec::new();
                for (param_name, param_type) in params.iter().zip(fcn_params) {
                    inference_data
                        .sym_table
                        .bind(param_name.clone(), param_type.clone());
                    param_types.push(param_type.clone());
                }

                let found_return_type = infer_expr_type(body, inference_data);
                inference_data.sym_table.exit_scope();

                Type::Function {
                    params: param_types,
                    return_type: Box::new(found_return_type),
                }
            } else {
                // do the normal way if no mapping exists (or if mapping is bizarre)
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
            infer_expr_type(function, inference_data);
            let fcn_type = (*inference_data.type_map.get(&function.id).as_ref().unwrap()).clone(); // always there
            match &fcn_type {
                Type::Unknown => Type::Unknown,
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
                        (**return_type).clone()
                    } else if contains_generic(&fcn_type) {
                        // need to do some generic stuff

                        // 1. get inferred types of all args and return type
                        // 2. using inferred types, try to build generic map as best as can
                        // 3. retype all the expressions using generic map as best as can
                        // 4. if still contains generic type, return to 1

                        let mut generic_map = HashMap::new();
                        let inferred_args: Vec<Type> = args
                            .iter()
                            .map(|elt| infer_expr_type(elt, inference_data))
                            .collect();
                        let outcome = inferred_args
                            .iter()
                            .zip(params)
                            .map(|(actual_arg, generic_param)| {
                                infer_generic_type(generic_param, actual_arg, &mut generic_map)
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
                        }

                        if generic_map.is_empty() {
                            // cannot resolve any more generics
                            // end normally
                            (**return_type).clone()
                        } else {
                            // degenerisize now
                            for (param_type, arg) in params.iter().zip(args) {
                                let (_, t) = degenerisize(param_type, &generic_map);
                                retype(arg, t, inference_data);
                            }

                            // degenerisize whole function
                            let (_, t) = degenerisize(&fcn_type, &generic_map);
                            retype(function, t.clone(), inference_data);

                            // then, infer again on this expr to see if we're done
                            // should be slowly removing generic types i think

                            return infer_expr_type(expr, inference_data);
                            // } else {
                            //     (**return_type).clone()
                            // }
                        }
                    } else {
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

                        (**return_type).clone()
                    }
                }
                _ => {
                    inference_data.diagnostics.push(Diagnostic {
                        range: expr.range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: "Could not determine type of function.".to_string(),

                        ..Default::default()
                    });
                    Type::Unknown
                }
            }
        }
        ExprKind::FieldAccess { object, field } => {
            let obj_type = infer_expr_type(object, inference_data);

            if matches!(obj_type, Type::Unknown) {
                Type::Unknown
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

                match type_from_user_def {
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
                }
            }

            // TODO remove this if it isn't useful!

            // Type::Tuple(elements) => {
            //     if let Ok(idx) = field.parse::<usize>() {
            //         elements.get(idx).cloned().unwrap_or(Type::Unknown)
            //     } else {
            //         Type::Unknown
            //     }
            // }
        }

        ExprKind::StructLiteral { name, fields } => {
            // the only valid struct literals are those of UserDef, right?
            // im going to go with that
            // LS: Determine if that is accurate
            for (_, value) in fields {
                infer_expr_type(value, inference_data);
            }

            if inference_data.user_def_table.get_fields(name).is_some() {
                Type::UserDef(name.clone())
            } else {
                inference_data.diagnostics.push(Diagnostic {
                    range: expr.range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: "This struct literal does not match any defined struct types."
                        .to_string(),
                    ..Default::default()
                });
                Type::Unknown
            }

            // let mut field_types = HashMap::new();
            // for (key, value) in fields {
            //     let val_type = infer_expr_type(value, inference_data);
            //     field_types.insert(key.clone(), val_type);
            // }
            // Type::Struct {
            //     name: name.clone(),
            //     fields: field_types,
            // }
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
                                "Cannot use operation '{}' on types {} and {}.",
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
                                "Cannot use operation '{}' on types {} and {}.",
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
        ExprKind::Match { scrutinee, arms } => {
            let _scrutinee_type = infer_expr_type(scrutinee, inference_data);

            if arms.is_empty() {
                return Type::Unknown;
            }

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

            first_type
        }
        ExprKind::Projection { index, tuple } => {
            // first, get type of the tuple
            let tuple_type = infer_expr_type(tuple, inference_data);
            match tuple_type {
                Type::Tuple(types) => match types.get(*index) {
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
    inference_data.type_map.overlay(expr.id, &found_type);

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

/// Suppose we have a type which includes some generics.
/// Knowing the actual type, we can create a mapping from generic types
/// to their actual types.
/// For instance, map<I,O>(|O| -> I, Vec<I>) has two generics. We want to be
/// able to infer what I and O are by looking at the generic type and the actual type.
///
/// This does its best without modifying any types or looking at the expressions.
/// Meaning, it cannot make all the inferences alone, and may need to be re-ran
/// if there are complex structures at play.
///
/// Stops at first error. Could aggregate them if we wanted, but this should
/// be sufficient for now.
///
/// TODO: Returning the string is good for readability, however this make the fcn HEAVY when used
/// on things that we aren't sure will match.
/// TODO: Think there may be a bug with this, if there are some kind of nested generics.
pub fn infer_generic_type(
    type_with_generics: &Type,
    actual_type: &Type,
    map: &mut HashMap<u8, Type>,
) -> Result<(), String> {
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
                    Some(t) => {
                        if t == *actual_type {
                            Ok(())
                        } else {
                            Err("Multiple definitions for the generic.".to_string())
                        }
                    }
                }
            }
        }
        Type::Vec(generic_inner) => {
            if let Type::Vec(actual_inner) = actual_type {
                infer_generic_type(generic_inner, actual_inner, map)
            } else {
                // TODO error?! need diagnostics too for error reporting
                Err(format!(
                    "Generic expects Vec: {}, but actual did not have Vec and instead had: {}",
                    type_with_generics, actual_type
                ))
            }
        }
        Type::Tuple(generic_items) => {
            if let Type::Tuple(actual_items) = actual_type {
                // TODO error if sizes are different
                if generic_items.len() != actual_items.len() {
                    Err(format!(
                        "Generic expects Tuple of size {}, but got Tuple of size {}.",
                        generic_items.len(),
                        actual_items.len()
                    ))
                } else {
                    generic_items
                        .iter()
                        .zip(actual_items.iter())
                        .try_fold((), |_, (generic, actual)| {
                            infer_generic_type(generic, actual, map)
                        })
                }
            } else {
                // TODO error?! need diagnostics too for error reporting
                Err(format!(
                    "Generic expects Tuple: {}, but actual did not have Tuple and instead had: {}",
                    type_with_generics, actual_type
                ))
            }
        }
        Type::Option(generic_inner) => {
            if let Type::Option(actual_inner) = actual_type {
                infer_generic_type(generic_inner, actual_inner, map)
            } else {
                // TODO error?! need diagnostics too for error reporting
                Err(format!(
                    "Generic expects Option: {}, but actual did not have Option and instead had: {}",
                    type_with_generics, actual_type
                ))
            }
        }
        Type::Function {
            params: generic_params,
            return_type: generic_return,
        } => {
            if let Type::Function {
                params: actual_params,
                return_type: actual_return,
            } = actual_type
            {
                if generic_params.len() != actual_params.len() {
                    Err(format!(
                        "Generic expects params to function of length {}, but got params to function of length {}.",
                        generic_params.len(),
                        actual_params.len()
                    ))
                } else {
                    generic_params
                        .iter()
                        .zip(actual_params.iter())
                        .try_fold((), |_, (generic, actual)| {
                            infer_generic_type(generic, actual, map)
                        })
                        .and(infer_generic_type(generic_return, actual_return, map))
                }
            } else {
                Err(format!(
                    "Generic expects Function: {}, but actual did not have Function and instead had: {}",
                    type_with_generics, actual_type
                ))
            }
        }
        _ => {
            if type_with_generics == actual_type {
                Ok(())
            } else {
                Err("Types did not match".to_string())
            }
        }
    }
}

pub fn contains_unknown(typ: &Type) -> bool {
    match typ {
        Type::Int => false,
        Type::Float => false,
        Type::Bool => false,
        Type::String => false,
        Type::Location => false,
        Type::Qubit => false,
        Type::QubitMap => false,
        Type::Gate => false,
        Type::ArchT => false,
        Type::StateT => false,
        Type::InstrT => false,
        Type::Vec(inner) => contains_unknown(inner),
        Type::Tuple(items) => items.iter().any(contains_unknown),
        Type::Option(inner) => contains_unknown(inner),
        Type::Function {
            params,
            return_type,
        } => params.iter().any(contains_unknown) || contains_unknown(return_type),
        Type::UserDef(_) => false,
        Type::Generic(_) => false,
        Type::Unknown => true,
    }
}

pub fn contains_generic(typ: &Type) -> bool {
    match typ {
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
        Type::Vec(t) => contains_generic(t),
        Type::Tuple(items) => items.iter().any(contains_generic),
        Type::Option(t) => contains_generic(t),
        Type::Function {
            params,
            return_type,
        } => params.iter().any(contains_generic) || contains_generic(return_type),
        Type::Generic(_) => true,
    }
}

/// Given an expression, changes its type to a new type. Does so recursively.
/// This is helpful for if we have some type that uses generics where we cannot
/// determine the type, then later discover the generic type.
/// For instance, if we have something with the type "|?| -> Vec<?>", we can
/// come back and tell it that it is actually "|Int| -> Vec<Float>". It will
/// recursively (as best it can) retype all the subexpressions to match this
/// format.
/// Doesn't retype identifiers...
pub fn retype(expr: &Expr, new_type: Type, inference_data: &mut InferenceData) {
    // eprintln!("  Range: {:?}", expr.range);
    if let Some(old_type) = inference_data.type_map.get(&expr.id) {
        eprintln!("Retyping {} from {} to {}", expr.kind, old_type, new_type);
    } else {
        eprintln!(":::HEY! Retyping {} to {}, but no old type existed!", expr.kind, new_type);
    }
    
    inference_data.type_map.overlay(expr.id, &new_type);
    
    // TODO: Can infer function return type this way, but don't have a good
    // method for doing this right now.
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
            // nothing special to do here
        }
        ExprKind::FieldAccess { .. } => {
            // nothing special to do here
        }
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
        ExprKind::BinaryOp { .. } => todo!(),
        ExprKind::UnaryOp { .. } => todo!(),
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

/// Given a type that we know has generics, and a generic map which maps from
/// the generic IDs to their actual types, reconstructs the actual type by
/// substituting in the generics with the types in the map.
/// If the first entry in the tuple is false, then there are still unresolved
/// generics. It likely means that there is a dependency of one generic on
/// another, which can (hopefully) be resolved by making substitutions and then
/// repeating the generic inference process.
pub fn degenerisize(type_with_generics: &Type, generic_map: &HashMap<u8, Type>) -> (bool, Type) {
    match type_with_generics {
        Type::Generic(c) => match generic_map.get(c) {
            Some(t) => (true, t.clone()),
            None => (false, Type::Generic(*c)),
        },
        Type::Vec(inner) => {
            let contents = degenerisize(inner, generic_map);
            (contents.0, Type::Vec(Box::new(contents.1)))
        }
        Type::Tuple(items) => {
            let results = items.iter().map(|elt| degenerisize(elt, generic_map));
            let done = results.clone().all(|b| b.0);

            (done, Type::Tuple(results.map(|elt| elt.1).collect()))
        }
        Type::Option(inner) => {
            let contents = degenerisize(inner, generic_map);
            (contents.0, Type::Option(Box::new(contents.1)))
        }
        Type::Function {
            params,
            return_type,
        } => {
            let params_results = params.iter().map(|elt| degenerisize(elt, generic_map));
            let return_type_results = degenerisize(return_type, generic_map);
            let done = params_results.clone().all(|b| b.0) && return_type_results.0;

            (
                done,
                Type::Function {
                    params: params_results.map(|elt| elt.1).collect(),
                    return_type: Box::new(return_type_results.1),
                },
            )
        }
        Type::Unknown => (true, Type::Unknown),
        _ => (true, type_with_generics.clone()),
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
