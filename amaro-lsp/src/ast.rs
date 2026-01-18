use tower_lsp::lsp_types::Range;

#[derive(Debug, Clone)]
pub struct AmaroFile {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub kind: String,
    pub range: Range,
    pub fields: Vec<Field>, 
}

#[derive(Debug, Clone)]
pub struct Field {
    pub key: String,
    pub key_range: Range,
    pub value: String,
    pub value_range: Range,
}
