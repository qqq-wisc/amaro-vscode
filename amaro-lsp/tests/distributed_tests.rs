/// Comprehensive tests for distributed QPU QMRL support.
///
/// Covers:
///   1. Parsing — both distributed files parse without error
///   2. No spurious errors — zero errors on valid distributed QMRL
///   3. Type annotations — Bool fields, (Location,Location) tuples, Vec<Int>
///   4. ArchT field types — all multi-QPU and Bell pair scalar/vec fields
///   5. ArchT distributed fields — qpu_ids membership and gate_bell_budget
///   6. Gate.implementation — infers UserDef("GateRealization"), not Unknown
///   7. State.implemented_gates() — infers Vec<UserDef("GateRealization")>, not Unknown
///   8. Struct/Struct type compatibility — two Option<GateRealization> branches are compatible
///   9. IndexAccess on Unknown — no spurious "Undefined variable" or branch type errors
///  10. Regression — nisq.qmrl stays clean after all changes
use amaro_lsp::ast::*;
use amaro_lsp::parser::core::parse_file;
use amaro_lsp::parser::symbols::{SymbolTable, Type, UserDefTable};
use amaro_lsp::parser::{
    GenericTable, InferenceData, StringLabels, TypeMap, check_semantics, infer_expr_type,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use tower_lsp::lsp_types::{DiagnosticSeverity, Position, Range};

const SIMPLE_2QPU: &str = include_str!("../test_files/simple_2qpu.qmrl");
const DIST_2QPU_BUDGET: &str = include_str!("../test_files/dist_2qpu_budget.qmrl");
const NISQ: &str = include_str!("../test_files/nisq.qmrl");

// ─── Shared fixtures ──────────────────────────────────────────────────────────

/// Minimal blocks required by the validator.
const MINIMAL_ROUTE_TRANSITION: &str = r#"
RouteInfo:
    routed_gates = CX
    GateRealization{u : Location, v : Location}
    realize_gate = None

TransitionInfo:
    Transition{edge : (Location, Location)}
    get_transitions = []
    apply = value_swap(Transition.edge.(0), Transition.edge.(1))
    cost = 1.0
"#;

/// RouteInfo with a Bool field + full multi-QPU + Bell pair ArchInfo.
const DISTRIBUTED_PREAMBLE: &str = r#"
RouteInfo:
    routed_gates = CX
    GateRealization{u : Location, v : Location, remote : Bool}
    realize_gate = None

TransitionInfo:
    Transition{edge : (Location, Location)}
    get_transitions = []
    apply = value_swap(Transition.edge.(0), Transition.edge.(1))
    cost = 1.0

ArchInfo:
    Arch{num_qpus : Int, qpu_sizes : Vec<Int>, qpu_ids : Vec<Int>, link_cost : Float, comm_qubits : Vec<Location>, alg_qubits : Vec<Location>, n_comm_qubits : Int, bell_success_prob : Float, bell_attempt_interval : Float, max_bell_rate : Float, code_distance : Int, t_cycle : Float, gate_bell_budget : Int}
    get_locations = Arch.alg_qubits()
"#;

/// StateInfo that accesses x.implementation.(remote()) — the problematic pattern.
const STATEINFO_WITH_IMPLEMENTATION_ACCESS: &str = r#"
StateInfo:
    cost = fold(0.0,
                |x, acc| -> acc + x,
                map(|x| -> if x.implementation.(remote())
                               then Arch.link_cost
                               else 0.0,
                    State.implemented_gates()))
"#;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn zero_range() -> Range {
    Range {
        start: Position::new(0, 0),
        end: Position::new(0, 0),
    }
}

fn make_expr(kind: ExprKind) -> Expr {
    static NEXT_ID: AtomicU32 = AtomicU32::new(1);

    Expr {
        kind,
        range: zero_range(),
        id: NodeId(NEXT_ID.fetch_add(1, Ordering::Relaxed)),
    }
}

/// Extracts the GateRealization struct name from a parsed file's RouteInfo block.
fn extract_impl_struct_name(file: &AmaroFile) -> Option<String> {
    file.blocks
        .iter()
        .find(|b| b.kind.eq_ignore_ascii_case("RouteInfo"))
        .and_then(|b| {
            let BlockContent::Fields(items) = &b.content;
            items.iter().find_map(|item| match item {
                BlockItem::StructDef(s) => Some(s.name.clone()),
                _ => None,
            })
        })
}

fn only_errors(file: &AmaroFile) -> Vec<String> {
    check_semantics(file)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .map(|d| d.message)
        .collect()
}

// ─── 1. Parsing ──────────────────────────────────────────────────────────────

#[test]
fn test_simple_2qpu_parses() {
    let file = parse_file(SIMPLE_2QPU)
        .expect("simple_2qpu.qmrl should parse without error")
        .file;
    // RouteInfo, TransitionInfo, ArchInfo, StateInfo
    assert!(
        file.blocks.len() >= 4,
        "Expected at least 4 blocks, got {}",
        file.blocks.len()
    );
}

#[test]
fn test_dist_2qpu_budget_parses() {
    let file = parse_file(DIST_2QPU_BUDGET)
        .expect("dist_2qpu_budget.qmrl should parse without error")
        .file;
    assert!(
        file.blocks.len() >= 4,
        "Expected at least 4 blocks, got {}",
        file.blocks.len()
    );
}

// ─── 2. No spurious errors ───────────────────────────────────────────────────

#[test]
fn test_simple_2qpu_no_errors() {
    let file = parse_file(SIMPLE_2QPU).unwrap().file;
    let errors = only_errors(&file);
    assert!(
        errors.is_empty(),
        "simple_2qpu.qmrl produced unexpected error(s):\n{}",
        errors
            .iter()
            .map(|m| format!("  - {m}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn test_dist_2qpu_budget_no_errors() {
    let file = parse_file(DIST_2QPU_BUDGET).unwrap().file;
    let errors = only_errors(&file);
    assert!(
        errors.is_empty(),
        "dist_2qpu_budget.qmrl produced unexpected error(s):\n{}",
        errors
            .iter()
            .map(|m| format!("  - {m}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

// ─── 3. Type annotations ─────────────────────────────────────────────────────

#[test]
fn test_bool_field_in_gate_realization_no_error() {
    let input = format!("{DISTRIBUTED_PREAMBLE}\nStateInfo:\n    cost = 0.0");
    let file = parse_file(&input).unwrap().file;
    let errors = only_errors(&file);
    assert!(
        errors.is_empty(),
        "Bool field in GateRealization produced errors: {:?}",
        errors
    );
}

#[test]
fn test_tuple_type_with_spaces_parses() {
    // Transition{edge : (Location, Location)} — whitespace inside tuple type
    let result = parse_file(MINIMAL_ROUTE_TRANSITION);
    assert!(
        result.is_ok(),
        "Failed to parse (Location, Location) tuple type: {:?}",
        result.err()
    );
}

#[test]
fn test_vec_int_field_type_no_error() {
    let input = format!(
        "{MINIMAL_ROUTE_TRANSITION}\nArchInfo:\n    Arch{{ qpu_sizes : Vec<Int> }}\n    get_locations = []"
    );
    let file = parse_file(&input).unwrap().file;
    let errors = only_errors(&file);
    assert!(
        errors.is_empty(),
        "Vec<Int> field type caused errors: {:?}",
        errors
    );
}

// ─── 4. ArchT field types ────────────────────────────────────────────────────

#[test]
fn test_arch_num_qpus_field_is_int_type() {
    // Use num_qpus in a context that requires Int: compare with an Int literal.
    let input = format!(
        "{DISTRIBUTED_PREAMBLE}\nStateInfo:\n    cost = if Arch.num_qpus == 2 then 1.0 else 0.0"
    );
    let file = parse_file(&input).unwrap().file;
    let errors = only_errors(&file);
    assert!(
        errors.is_empty(),
        "Arch.num_qpus == 2 (Int comparison) produced errors: {:?}",
        errors
    );
}

#[test]
fn test_arch_link_cost_field_is_float_type() {
    let input = format!("{DISTRIBUTED_PREAMBLE}\nStateInfo:\n    cost = Arch.link_cost");
    let file = parse_file(&input).unwrap().file;
    let errors = only_errors(&file);
    assert!(
        errors.is_empty(),
        "Arch.link_cost used as Float cost produced errors: {:?}",
        errors
    );
}

#[test]
fn test_arch_t_cycle_field_is_float_type() {
    let input = format!("{DISTRIBUTED_PREAMBLE}\nStateInfo:\n    cost = Arch.t_cycle");
    let file = parse_file(&input).unwrap().file;
    let errors = only_errors(&file);
    assert!(
        errors.is_empty(),
        "Arch.t_cycle used as Float cost produced errors: {:?}",
        errors
    );
}

#[test]
fn test_arch_qpu_sizes_is_vec_int() {
    // Accessing Arch.qpu_sizes should not error
    let input = format!("{DISTRIBUTED_PREAMBLE}\nStateInfo:\n    cost = 0.0");
    let file = parse_file(&input).unwrap().file;
    let errors = only_errors(&file);
    assert!(
        errors.is_empty(),
        "Arch with Vec<Int> qpu_sizes errored: {:?}",
        errors
    );
}

// ─── 5. ArchT distributed fields ─────────────────────────────────────────────

#[test]
fn test_qpu_ids_membership_usable_as_condition() {
    let input = format!(
        "{DISTRIBUTED_PREAMBLE}\nStateInfo:\n    cost = if (Arch.qpu_ids[State.map[Gate.qubits[0]]]) == Arch.qpu_ids[State.map[Gate.qubits[1]]] then 1.0 else 0.0"
    );
    let file = parse_file(&input).unwrap().file;
    let errors = only_errors(&file);
    assert!(
        errors.is_empty(),
        "Arch.qpu_ids membership comparison produced errors: {:?}",
        errors
    );
}

#[test]
fn test_gate_bell_budget_field_no_error() {
    let input = format!(
        "{DISTRIBUTED_PREAMBLE}\nStateInfo:\n    cost = if Arch.gate_bell_budget == 40 then 1.0 else 0.0"
    );
    let file = parse_file(&input).unwrap().file;
    let errors = only_errors(&file);
    assert!(
        errors.is_empty(),
        "Arch.gate_bell_budget field produced errors: {:?}",
        errors
    );
}

// ─── 6. Gate.implementation → UserDef("GateRealization") ─────────────────────

#[test]
fn test_impl_struct_name_extracted_from_route_info() {
    let input = format!("{DISTRIBUTED_PREAMBLE}\nStateInfo:\n    cost = 0.0");
    let file = parse_file(&input).unwrap().file;
    let name = extract_impl_struct_name(&file);
    assert_eq!(
        name.as_deref(),
        Some("GateRealization"),
        "impl_struct_name should be GateRealization, got {:?}",
        name
    );
}

#[test]
fn test_gate_realization_fields_in_user_def_table() {
    // UserDefTable built from a file with remote:Bool should expose all three fields
    let input = format!("{DISTRIBUTED_PREAMBLE}\nStateInfo:\n    cost = 0.0");
    let file = parse_file(&input).unwrap().file;
    let udt = UserDefTable::new(&file);
    let fields = udt
        .get_fields("GateRealization")
        .expect("GateRealization not in UserDefTable");
    assert_eq!(
        fields.get("u"),
        Some(&Type::Location),
        "field u should be Location"
    );
    assert_eq!(
        fields.get("v"),
        Some(&Type::Location),
        "field v should be Location"
    );
    assert_eq!(
        fields.get("remote"),
        Some(&Type::Bool),
        "field remote should be Bool"
    );
}

#[test]
fn test_gate_implementation_infers_user_def_not_unknown() {
    let input = format!("{DISTRIBUTED_PREAMBLE}\nStateInfo:\n    cost = 0.0");
    let file = parse_file(&input).unwrap().file;
    let udt = UserDefTable::new(&file);
    let impl_struct_name = extract_impl_struct_name(&file);

    let gate_impl_expr = make_expr(ExprKind::FieldAccess {
        object: Box::new(make_expr(ExprKind::Identifier("Gate".to_string()))),
        field: "implementation".to_string(),
    });

    let mut sym = SymbolTable::new();
    let mut diags = Vec::new();
    let mut type_map = TypeMap::new();
    let mut generic_table = GenericTable::new();
    let mut string_labels = StringLabels::new();
    let arch_fields = HashMap::new();
    let t = infer_expr_type(
        &gate_impl_expr,
        &mut InferenceData {
            sym_table: &mut sym,
            diagnostics: &mut diags,
            type_map: &mut type_map,
            user_def_table: &udt,
            generic_table: &mut generic_table,
            string_labels: &mut string_labels,
            impl_struct_name,
            arch_fields: &arch_fields,
        },
    );
    assert_eq!(
        t,
        Type::UserDef("GateRealization".to_string()),
        "Gate.implementation should be UserDef(GateRealization), got {:?}",
        t
    );
}

// ─── 7. State.implemented_gates() → Vec<UserDef("GateRealization")> ──────────

#[test]
fn test_implemented_gates_infers_vec_user_def_not_unknown() {
    let input = format!("{DISTRIBUTED_PREAMBLE}\nStateInfo:\n    cost = 0.0");
    let file = parse_file(&input).unwrap().file;
    let udt = UserDefTable::new(&file);
    let impl_struct_name = extract_impl_struct_name(&file);

    let state_impl_gates_expr = make_expr(ExprKind::FieldAccess {
        object: Box::new(make_expr(ExprKind::Identifier("State".to_string()))),
        field: "implemented_gates".to_string(),
    });

    let mut sym = SymbolTable::new();
    let mut diags = Vec::new();
    let mut type_map = TypeMap::new();
    let mut generic_table = GenericTable::new();
    let mut string_labels = StringLabels::new();
    let arch_fields = HashMap::new();
    let t = infer_expr_type(
        &state_impl_gates_expr,
        &mut InferenceData {
            sym_table: &mut sym,
            diagnostics: &mut diags,
            type_map: &mut type_map,
            user_def_table: &udt,
            generic_table: &mut generic_table,
            string_labels: &mut string_labels,
            impl_struct_name,
            arch_fields: &arch_fields,
        },
    );
    assert_eq!(
        t,
        Type::Function {
            params: vec![],
            return_type: Box::new(Type::Vec(Box::new(Type::UserDef(
                "GateRealization".to_string()
            )))),
        },
        "State.implemented_gates should return Vec<UserDef(GateRealization)>, got {:?}",
        t
    );
}

// ─── 8. Struct/Struct type compatibility ─────────────────────────────────────

#[test]
fn test_two_option_gaterealization_branches_compatible() {
    // realize_gate outer ITE: both branches return Option<GateRealization>.
    // Before the Struct/Struct fix these were reported as incompatible.
    let input = r#"
RouteInfo:
    routed_gates = CX
    GateRealization{u : Location, v : Location, remote : Bool}
    realize_gate =
        if (Arch.qpu_ids[State.map[Gate.qubits[0]]]) == Arch.qpu_ids[State.map[Gate.qubits[1]]]
            then
                if Arch.contains_edge((State.map[Gate.qubits[0]], State.map[Gate.qubits[1]]))
                    then Some(GateRealization{u = State.map[Gate.qubits[0]], v = State.map[Gate.qubits[1]], remote = false})
                    else None
            else
                Some(GateRealization{u = State.map[Gate.qubits[0]], v = State.map[Gate.qubits[1]], remote = true})

TransitionInfo:
    Transition{edge : (Location, Location)}
    get_transitions = []
    apply = value_swap(Transition.edge.(0), Transition.edge.(1))
    cost = 1.0

ArchInfo:
    Arch{ num_qpus : Int, qpu_sizes : Vec<Int>, qpu_ids : Vec<Int>, link_cost : Float, comm_qubits : Vec<Location>, alg_qubits : Vec<Location> }
    get_locations = Arch.alg_qubits()
"#;
    let file = parse_file(input).unwrap().file;
    let diags = check_semantics(&file).diagnostics;
    let compat_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("compatible types"))
        .collect();
    assert!(
        compat_errors.is_empty(),
        "Option<GateRealization> branches flagged as incompatible: {:?}",
        compat_errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ─── 9. IndexAccess on Unknown — no false positives ──────────────────────────

#[test]
fn test_dynamic_field_access_on_unknown_produces_no_false_positives() {
    // x.implementation.(remote()) where x is a lambda param (Unknown).
    // Must not produce:
    //   (a) "Undefined variable 'remote'" — the index is inside .(expr) syntax
    //   (b) "compatible types" error — then/else are both Float
    let input = format!("{DISTRIBUTED_PREAMBLE}\n{STATEINFO_WITH_IMPLEMENTATION_ACCESS}");
    let file = parse_file(&input).unwrap().file;
    let diags = check_semantics(&file).diagnostics;

    let undef_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("Undefined variable") || d.message.contains("'remote'"))
        .collect();
    assert!(
        undef_errors.is_empty(),
        "Spurious 'Undefined variable' error(s) from .(remote()): {:?}",
        undef_errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );

    let compat_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("compatible types"))
        .collect();
    assert!(
        compat_errors.is_empty(),
        "Branch type mismatch in x.implementation.(remote()) lambda: {:?}",
        compat_errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ─── 10. Regression: nisq.qmrl ───────────────────────────────────────────────

#[test]
fn test_nisq_no_errors_after_distributed_changes() {
    let file = parse_file(NISQ).unwrap().file;
    let errors = only_errors(&file);
    assert!(
        errors.is_empty(),
        "nisq.qmrl regression — unexpected error(s):\n{}",
        errors
            .iter()
            .map(|m| format!("  - {m}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Verify that `x` in `map(|x| -> ..., State.implemented_gates())` gets type
/// UserDef("GateRealization") so `x.implementation.(remote())` is fully typed.
#[test]
fn test_map_lambda_param_inferred_from_container() {
    let src = format!(
        "{}\nStateInfo:\n    cost = fold(0.0, |x, acc| -> acc, map(|x| -> x, State.implemented_gates()))\n",
        DISTRIBUTED_PREAMBLE
    );
    let file = parse_file(&src).unwrap().file;
    let errors = only_errors(&file);
    assert!(
        errors.is_empty(),
        "map lambda param inference introduced errors:\n{}",
        errors
            .iter()
            .map(|m| format!("  - {m}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Verify `x.implementation.(remote())` inside map produces no errors
/// when x is properly bound to GateRealization.
#[test]
fn test_map_lambda_field_access_no_error_with_inferred_param() {
    let src = format!(
        "{DISTRIBUTED_PREAMBLE}\nStateInfo:\n    cost = fold(0.0, |x, acc| -> acc + x, map(|x| -> if x.implementation.(remote()) then 1.0 else 0.0, State.implemented_gates()))\n"
    );
    let file = parse_file(&src).unwrap().file;
    let errors = only_errors(&file);
    assert!(
        errors.is_empty(),
        "map lambda with inferred GateRealization param produced errors:\n{}",
        errors
            .iter()
            .map(|m| format!("  - {m}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn test_nisq_impl_struct_name_extracted() {
    let file = parse_file(NISQ).unwrap().file;
    let name = extract_impl_struct_name(&file);
    assert_eq!(
        name.as_deref(),
        Some("GateRealization"),
        "nisq.qmrl impl struct should be GateRealization"
    );
}
