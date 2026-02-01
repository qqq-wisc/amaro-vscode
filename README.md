# Amaro VS Code Extension

**Amaro** is a domain-specific language for defining Quantum Gate Realizations, Transitions, and Architectures. This extension provides rich syntax highlighting and Language Server Protocol (LSP) support to make writing `.qmrl` files easier and error-free.

## Features

### Syntax Highlighting
Full-color syntax highlighting for the Amaro language structure, including:
* **Blocks:** `GateRealization`, `Transition`, `Architecture`, `Step`.
* **Info Definitions:** `RouteInfo:`, `TransitionInfo:`, `ArchInfo:`.
* **Embedded Rust:** Correctly highlights Rust code inside `{{ ... }}` blocks.
* **Quantum Types:** Special highlighting for `CX`, `T`, `Pauli`, and `Location`.

### Language Server Protocol (LSP)
Includes a custom Rust-based Language Server (`amaro-lsp`) that provides:

1.  **Semantic Analysis & Diagnostics:**
    * **Validation:** Checks for mandatory blocks (e.g., `RouteInfo`) and required fields (e.g., `routed_gates`).
    * **Style/Lint Checks:** Warns if block names are not capitalized (e.g., `transition` → `Transition`).
    * **Structure:** Validates correct key-value pairs, fields and struct definitions.
2.  **Document Outline (Symbols):**
    * Navigate complex blocks, steps, fields and files easily using the VS Code "Outline" view or "Go to Symbol" (`Ctrl+Shift+O`).
    * Symbols are categorized by hierarchy: Blocks (Classes), Steps (Functions), and Fields.
3.  **Robust Parsing:**
    * Fault-tolerant parsing that continues analyzing the file even after encountering syntax errors (Error recovery).
    * Full support for embedded Rust blocks `{{ ... }}`.

## Requirements

This extension relies on a Rust-based Language Server (`amaro-lsp`) that must be built locally.

1.  **Node.js & npm:** The VS Code extention portion requires Node.js.
    * Install from [nodejs.org](https://nodejs.org/en).
2.  **Rust Toolchain:** You need `cargo` installed to build the language server.
    * Install from [rustup.rs](https://rustup.rs/).
3.  **Build Step:**
    * The extension looks for the binary at `amaro-lsp/target/debug/amaro-lsp` after the build step is complete.

## Building & Running the Extension
Clone the repository and build as follows:
```bash
# Install Node modules
npm install

# Build the Rust language server
cd amaro-lsp
cargo build
cd ..

# Start the extension in watch mode
npm run watch
```

### Running the Extension
After running `npm run watch`:
1. **Press `F5` in VS Code.**
2. A new VS Code window will open with the **Amaro extension loaded**.
3. Open any `.qmrl` file to see highlighting, symbols, and diagnostics.

## Example Code

This extension provides highlighting, navigation, and error checking for Amaro files like this:

```amaro
RouteInfo:
    routed_gates = CX
    realize_gate = Some(value)

    // Struct definitions are supported inside blocks
    GateRealization { u: Location, v: Location }

TransitionInfo:
    cost = 1.0
    apply = identity

{{
    // Embedded Rust code is parsed and ignored by the Amaro validator
    fn get_cost(pair: (Location, Location)) -> f64 {
        return 0.0;
    }
}}
```

## Known Issues
* **Binary Path:** The extension expects the `amaro-lsp` binary to be built in `target/debug`. You must run `cargo build` before starting the extension.
* **Type Checking:** Advanced type checking (e.g., validating that `routed_gates` is assigned a valid Gate type) is currently in early development.

## Release Notes
**0.0.1**
* Initial release.
* Added Grammar for `.qmrl `files.
* Added Language Client connection to `amaro-lsp`.
* **Core Parser:** Implemented robust AST parsing with error recovery.
* **Semantic Analysis:** Added diagnostics for missing mandatory blocks and required keys.
* **Symbol Navigation:** Added support for "Go to Symbol" and Outline view.
* **Embedded Rust:** Improved parsing for Rust code blocks {{ ... }}.
* **Fixes:** Resolved concurrency issues with node IDs in the language server.
