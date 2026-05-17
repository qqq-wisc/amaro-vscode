use amaro_lsp::parser::symbols::*;

#[test]
fn test_scope_management() {
    let mut table = SymbolTable::new();

    table.bind("x".to_string(), Type::Int);
    assert!(matches!(table.lookup("x"), Some(Type::Int)));

    table.enter_scope();
    table.bind("y".to_string(), Type::Float);

    assert!(matches!(table.lookup("x"), Some(Type::Int)));
    assert!(matches!(table.lookup("y"), Some(Type::Float)));

    table.exit_scope();

    assert!(matches!(table.lookup("x"), Some(Type::Int)));
    assert!(matches!(table.lookup("y"), None));
}

#[test]
fn test_shadowing() {
    let mut table = SymbolTable::new();

    table.bind("x".to_string(), Type::Int);
    assert!(matches!(table.lookup("x"), Some(Type::Int)));

    table.enter_scope();
    table.bind("x".to_string(), Type::Float);
    assert!(matches!(table.lookup("x"), Some(Type::Float)));

    table.exit_scope();
    assert!(matches!(table.lookup("x"), Some(Type::Int)));
}

#[test]
fn test_nested_scopes() {
    let mut table = SymbolTable::new();

    table.bind("a".to_string(), Type::Int);

    table.enter_scope();
    table.bind("b".to_string(), Type::Float);

    table.enter_scope();
    table.bind("c".to_string(), Type::Bool);

    assert!(matches!(table.lookup("a"), Some(Type::Int)));
    assert!(matches!(table.lookup("b"), Some(Type::Float)));
    assert!(matches!(table.lookup("c"), Some(Type::Bool)));

    table.exit_scope();

    assert!(matches!(table.lookup("a"), Some(Type::Int)));
    assert!(matches!(table.lookup("b"), Some(Type::Float)));
    assert!(matches!(table.lookup("c"), None));

    table.exit_scope();

    assert!(matches!(table.lookup("a"), Some(Type::Int)));
    assert!(matches!(table.lookup("b"), None));
}

#[test]
fn test_lookup_nonexistent() {
    let table = SymbolTable::new();
    assert!(matches!(table.lookup("nonexistent"), None));
}
