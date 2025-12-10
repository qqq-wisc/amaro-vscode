# Marol VS Code Extension

**Marol** is a domain-specific language for defining Quantum Gate Realizations, Transitions, and Architectures. This extension provides rich syntax highlighting and language server support to make writing `.qmrl` files easier and error-free.

## Features

### Syntax Highlighting
Full-color syntax highlighting for the Marol language structure, including:
* **Blocks:** `GateRealization[...]`, `Transition[...]`, `Architecture[...]`, `Step[...]`.
* **Info Definitions:** `RouteInfo:`, `TransitionInfo:`, etc.
* **Embedded Rust:** Correctly highlights Rust code inside `{{ ... }}` blocks.
* **Quantum Types:** Special highlighting for `CX`, `T`, `Pauli`, and `Location`.

### Language Server Protocol (LSP)
Includes a custom Rust-based Language Server (`marol-lsp`) that runs in the background to provide:
* **File Analysis:** Logs file open/change events (Foundation for future type checking).
* **Safety Checks:** automatically validates that the LSP binary exists.

## Requirements

This extension includes a Rust-based Language Server that must be built locally.

1.  **Rust Toolchain:** You need `cargo` installed to build the language server.
    * Install from [rustup.rs](https://rustup.rs/).
2.  **Build Step:**
    * Navigate to the extension folder: `cd marol-lsp`
    * Run `cargo build`
    * The extension will look for the binary at `marol-lsp/target/debug/marol-lsp`.

## Extension Settings

Currently, this extension does not contribute custom settings. It automatically activates for files with the `.qmrl` extension.

## Example Code

This extension provides highlighting for Marol files like this:

```marol
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
* **Binary Path:** The extension expects the `marol-lsp` binary to be built in `target/debug`. If you move the folder structure, the extension may fail to start.
* **LSP Features:** Diagnostics (error checking) are currently in development.

## Release Notes
**0.0.1**
* Initial release.
* Added Grammar for `.qmrl `files.
* Added Language Client connection to `marol-lsp`.
* Support for embedded Rust syntax.
