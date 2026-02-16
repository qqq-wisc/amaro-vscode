# Contributing to Amaro VS Code Extension

Thank you for your interest in contributing to the Amaro language tools! This document provides guidance on building, testing, and debugging the extension and the language server.

## Architecture Overview

The project consists of two main components:

1.  **Extension Client (`/` root)**: A TypeScript-based VS Code extension that handles syntax highlighting, configuration, and launching the Language Server.
2.  **Language Server (`/amaro-lsp`)**: A Rust-based server that implements the Language Server Protocol (LSP). It handles parsing, AST generation, and semantic analysis.

## Development Setup

### Prerequisites
* **Node.js** (v14 or higher)
* **Rust** (latest stable)
* **VS Code**

### Initial Build
1.  **Install Client Dependencies:**
    ```bash
    npm install
    ```
2.  **Build the Language Server:**
    ```bash
    cd amaro-lsp
    cargo build
    # The extension expects the binary at: amaro-lsp/target/debug/amaro-lsp
    ```

## Debugging Loop

1.  **Open the project in VS Code.**
2.  **Press `F5`** to launch the "Extension Development Host" window.
3.  **Open a `.qmrl` file** in the new window.
4.  **View Logs:**
    * Open the Output panel (`Ctrl+Shift+U`).
    * Select **"Amaro Language Server"** from the dropdown to see Rust `eprintln!` logs.

**Tip:** If you modify the Rust code (`amaro-lsp`), you must:
1.  Stop the debugger (`Shift+F5`).
2.  Re-run `cargo build` in `amaro-lsp`.
3.  Press `F5` again.

## Testing

We use standard Rust testing for the parser and semantic analyzer.

### Running Tests
```bash
cd amaro-lsp
cargo test
```

### Adding New Tests
Tests are located in `amaro-lsp/src/tests/`.
* **Parser Tests:** Ensure new syntax is parsed correctly into the AST.
* **Semantic Tests:** Ensure type inference works and invalid code produces Diagnostics.

Example of a Semantic Test (`src/tests/semantics.rs`):
```rust
#[test]
fn test_my_new_feature() {
    let input = "RouteInfo: ...";
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    assert!(diags.is_empty());
}
```

## Project Structure
* `package.json` - Extension manifest (defines grammar, commands, configuration).
* `syntaxes/amaro.tmLanguage.json` - TextMate grammar for syntax highlighting.
* `src/extension.ts` - Client entry point.
* `amaro-lsp/`
    * `src/ast.rs` - Abstract Syntax Tree definitions.
    * `src/parser.rs` - Nom-based parser implementation.
    * `src/semantics.rs` - Type checking and validation logic.
    * `src/server.rs` - LSP implementation (Tower LSP).

## Style Guide
* **Rust:** Follow standard `rustfmt` guidelines. Run `cargo fmt` before committing.
* **TypeScript:** Use Prettier (configured in `package.json`).
* **Commits:** Use descriptive commit messages (e.g., `feat: add support for tuple indexing`).
