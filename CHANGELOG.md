# Change Log

All notable changes to the "amaro-vscode" extension will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/).

<<<<<<< HEAD
## [1.0.3] - 2026-02-27
=======
## [1.0.3] - 2026-03-04
>>>>>>> main

### Added
- **Autocomplete:** Typing a '.' after some types will provide autocomplete options. For instance, typing `Arch.` shows the fields and functions on `Arch`.
- **Expression type shown on hover:** Hovering over most expressions indicates the type of the expression.
- **Field type shown on hover:** Hovering over a field within a block indicates the type of the field for most basic fields. For instance, hovering over `cost` in the `TransitionInfo` block will indicate that it expects a function `(Transition) -> Float`.
- **`match` Expression Support:** The parser now handles `match <expr> with | Pattern -> body` syntax. Type inference checks that all arms return consistent types and warns on mismatches. `match` and `with` are correctly reserved as keywords.
- **Trap Topology Fields:** `ArchT` now recognizes `trap_positions`, `trap_vertices`, `trap_edges`, `locations`, and `edges_between` as valid fields, enabling trapped-ion architecture files to validate without false errors.
- **Missing Built-in Functions:** Added `consistent`, `to_2d`, `combinations`, `max`, `min`, `abs`, and `dist` to the global symbol table. These were previously flagged as "Undefined variable" errors.
- **`Step` Context Variable:** Registered `Step` (capitalized) as an alias for the state context variable, restoring compatibility with older-format `.qmrl` files.

### Fixed
- **Invalid Expression Ranges:** Ranges now accurately reflect start and end bounds of all expressions.
- **BinaryOp Type Inference:** Comparison operators (`==`, `!=`, `<`, `<=`, `>`, `>=`) now correctly return `Bool`. Arithmetic operators return the operand type (`Int` or `Float`). Previously all binary operations returned `Unknown`, allowing invalid conditions like `cost = (x > y)` to silently pass.
- **UnaryOp Type Inference:** `!expr` now returns `Bool` (and errors if the operand is non-Bool). `-expr` returns `Int` or `Float` based on the operand. Both previously returned `Unknown`.
- **Projection Type Inference:** Tuple projection (e.g., `edge.(0)`) now returns the type of the indexed element instead of `Unknown`.
- **UnaryOp Range Overflow Panic:** Fixed a crash where `!x` and `-x` expressions caused a `usize` underflow panic due to a column number being incorrectly used as a byte offset when computing the diagnostic range.
- **Struct Field Pre-pass:** The semantic checker now performs a pre-pass to register user-defined struct fields (e.g., `GateRealization{path : Vec<Location>}`) before type-checking expressions. Field accesses on user-defined structs now return the correct declared type instead of `Unknown`.
- **`cost` Field Type Enforcement:** `TransitionInfo.cost` and `StateInfo.cost` now reject non-Float values. `Bool` and `String` values produce an error; `Int` is accepted with leniency.
- **`return` Keyword Warning:** When a field value starts with `return` (invalid in Amaro's functional style), the extension now emits a targeted warning explaining the issue, rather than silently dropping the field and producing a misleading "missing required field" error downstream.
- **Human-Readable Diagnostic Messages:** Error messages that previously showed Rust debug format (e.g., `Vec(Box(Location))`) now display proper type names (e.g., `Vec<Location>`).

### Changed
- **Expression inference:** The `infer_expr_type` method has changed signature. It additionally takes in a mutable `type_map` field, which stores mappings from expression IDs to their `Type`s as expression types are inferred. As well as this, a `user_def_table` is passed, which holds the fields for user-defined structs such as `Transition`.
- **Type enum:** In conjunction with the above, the `Type` enum has been extended to allow for referring to user-defined types.

## [1.0.2] - 2026-02-18

### Added
- **Multi-Platform Support:** Bundled pre-compiled binaries for Windows (`win32`), macOS (`darwin`), and Linux (`linux`) directly in the extension.
- **Automatic OS Detection:** The extension now automatically detects the operating system and launches the correct language server binary.
- **WSL Compatibility:** Added specific support for running in Windows Subsystem for Linux (WSL) environments by serving the native Linux binary.

### Fixed
- **"Cargo Build" Requirement:** Removed the need for users to install Rust or run `cargo build` manually. The extension now works out-of-the-box.
- **Permission Errors:** Added automatic `chmod +x` (755) execution for binaries on macOS and Linux to prevent "permission denied" errors on first run.
- **Exec Format Error:** Fixed the crash where WSL/Linux environments attempted to execute the macOS binary by mistake.


## [1.0.1] - 2026-02-16

### Fixed
- Fixed missing syntax highlighting - moved `syntaxes/` folder to project root
- Updated TextMate grammar path in package.json to `./syntaxes/amaro.tmLanguage.json`

## [1.0.0] - 2026-02-16

### Added
- **QubitMap Index Type Checking:** `State.map[Gate.qubits[0]]` is now correctly validated — `QubitMap` accepts `Qubit` indexes, not just `Int`.
- **Qubit/Int Leniency:** `Qubit` and `Int` are treated as compatible index types since `Qubit` wraps a `usize`.
- **`get_transitions` Required Field:** `TransitionInfo` now enforces `get_transitions` as a required field, matching the compiler.
- **`shortest_path` Built-in:** Added `shortest_path(Arch, Vec<Location>, Vec<Location>, Vec<Location>) -> Option<Vec<Location>>` to the global symbol table.
- **`stack_size` Arch Field:** Added `stack_size : Int` as a valid field on `ArchT` to support ILQ-style architectures.
- **Gate Index Methods:** Added `x_indices()`, `y_indices()`, `z_indices()` to `Gate` type, each returning `Vec<Qubit>`.
- **Unknown Index Leniency:** Indexing on an `Unknown` type (e.g. `x.implementation.(path())`) is now silently accepted without a false error.
- **Examples Folder:** 4 production-ready `.qmrl` examples (NISQ, SCMR, ILQ, MQLSS) with detailed README.
- **Documentation:** Complete CONTRIBUTING.md with architecture guide, contribution scenarios, and troubleshooting. Doc comments on all public functions.
- **Auto-build:** `postinstall` npm script automatically builds LSP binary on `npm install`.

### Changed
- **`State.map` is a Zero-Arg Function:** Changed from a plain property (`QubitMap`) to a zero-arg function (`() -> QubitMap`) so both `State.map` and `State.map()` work correctly.
- **Index Error Message:** Improved to show the expected index type (e.g. `Expected 'Qubit' but got 'Int'`) instead of a generic message.
- **`State.implemented_gates` Type:** Changed from `Vec<Gate>` to `Unknown` to accurately reflect its complex `HashSet<ImplementedGate<T>>` return type while avoiding false errors.
- **Code Quality:** Fixed all Clippy warnings. Refactored `SymbolTable::new()` into named helper methods for maintainability.
- **Debug Mode:** Moved AST debug helpers under `#[cfg(debug_assertions)]` for cleaner release builds.

### Fixed
- Fixed false positives on `State.map[Gate.qubits[0]]` — the old check required all indexes to be `Int` regardless of the container type.
- Fixed `State.map()` being flagged as "attempted to call a non-function value."
- Fixed `x.implementation.(path())` triggering an index type mismatch error.
- Fixed duplicate index check that fired both the new context-aware check and the old hardcoded `Int` check simultaneously.


## [0.2.0] - 2026-02-10

### Added
- **Advanced Control Flow:** Added full support for chained `let ... in` bindings within `if-then-else` blocks.
- **Vector Semantics:** Added type checking for vector mutation methods (`push`, `pop`, `extend`) and helper functions (`all_paths`).
- **Tuple Indexing:** Added parser support for direct integer access on tuples (e.g., `transition.edge.(0)`).
- **Type Compatibility:** Added explicit type equivalence checks for `Arch`, `State`, and `Gate` types to allow passing them as function arguments.

### Changed
- **Unified Field Access:** Properties and zero-argument functions are now interchangeable (e.g., `State.map` and `State.map()`).
- **Parser Logic:** Updated `parse_postfix_expr` to accept integer literals after a dot, resolving parse errors on tuple access.
- **Type Inference:** Improved inference for empty vectors (`Vec()`) and `None` options.

### Fixed
- Fixed a critical issue where `let` bindings inside `then` blocks were being swallowed by the parser.
- Fixed a semantic error where `if-then-else` branches returning `Vec` and `Option` caused type mismatch errors.
- Fixed `RouteInfo` parsing to correctly identify `realize_gate` even when preceded by complex struct definitions.


## [0.1.0] - 2025-12-10

### Added
- **Core Parser & AST:**
    - Implemented a robust recursive descent parser for the Amaro language.
    - Added support for complex control flow (`if-then-else` with precedence).
    - Added support for scoped bindings (`let var = val in body`).
    - Added support for nested generic types (e.g., `Vec<Vec<Float>>`).
    - Added support for advanced method chaining and dynamic projection (`obj.(expr)`).
- **Language Server (LSP):**
    - Initial integration of the Rust-based `amaro-lsp` server.
    - Diagnostics for mandatory blocks (`RouteInfo`) and required fields.
    - Outline view and "Go to Symbol" navigation for Blocks, Structs, and Fields.
    - Fault-tolerant parsing with error recovery. Continues analyzing the file even after encountering syntax errors.
- **VS Code Extension:**
    - Syntax highlighting for `.qmrl` files, including embedded Rust blocks (`{{ ... }}`).
    - Client-side configuration to launch the LSP binary.

### Fixed
- Fixed operator precedence in mathematical and conditional expressions.
- Fixed parsing of newlines within `if-then-else` blocks.
- Resolved ambiguity between tuple projection (`.0`) and dynamic indexing (`.(path())`).
- Fixed concurrency safety for AST Node ID generation.
