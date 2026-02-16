# Contributing to Amaro VS Code Extension

Thank you for your interest in contributing to the Amaro language tools! This document provides guidance on building, testing, and debugging the extension and the language server.

## Architecture Overview

The project consists of two main components:

1. **Extension Client (`/` root)**: A TypeScript-based VS Code extension that handles syntax highlighting, configuration, and launching the Language Server.
2. **Language Server (`/amaro-lsp`)**: A Rust-based server that implements the Language Server Protocol (LSP). It handles parsing, AST generation, and semantic analysis.

### Language Server Pipeline

The LSP follows a three-stage pipeline:

```
.qmrl file → Parser → AST → Semantics → Diagnostics → VS Code
```

**Stage 1: Parser (`parser/core.rs`, `parser/expr.rs`)**
- Uses `nom` combinators to parse `.qmrl` syntax
- Builds an Abstract Syntax Tree (AST) defined in `ast.rs`
- Handles error recovery (continues parsing after syntax errors)
- Parses embedded Rust blocks `{{ ... }}`

**Stage 2: Semantic Analyzer (`parser/semantics.rs`)**
- Validates block structure (RouteInfo, TransitionInfo, etc.)
- Checks required fields (`routed_gates`, `realize_gate`, etc.)
- Performs type inference on all expressions
- Emits diagnostics (errors and warnings)

**Stage 3: Symbol Table (`parser/symbols.rs`)**
- Tracks variable bindings and their types
- Manages scopes (global, let-bindings, lambda parameters)
- Contains all built-in functions and type constructors
- Handles type compatibility checks

## Development Setup

### Prerequisites
* **Node.js** (v14 or higher)
* **Rust** (latest stable)
* **VS Code**

### Initial Build
1. **Install Client Dependencies:**
   ```bash
   npm install
   ```
2. **Build the Language Server:**
   ```bash
   cd amaro-lsp
   cargo build
   # The extension expects the binary at: amaro-lsp/target/debug/amaro-lsp
   ```

## Debugging Loop

1. **Open the project in VS Code.**
2. **Press `F5`** to launch the "Extension Development Host" window.
3. **Open a `.qmrl` file** in the new window.
4. **View Logs:**
   * Open the Output panel (`Ctrl+Shift+U` or `Cmd+Shift+U` on Mac).
   * Select **"Amaro Language Server"** from the dropdown to see Rust `eprintln!` logs.

**Tip:** If you modify the Rust code (`amaro-lsp`), you must:
1. Stop the debugger (`Shift+F5`).
2. Re-run `cargo build` in `amaro-lsp`.
3. Press `F5` again.

### Debug Logging

To enable AST debug output, uncomment the `#[cfg(debug_assertions)]` block in `server.rs`:

```rust
#[cfg(debug_assertions)]
{
    let ast_summary = format_simple_ast(&file);
    self.client.log_message(MessageType::INFO, format!("Parsed AST:\n{}", ast_summary)).await;
}
```

## Testing

We use standard Rust testing for the parser and semantic analyzer.

### Running Tests
```bash
cd amaro-lsp
cargo test
```

### Running a Specific Test
```bash
cargo test test_qubit_index_on_qubitmap
```

### Adding New Tests
Tests are located in:
- `tests/parser_tests.rs` — Parser and AST tests
- `tests/semantic_tests.rs` — Type inference and validation tests
- `tests/symbols_tests.rs` — Symbol table tests
- `tests/utils_tests.rs` — Utils tests

Example of a Semantic Test:
```rust
#[test]
fn test_my_new_feature() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    GateRealization{path : Vec<Location>}
    realize_gate = State.map[Gate.qubits[0]]
TransitionInfo:
    get_transitions = []
    apply = []
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    let errors: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
}
```

## Common Contribution Scenarios

### Adding a New Built-in Function

1. **Add the function signature to `symbols.rs`:**
   ```rust
   fn register_builtin_functions(scope: &mut HashMap<String, Type>) {
       // ... existing functions
       
       scope.insert("my_new_function".to_string(), Type::Function {
           params: vec![Type::ArchT, Type::Int],
           return_type: Box::new(Type::Vec(Box::new(Type::Location))),
       });
   }
   ```

2. **Add a test in `tests/semantic_tests.rs`:**
   ```rust
   #[test]
   fn test_my_new_function() {
       let input = r#"
   RouteInfo:
       routed_gates = CX
       GateRealization{}
       realize_gate = my_new_function(Arch, 5)
   TransitionInfo:
       get_transitions = []
       apply = []
       cost = 0.0
   "#;
       let file = parse_file(input).unwrap();
       let diags = check_semantics(&file);
       assert!(diags.is_empty());
   }
   ```

3. **Run tests:**
   ```bash
   cargo test
   ```

### Adding a New Block Type

1. **Add validation rules in `semantics.rs`:**
   ```rust
   let known_blocks = [
       "RouteInfo", "TransitionInfo", "ArchInfo", "StateInfo",
       "MyNewBlock"  // Add here
   ];
   
   // Add required fields
   required_keys.insert("MyNewBlock", vec!["field1", "field2"]);
   ```

2. **Add the block to mandatory checks if needed:**
   ```rust
   let required_blocks = ["RouteInfo", "TransitionInfo", "MyNewBlock"];
   ```

3. **Add tests to verify validation works.**

### Adding a New Type

1. **Add to the `Type` enum in `symbols.rs`:**
   ```rust
   pub enum Type {
       // ... existing types
       MyNewType,
   }
   ```

2. **Add type compatibility rules in `types_compatible()`:**
   ```rust
   fn types_compatible(t1: &Type, t2: &Type) -> bool {
       // ... existing rules
       (Type::MyNewType, Type::MyNewType) => true,
   }
   ```

3. **Add field access rules if needed in `infer_expr_type()`:**
   ```rust
   Type::MyNewType => {
       match field.as_str() {
           "my_field" => Type::Int,
           _ => Type::Unknown,
       }
   }
   ```

### Adding New Syntax

1. **Update the parser in `parser/expr.rs` or `parser/core.rs`**
2. **Add the corresponding AST node in `ast.rs`**
3. **Add type inference logic in `semantics.rs`**
4. **Add parser tests in `tests/parser_tests.rs`**
5. **Add semantic tests in `tests/semantic_tests.rs`**

## Project Structure

```
amaro-vscode/
├── package.json              # Extension manifest
├── syntaxes/
│   └── amaro.tmLanguage.json # TextMate grammar for syntax highlighting
├── src/
│   └── extension.ts          # Client entry point (TypeScript)
└── amaro-lsp/                # Language Server (Rust)
    ├── src/
    │   ├── main.rs           # LSP server entry point
    │   ├── server.rs         # LSP protocol implementation
    │   ├── ast.rs            # AST definitions
    │   └── parser/
    │       ├── mod.rs        # Parser module exports
    │       ├── core.rs       # Block and top-level parsing
    │       ├── expr.rs       # Expression parsing
    │       ├── symbols.rs    # Symbol table and type system
    │       ├── semantics.rs  # Type inference and validation
    │       └── utils.rs      # Helper functions
    └── tests/
        ├── parser_tests.rs   # Parser tests
        ├── semantic_tests.rs # Semantic analysis tests
        └── symbols_tests.rs  # Symbol table tests
```

### Module Responsibilities

**`ast.rs`**
- Defines all AST node types
- Each node has a `Range` for LSP diagnostics
- Each node has a `NodeId` for tracking

**`parser/core.rs`**
- Parses blocks (RouteInfo, TransitionInfo, etc.)
- Parses struct definitions
- Parses fields and their values
- Handles embedded Rust `{{ ... }}`

**`parser/expr.rs`**
- Parses all expressions (literals, operators, function calls)
- Handles precedence and associativity
- Parses lambdas, let-bindings, if-then-else
- Manages recursion depth limits

**`parser/symbols.rs`**
- Defines the type system (`Type` enum)
- Manages scoped symbol table
- Registers all built-in functions
- Provides type lookup

**`parser/semantics.rs`**
- Validates block structure and required fields
- Performs type inference on expressions
- Checks type compatibility
- Generates LSP diagnostics

**`server.rs`**
- Implements LSP protocol handlers
- Provides document symbols (outline view)
- Validates documents on open/change
- Formats error messages

## Code Quality

### Before Committing

```bash
# Format code
cargo fmt

# Check for warnings
cargo clippy

# Run tests
cargo test

# Verify extension still works
npm run watch  # Then press F5 in VS Code
```

### Style Guide

**Rust:**
- Follow standard `rustfmt` guidelines (run `cargo fmt`)
- Add doc comments `///` to all public functions
- Use `Type::Unknown` for intentional leniency (document why)
- Prefer `if let` over `match` for single-pattern matches

**TypeScript:**
- Use Prettier (configured in `package.json`)
- Keep LSP client minimal — logic belongs in Rust

**Commits:**
- Use conventional commits: `feat:`, `fix:`, `docs:`, `test:`
- Be descriptive: `feat: add QubitMap indexing by Qubit type`

## Common Issues

### "Binary not found" error
The extension can't find `amaro-lsp/target/debug/amaro-lsp`. Run:
```bash
cd amaro-lsp && cargo build
```

### Changes not reflected
After modifying Rust code, you must:
1. Stop the debugger
2. Rebuild: `cargo build`
3. Restart: Press `F5`

### Tests failing
Make sure your changes didn't break type inference. Run:
```bash
cargo test --verbose
```

### Clippy warnings
Fix before committing:
```bash
cargo clippy --fix
```

## Getting Help

- **Questions:** Open a GitHub Discussion
- **Bugs:** Open a GitHub Issue with a minimal `.qmrl` reproduction
- **Feature Requests:** Open an Issue describing the use case

Thank you for contributing!
