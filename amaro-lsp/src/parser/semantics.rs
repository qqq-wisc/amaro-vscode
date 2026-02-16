use tower_lsp::lsp_types::{
    Diagnostic,
    DiagnosticSeverity,
    DiagnosticRelatedInformation,
    Location,
    Range,
    Url
};
use std::collections::HashMap;
use crate::ast::*;
use super::symbols::*;

// Semantic Analysis
pub fn check_semantics(file: &AmaroFile) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let known_blocks = [
        "GateRealization", "Transition", "Architecture", "Arch", "Step",
        "RouteInfo", "TransitionInfo", "ArchInfo", "StateInfo"
    ];

    let mut required_keys: HashMap<&str, Vec<&str>> = HashMap::new();
    required_keys.insert("RouteInfo", vec!["routed_gates", "realize_gate"]);
    required_keys.insert("TransitionInfo", vec!["get_transitions", "apply", "cost"]);
    required_keys.insert("ArchInfo", vec![]);
    required_keys.insert("StateInfo", vec![]);

    let mut found_blocks: HashMap<String, Range> = HashMap::new();

    // Block Level Validation
    for block in &file.blocks {
        let block_name = block.kind.as_str();
        let lower_name = block_name.to_lowercase();

        // 1. Capitalization Check
        if let Some(correct_name) = known_blocks.iter().find(|&&kb| kb.eq_ignore_ascii_case(block_name)) {
            if block_name != *correct_name {
                diagnostics.push(Diagnostic {
                    range: block.range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    message: format!("Block '{}' should be Capitalized (e.g., '{}').", block_name, correct_name),
                    ..Default::default()
                });
            }
        }

        // 2. Uniqueness Check
        if let Some(first_range) = found_blocks.get(&lower_name) {
            diagnostics.push(Diagnostic {
                range: block.range,
                severity: Some(DiagnosticSeverity::ERROR),
                message: format!("Duplicate definition of '{}' block.", block_name),
                related_information: Some(vec![
                    DiagnosticRelatedInformation {
                        location: Location {
                            uri: Url::parse("file:///previous/definition").unwrap_or_else(|_| Url::parse("file:///unknown").unwrap()),
                            range: *first_range
                        },
                        message: "First defined here".to_string()
                    }
                ]),
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
            if let BlockItem::Field(field) = item {                
                present_keys.push(field.key.as_str());
                infer_expr_type(&field.value, &mut sym_table, &mut diagnostics);

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
                        message: format!("Block '{}' is missing required field: '{}'", block_name, req),
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

fn validate_gates(expr: &Expr, diagnostics: &mut Vec<Diagnostic>) {
    let valid_gates = ["CX", "T", "Pauli", "PauliMeasurement"];

    match &expr.kind {
        ExprKind::Identifier(name) => {
            if !valid_gates.contains(&name.as_str()) {
                diagnostics.push(Diagnostic {
                    range: expr.range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    message: format!("'{}' is not a recognized standard gate. Expected one of: {:?}", name, valid_gates),
                    ..Default::default()
                });
            }
        },
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            for item in items {
                validate_gates(item, diagnostics);
            }
        },
        _ => {}
    }
}

// Type Inference Engine
pub fn infer_expr_type(expr: &Expr, sym_table: &mut SymbolTable, diagnostics: &mut Vec<Diagnostic>) -> Type {
    match &expr.kind {
        ExprKind::IntLiteral(_) => Type::Int,
        ExprKind::FloatLiteral(_) => Type::Float,
        ExprKind::BoolLiteral(_) => Type::Bool,
        ExprKind::StringLiteral(_) => Type::String,
        ExprKind::None => Type::Option(Box::new(Type::Unknown)),

        ExprKind::Identifier(name) => {
            if matches!(name.as_str(), "CX" | "T" | "Pauli" | "PauliMeasurement") {
                return Type::Gate;
            }

            sym_table.lookup(name).cloned().unwrap_or_else(|| {
                diagnostics.push(Diagnostic {
                    range: expr.range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: format!("Undefined variable '{}'.", name),
                    ..Default::default()
                });
                Type::Unknown
            })
        },

        ExprKind::List(items) => {
            if items.is_empty() {
                Type::Vec(Box::new(Type::Unknown))
            } else {
                let first_type = infer_expr_type(&items[0], sym_table, diagnostics);
                for item in &items[1..] {
                    let item_type = infer_expr_type(item, sym_table, diagnostics);
                    if item_type != first_type {
                        diagnostics.push(Diagnostic {
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
        },

        ExprKind::Tuple(items) => {
            Type::Tuple(items.iter()
                .map(|e| infer_expr_type(e, sym_table, diagnostics))
                .collect())
        },

        ExprKind::Some(inner) => {
            let inner_type = infer_expr_type(inner, sym_table, diagnostics);
            Type::Option(Box::new(inner_type))
        },

        ExprKind::Lambda { params, body } => {
            sym_table.enter_scope();
            let mut param_types = Vec::new();
            for param in params {
                sym_table.bind(param.clone(), Type::Unknown);
                param_types.push(Type::Unknown);
            }
            let return_type = infer_expr_type(body, sym_table, diagnostics);
            sym_table.exit_scope();

            Type::Function {
                params: param_types,
                return_type: Box::new(return_type),
            }
        },

        ExprKind::LetBinding { name, value, body } => {
            sym_table.enter_scope();
            let value_type = infer_expr_type(value, sym_table, diagnostics);
            sym_table.bind(name.clone(), value_type);
            let body_type = infer_expr_type(body, sym_table, diagnostics);
            sym_table.exit_scope();
            body_type
        },

        ExprKind::IfThenElse { condition, then_branch, else_branch } => {
            let cond_type = infer_expr_type(condition, sym_table, diagnostics);
            if !types_compatible(&cond_type, &Type::Bool) {
                diagnostics.push(Diagnostic {
                    range: condition.range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: "Condition in if-then-else must be of type 'Bool'.".to_string(),
                    ..Default::default()
                });
            }

            let then_type = infer_expr_type(then_branch, sym_table, diagnostics);
            let else_type = infer_expr_type(else_branch, sym_table, diagnostics);

            if !types_compatible(&then_type, &else_type) {
                diagnostics.push(Diagnostic {
                    range: expr.range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: "Then and else branches of if-then-else must have compatible types.".to_string(),
                    ..Default::default()
                });
            }
            then_type  
        },

        ExprKind::FunctionCall { function, args } => {
            let func_type = infer_expr_type(function, sym_table, diagnostics);
            match func_type {
                Type::Function { params, return_type } => {
                    if params.len() != args.len() {
                        diagnostics.push(Diagnostic {
                            range: expr.range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            message: format!("Expected {} arguments but got {}.", params.len(), args.len()),
                            ..Default::default()
                        });
                        return *return_type;
                    }
                    for (i, (param_type, arg)) in params.iter().zip(args).enumerate() {
                        let arg_type = infer_expr_type(arg, sym_table, diagnostics);
                        
                        // Following Logic
                        // 1. If param_type Unknown, Accept
                        // 2. If arg_type Unknown, Accept (Avoid Cascading Errors)
                        // 3. Otherwise, Check Compatibility
                        if *param_type != Type::Unknown && arg_type != Type::Unknown && !types_compatible(param_type, &arg_type) {
                            diagnostics.push(Diagnostic {
                                range: arg.range,
                                severity: Some(DiagnosticSeverity::ERROR),
                                message: format!("Argument {} expected type '{:?}' but got '{:?}'.", i + 1, param_type, arg_type),
                                ..Default::default()
                            });
                        }
                    }
                    *return_type
                },
                Type::Unknown => Type::Unknown, // Avoid Cascading Errors
                _ => {
                    diagnostics.push(Diagnostic {
                        range: function.range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: "Attempted to call a non-function value.".to_string(),
                        ..Default::default()
                    });
                    Type::Unknown
                }
            }
        },
        
        ExprKind::FieldAccess { object, field } => {
            let obj_type = infer_expr_type(object, sym_table, diagnostics);
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
                            return_type: Box::new(Type::Option(inner.clone()))
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
                },
                Type::Struct { fields, .. } => {
                    fields.get(field).cloned().unwrap_or(Type::Unknown)
                },
                Type::Tuple(elements) => {
                    if let Ok(idx) = field.parse::<usize>() {
                        elements.get(idx).cloned().unwrap_or(Type::Unknown)
                    } else {
                        Type::Unknown
                    }
                },

                // Built-in Types
                Type::ArchT => {
                    match field.as_str() {
                        "width" | "height" => Type::Int,
                        "edges" => Type::Function {
                            params: vec![],
                            return_type: Box::new(Type::Vec(Box::new(Type::Tuple(vec![Type::Location, Type::Location])))),
                        },
                        "succ_rates" => Type::Vec(Box::new(Type::Vec(Box::new(Type::Float)))),
                        "contains_edge" => Type::Function {
                            params: vec![Type::Tuple(vec![Type::Location, Type::Location])],
                            return_type: Box::new(Type::Bool),
                        },
                        "magic_state_qubits" | "alg_qubits" => Type::Function {
                            params: vec![],
                            return_type: Box::new(Type::Vec(Box::new(Type::Location)))
                        },
                        _ => Type::Unknown,
                    }
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
                },
                Type::Gate => {
                    match field.as_str() {
                        "qubits" => Type::Vec(Box::new(Type::Qubit)),
                        "gate_type" => Type::Function {
                            params: vec![],
                            return_type: Box::new(Type::Gate),
                        },
                        "implementation" => Type::Unknown,
                        _ => Type::Unknown,
                    }
                },
                Type::Unknown => Type::Unknown,
                _ => Type::Unknown,
            }
        },

        ExprKind::StructLiteral { name, fields } => {
            let mut field_types = HashMap::new();
            for (key, value) in fields {
                let val_type = infer_expr_type(value, sym_table, diagnostics);
                field_types.insert(key.clone(), val_type);
            }
            Type::Struct {
                name: name.clone(),
                fields: field_types,
            }
        },

        ExprKind::IndexAccess { object, index } => {
            let obj_type = infer_expr_type(object, sym_table, diagnostics);
            let idx_type = infer_expr_type(index, sym_table, diagnostics);

            if obj_type == Type::Unknown {
                return Type::Unknown;
            }

            let expected_idx_type = match &obj_type {
                Type::Vec(_) => Type::Int,
                Type::QubitMap => Type::Qubit,
                Type::Function { params, return_type } if params.is_empty() => {
                    match return_type.as_ref() {
                        Type::QubitMap => Type::Qubit,
                        _ => Type::Int,
                    }
                }
                _ => Type::Int,
            };

            if idx_type != Type::Unknown && !types_compatible(&idx_type, &expected_idx_type) {
                diagnostics.push(Diagnostic {
                    range: index.range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: format!("Index type mismatch. Expected '{:?}' but got '{:?}'.", expected_idx_type, idx_type),
                    ..Default::default()
                });
            }

            // If object is a 0-arg function, auto-call it
            let actual_type = match obj_type {
                Type::Function { params, return_type } if params.is_empty() => *return_type,
                other => other,
            };

            match actual_type {
                Type::Vec(inner) => *inner,
                Type::QubitMap => Type::Location,
                Type::Unknown => Type::Unknown,
                _ => {
                    diagnostics.push(Diagnostic {
                        range: object.range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: "Attempted to index a non-indexable type.".to_string(),
                        ..Default::default()
                    });
                    Type::Unknown
                }
            }
        },

        _ => Type::Unknown,
    }
}

fn types_compatible(t1: &Type, t2: &Type) -> bool {
    // Avoid Cascading errors
    if matches!(t1, Type::Unknown) || matches!(t2, Type::Unknown) {
        return true;
    }

    match (t1, t2) {
        (Type::Int, Type::Int) |
        (Type::Float, Type::Float) |
        (Type::Bool, Type::Bool) |
        (Type::String, Type::String) |
        (Type::Location, Type::Location) |
        (Type::Qubit, Type::Qubit) |
        (Type::QubitMap, Type::QubitMap) |
        (Type::Gate, Type::Gate) => true,

        // Numeric leniency
        (Type::Int, Type::Float) => true, 
        (Type::Float, Type::Int) => true,

        (Type::ArchT, Type::ArchT) => true,
        (Type::StateT, Type::StateT) => true,

        (Type::Vec(inner1), Type::Vec(inner2)) => types_compatible(inner1, inner2),
        (Type::Tuple(items1), Type::Tuple(items2)) => {
            items1.len() == items2.len() && items1.iter().zip(items2).all(|(a, b)| types_compatible(a, b))
        },
        (Type::Option(inner1), Type::Option(inner2)) => types_compatible(inner1, inner2),

        (Type::Function { params: p1, return_type: r1 }, Type::Function { params: p2, return_type: r2 }) => {
            p1.len() == p2.len() && p1.iter().zip(p2).all(|(a, b)| types_compatible(a, b)) && types_compatible(r1, r2)
        },

        _ => false,
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
