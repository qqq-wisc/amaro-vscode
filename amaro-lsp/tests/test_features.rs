use amaro_lsp::parser::{check_semantics, parse_file};

#[test]
fn test_advanced_features_and_vectors() {
    let input = r#"
    RouteInfo:
        routed_gates = CX
        
        GateRealization{path : Vec()}
        
        realize_gate = 
            if (Gate.gate_type()) == CX 
            then 
                (let v = Vec() in
                let v2 = v.push(Location(0)) in
                let v3 = v.extend(v2) in
                let popped = v3.pop() in
                
                all_paths(Arch, 
                            vertical_neighbors(State.map[Gate.qubits[0]], 10, 10), 
                            horizontal_neighbors(State.map[Gate.qubits[1]], 10), 
                            Vec()))
            else 
                Vec()

    TransitionInfo:
        Transition{na : Location}
        
        get_transitions = (Vec()).push(Transition{na=Location(0)})
        
        apply = identity_application(step)
        cost = 0.0

    ArchInfo:
        Arch{width : Int}

    StateInfo:
        cost = 1.0
    "#;

    let file = parse_file(&input).unwrap();
    let diags = check_semantics(&file);

    // 3. Assert NO Errors
    for diag in &diags {
        println!("Diagnostic: {:?}", diag);
    }
    assert!(
        diags.is_empty(),
        "Expected 0 diagnostics, found {}",
        diags.len()
    );
}
