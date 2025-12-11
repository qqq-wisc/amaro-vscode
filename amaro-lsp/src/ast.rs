use tower_lsp::lsp_types::Range;

#[derive(Debug, Clone)]
pub struct AmaroFile {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub kind: String,
    pub range: Range,
}
