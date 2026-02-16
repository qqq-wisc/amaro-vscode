# Amaro VS Code Extension

**Amaro** is a domain-specific language for defining Quantum Gate Realizations, Transitions, and Architectures. This extension provides rich syntax highlighting and Language Server Protocol (LSP) support to make writing `.qmrl` files easier and error-free.

## Features

### Advanced Semantic Analysis
The LSP performs deep type inference and validation of your quantum routing logic:

* **QubitMap Indexing:** Correctly validates `State.map[Gate.qubits[0]]` - understands that `QubitMap` is indexed by `Qubit`, not `Int`.
* **Unified Access:** Intelligently handles both property access (`State.map`) and functional access (`State.map()`), allowing for cleaner, more flexible code styles.
* **Type Inference:** Infers types through nested expressions including `map`, `fold`, `let...in`, and `if-then-else`.
* **Smart Leniency:** Expressions of `Unknown` type (e.g. `x.implementation.(path())`) are accepted without false errors.
* **Control Flow Validation:** Ensures type consistency across `if-then-else` branches and supports nested `let...in` bindings.
* **Vector Operations:** Built-in support for standard vector methods (`push`, `pop`, `extend`) and tuple indexing (`edge.0`).
* **Deep Type Checking:** Recursively validates generic types (e.g., `Vec<Vec<Location>>`) and custom Struct compatibility.


### Syntax Highlighting
Full-color syntax highlighting for `.qmrl` files including:
* **Blocks:** `RouteInfo`, `TransitionInfo`, `ArchInfo`, `StateInfo`.
* **Embedded Rust:** Correctly highlights Rust code inside `{{ ... }}` blocks.
* **Quantum Types:** Special highlighting for `CX`, `T`, `Pauli`, `Location` and `Qubit`.
* **Smart Parsing:** Correctly parses integers as field names for tuple access (e.g., `transition.edge.0`).
* **Expressions:** Struct definitions, field access, and lambda expressions


### Language Server Protocol (LSP)
A custom Rust-based Language Server (`amaro-lsp`) providing:

1.  **Semantic Analysis & Diagnostics:**
    * **Validation:** Validates mandatory blocks (`RouteInfo`, `TransitionInfo`) and required fields (`routed_gates`, `realize_gate`, `get_transitions`, `apply`, `cost`).
    * **Style/Lint Checks:** Warns on incorrectly capitalized block names.
    * **Structure:** Validates correct key-value pairs, fields and struct definitions.
2.  **Document Outline (Symbols):**
    * Navigate complex blocks, steps, fields and files easily using the VS Code "Outline" view or "Go to Symbol" (`Ctrl+Shift+O`).
    * Symbols are categorized by hierarchy: Blocks (Classes), Steps (Functions), and Fields.
3.  **Robust Parsing:**
    * Fault-tolerant parsing that continues analyzing the file even after encountering syntax errors (Error recovery).
    * Full support for embedded Rust blocks `{{ ... }}`.

## Requirements
Because this extension is currently in alpha, you must compile the language server manually:
1. **Node.js & npm** - [nodejs.org](https://nodejs.org/en)
2. **Rust Toolchain** - [rustup.rs](https://rustup.rs/)

## Building & Running
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

Press `F5` in VS Code to open a development window with the extension loaded. Open any `.qmrl` file to activate highlighting, symbols, and diagnostics.

## Testing

**Running Unit Tests**
The project includes a comprehensive suite of unit tests for the parser, covering syntax, precedence, and complex expressions.
```bash
cd amaro-lsp
cargo test
```

**Clean Build** To remove all build artifacts and compile from scratch (useful if you encounter strange caching issues):

```bash
cd amaro-lsp
cargo clean && cargo build
```

## Example Code
This extension provides highlighting, navigation, and error checking for Amaro files like this:

```amaro
RouteInfo:
    routed_gates = CX, T
    GateRealization{path : Vec<Location>}
    realize_gate =
        if (Gate.gate_type()) == CX
        then
            map(|x| -> GateRealization{path = x},
                all_paths(Arch,
                    vertical_neighbors(State.map[Gate.qubits[0]], Arch.width, Arch.height),
                    horizontal_neighbors(State.map[Gate.qubits[1]], Arch.width),
                    (values(State.map())).extend(Arch.magic_state_qubits())))
        else
            map(|x| -> GateRealization{path = x},
                all_paths(Arch,
                    vertical_neighbors(State.map[Gate.qubits[0]], Arch.width, Arch.height),
                    fold(Vec(), |x, acc| -> acc.extend(x),
                         map(|x| -> horizontal_neighbors(x, Arch.width), Arch.magic_state_qubits())),
                    (values(State.map())).extend(Arch.magic_state_qubits())))

TransitionInfo:
    Transition{edge : (Location, Location)}
    get_transitions = map(|x| -> Transition{edge = x}, Arch.edges())
    apply = value_swap(Transition.edge.(0), Transition.edge.(1))
    cost = 1.0

ArchInfo:
    Arch{width : Int, height : Int}

StateInfo:
    cost = 1.0

{{
    // Embedded Rust code is parsed and ignored by the Amaro validator
    fn get_cost(pair: (Location, Location)) -> f64 {
        return 0.0;
    }
}}
```

## Examples

The `examples/` folder contains working `.qmrl` files demonstrating different quantum routing strategies:

- **nisq.qmrl** - Simple NISQ routing for near-term devices
- **scmr.qmrl** - Surface Code Mapping & Routing with magic states
- **ilq.qmrl** - Interleaved Lattice Qubits with layer-aware routing
- **mqlss.qmrl** - Magic State Lattice Surgery using Steiner trees

See `examples/Readme.md` for detailed explanations.

## Known Issues
* **LSP Binary Location:** The extension currently looks for the language server in `amaro-lsp/target/debug/amaro-lsp`. You **must** run `cargo build` inside the `amaro-lsp` folder before launching the extension.
* **Type Checking:** The `gate_type()` return type is treated as `Gate` for comparison purposes; enum variants are not distinguished.


See [CHANGELOG.md](CHANGELOG.md) for full history.
