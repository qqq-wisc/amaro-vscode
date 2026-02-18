# Change Log

All notable changes to the "amaro-vscode" extension will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/).

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
