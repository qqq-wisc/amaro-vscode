# Amaro VS Code Extension

**Amaro** is a domain-specific language for defining Quantum Gate Realizations, Transitions, and Architectures. This extension provides rich syntax highlighting and Language Server Protocol (LSP) support to make writing `.qmrl` files easier and error-free.

## Features

### Syntax Highlighting
Full-color syntax highlighting for the Amaro language structure, including:
* **Blocks:** `GateRealization[...]`, `Transition[...]`, `Architecture[...]`, `Step[...]`.
* **Info Definitions:** `RouteInfo:`, `TransitionInfo:`, etc.
* **Embedded Rust:** Correctly highlights Rust code inside `{{ ... }}` blocks.
* **Quantum Types:** Special highlighting for `CX`, `T`, `Pauli`, and `Location`.

### Language Server Protocol (LSP)
Includes a custom Rust-based Language Server (`amaro-lsp`) that runs in the background to provide:
* **Code Diagnostics & Checks:** Includes basic syntactic validation and style checks:
    * **Style Convention Check:** Warns if recognized block names (e.g., `transition`) are not capitalized (`Transition`).
    * **Robust Parsing:** Handles complex nested brackets and declarative colon blocks (e.g., `RouteInfo:`) without breaking the parser.
* **File Analysis:** Logs file open/change events (Foundation for future type checking).
* **Safety Checks:** automatically validates that the LSP binary exists.

## Requirements

This extension relies on a Rust-based Language Server (`amaro-lsp`) that must be built locally.

1.  **Rust Toolchain:** You need `cargo` installed to build the language server.
    * Install from [rustup.rs](https://rustup.rs/).
2.  **Build Step:**
    * Navigate to the extension folder: `cd amaro-lsp`
    * Run `cargo build`
    * The extension will look for the binary at `amaro-lsp/target/debug/amaro-lsp`.

## Extension Settings

Currently, this extension does not contribute custom settings. It automatically activates for files with the `.qmrl` extension.

## Example Code

This extension provides highlighting and diagnostics for Amaro files like this:

```amaro
RouteInfo:
    routed_gates = CX
    GateRealization{u : Location, v : Location}
    realize_gate = if Arch.contains_edge((State.map[Gate.qubits[0]],State.map[Gate.qubits[1]]))
            then Some(GateRealization{u = State.map[Gate.qubits[0]],v = State.map[Gate.qubits[1]]})
            else None

{{
    // Embedded Rust code
    fn get_cost(pair: (Location, Location)) -> f64 {
        return 0.0;
    }
}}
```

## Known Issues
* **Binary Path:** The extension expects the `amaro-lsp` binary to be built in `target/debug`. You must run `cargo build` before starting the extension.
* You may need to adjust your build process if you change the project structure.
* **LSP Features:** Comprehensive type-checking and structural validation is still under development.

## Release Notes
**0.0.1**
* Initial release.
* Added Grammar for `.qmrl `files.
* Added Language Client connection to `amaro-lsp`.
* Support for embedded Rust syntax (`{{ ... }}`).
* Implemented Style Diagnostics (Capitalization checks for blocks).
