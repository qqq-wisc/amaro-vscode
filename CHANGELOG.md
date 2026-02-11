# Change Log

All notable changes to the "amaro-vscode" extension will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/).

## [0.2.0] - 2026-02-10

### Added
- **Advanced Control Flow:** Added full support for chained `let ... in` bindings within `if-then-else` blocks.
- **Vector Semantics:** Added type checking for vector mutation methods (`push`, `pop`, `extend`) and helper functions (`all_paths`).
- **Tuple Indexing:** Added parser support for direct integer access on tuples (e.g., `transition.edge.0`).
- **Type Compatibility:** Added explicit type equivalence checks for `Arch`, `State`, and `Gate` types to allow passing them as function arguments.

### Changed
- **Unified Field Access:** Refactored semantics to treat Properties and Zero-Argument Functions interchangeably (e.g., `State.map` and `State.map()` are both valid).
- **Parser Logic:** Updated `parse_postfix_expr` to accept integer literals after a dot, resolving parse errors on tuple access.
- **Type Inference:** Improved inference for empty Vectors (`Vec()`) and empty Options (`None`) when matching against typed branches.

### Fixed
- Fixed a critical issue where `let` bindings inside `then` blocks were being swallowed by the parser.
- Fixed a semantic error where `if-then-else` branches returning `Vec` and `Option` caused type mismatch panics (now strictly enforced).
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
    - **Diagnostics:** Semantic validation for mandatory blocks (`RouteInfo`) and required fields.
    - **Symbols:** Outline view and "Go to Symbol" navigation for Blocks, Structs, and Fields.
    - **Error Recovery:** Parser continues analyzing the file even after encountering syntax errors.
- **VS Code Extension:**
    - Syntax highlighting for `.qmrl` files, including embedded Rust blocks (`{{ ... }}`).
    - Client-side configuration to launch the LSP binary.

### Fixed
- Fixed operator precedence issues in mathematical and conditional expressions.
- Fixed parsing of newlines within `if-then-else` blocks.
- Resolved ambiguity between tuple projection (`.0`) and dynamic indexing (`.(path())`).
- Fixed concurrency safety for AST Node ID generation.
