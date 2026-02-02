# Change Log

All notable changes to the "amaro-vscode" extension will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/).

## [0.1.0] - 2026-02-02

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
