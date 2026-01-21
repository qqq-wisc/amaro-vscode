#![allow(dead_code)]

use tower_lsp::lsp_types::Range;
use std::sync::atomic::{AtomicU32, Ordering};

// Global node ID counter for unique AST node identification
static NEXT_NODE_ID: AtomicU32 = AtomicU32::new(0);

pub fn next_node_id() -> NodeId {
    NodeId(NEXT_NODE_ID.fetch_add(1, Ordering::Relaxed))
}

// Commented since this was causing race condition in tests
// pub fn reset_node_ids() {
//     NEXT_NODE_ID.store(0, Ordering::Relaxed);
// }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

#[derive(Debug, Clone)]
pub struct AmaroFile {
    pub blocks: Vec<Block>,
    pub id: NodeId,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub kind: String,
    pub range: Range,
    pub content: BlockContent,
    pub id: NodeId,
}

#[derive(Debug, Clone)]
pub enum BlockContent {
    Fields(Vec<BlockItem>),
}

#[derive(Debug, Clone)]
pub enum BlockItem {
    Field(Field),
    StructDef(StructDef),
}

#[derive(Debug, Clone)]
pub struct Field {
    pub key: String,
    pub key_range: Range,
    pub value: Expr,
    pub value_range: Range,
    pub id: NodeId,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub name_range: Range,
    pub fields: Vec<TypedParam>,
    pub range: Range,
    pub id: NodeId,
}

#[derive(Debug, Clone)]
pub struct TypedParam {
    pub name: String,
    pub type_annotation: TypeAnnotation,
    pub range: Range,
    pub id: NodeId,
}

#[derive(Debug, Clone)]
pub enum TypeAnnotation {
    Simple(String),
    Generic(String, Vec<TypeAnnotation>),
    Tuple(Vec<TypeAnnotation>),
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub range: Range,
    pub id: NodeId,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    // Literals
    Identifier(String),
    IntLiteral(i64),
    FloatLiteral(f64),
    StringLiteral(String),
    BoolLiteral(bool),
    
    // Collections
    List(Vec<Expr>),
    Tuple(Vec<Expr>),
    
    // Struct construction
    StructLiteral {
        name: String,
        fields: Vec<(String, Expr)>,
    },
    
    // Function application
    FunctionCall {
        function: Box<Expr>,
        args: Vec<Expr>,
    },
    
    // Field access
    FieldAccess {
        object: Box<Expr>,
        field: String,
    },
    
    // Indexing
    IndexAccess {
        object: Box<Expr>,
        index: Box<Expr>,
    },
    
    // Lambda expressions
    Lambda {
        params: Vec<String>,
        body: Box<Expr>,
    },
    
    // Control flow
    IfThenElse {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
    },
    
    // Let binding
    LetBinding {
        name: String,
        value: Box<Expr>,
        body: Box<Expr>,
    },
    
    // Binary operations
    BinaryOp {
        op: BinaryOperator,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    
    // Unary operations
    UnaryOp {
        op: UnaryOperator,
        operand: Box<Expr>,
    },
    
    // Option types
    Some(Box<Expr>),
    None,
    
    // Amaro-specific operators
    TensorProduct {
        left: Box<Expr>,
        right: Box<Expr>,
    },
    
    Projection {
        index: usize,
        tuple: Box<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOperator {
    // Arithmetic
    Add, Sub, Mul, Div, Mod,
    
    // Comparison
    Eq, Ne, Lt, Le, Gt, Ge,
    
    // Logical
    And, Or,
    
    // Range
    Range,
    
    // Amaro-specific
    Tensor,  // ⊗
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOperator {
    Not,
    Neg,
}

impl Expr {
    pub fn new(kind: ExprKind, range: Range) -> Self {
        Expr { 
            kind, 
            range,
            id: next_node_id(),
        }
    }
    
    pub fn identifier(name: String, range: Range) -> Self {
        Expr::new(ExprKind::Identifier(name), range)
    }
    
    pub fn int(value: i64, range: Range) -> Self {
        Expr::new(ExprKind::IntLiteral(value), range)
    }
    
    pub fn float(value: f64, range: Range) -> Self {
        Expr::new(ExprKind::FloatLiteral(value), range)
    }
    
    pub fn string(value: String, range: Range) -> Self {
        Expr::new(ExprKind::StringLiteral(value), range)
    }
    
    pub fn bool(value: bool, range: Range) -> Self {
        Expr::new(ExprKind::BoolLiteral(value), range)
    }
    
    pub fn summarize(&self) -> String {
        self.summarize_with_limit(50)
    }
    
    pub fn summarize_with_limit(&self, limit: usize) -> String {
        let full = self.format_summary();
        if full.len() <= limit {
            full
        } else {
            format!("{}...", &full[..limit.saturating_sub(3)])
        }
    }
    
    fn format_summary(&self) -> String {
        match &self.kind {
            ExprKind::Identifier(name) => name.clone(),
            ExprKind::IntLiteral(n) => n.to_string(),
            ExprKind::FloatLiteral(f) => f.to_string(),
            ExprKind::StringLiteral(s) => format!("'{}'", s),
            ExprKind::BoolLiteral(b) => b.to_string(),
            ExprKind::List(items) => {
                if items.is_empty() {
                    "[]".to_string()
                } else if items.len() <= 3 {
                    format!("[{}]", items.iter()
                        .map(|e| e.format_summary())
                        .collect::<Vec<_>>()
                        .join(", "))
                } else {
                    format!("[{}, ... +{} more]", 
                        items[0].format_summary(), 
                        items.len() - 1)
                }
            }
            ExprKind::Tuple(items) => {
                if items.len() <= 2 {
                    format!("({})", items.iter()
                        .map(|e| e.format_summary())
                        .collect::<Vec<_>>()
                        .join(", "))
                } else {
                    format!("({}, ... +{} more)", 
                        items[0].format_summary(), 
                        items.len() - 1)
                }
            }
            ExprKind::StructLiteral { name, fields } => {
                format!("{}{{{} fields}}", name, fields.len())
            }
            ExprKind::FunctionCall { function, args } => {
                format!("{}(...)", function.format_summary())
            }
            ExprKind::FieldAccess { object, field } => {
                format!("{}.{}", object.format_summary(), field)
            }
            ExprKind::Lambda { params, .. } => {
                format!("|{}| -> ...", params.join(", "))
            }
            ExprKind::IfThenElse { .. } => "if-then-else".to_string(),
            ExprKind::LetBinding { name, .. } => format!("let {}", name),
            ExprKind::Some(_) => "Some(...)".to_string(),
            ExprKind::None => "None".to_string(),
            ExprKind::TensorProduct { .. } => "⊗".to_string(),
            ExprKind::Projection { index, .. } => format!("proj_{}", index),
            _ => "...".to_string(),
        }
    }
}

impl Block {
    pub fn new(kind: String, range: Range, content: BlockContent) -> Self {
        Block {
            kind,
            range,
            content,
            id: next_node_id(),
        }
    }
}

impl Field {
    pub fn new(key: String, key_range: Range, value: Expr, value_range: Range) -> Self {
        Field {
            key,
            key_range,
            value,
            value_range,
            id: next_node_id(),
        }
    }
}

impl StructDef {
    pub fn new(name: String, name_range: Range, fields: Vec<TypedParam>, range: Range) -> Self {
        StructDef {
            name,
            name_range,
            fields,
            range,
            id: next_node_id(),
        }
    }
}

impl TypedParam {
    pub fn new(name: String, type_annotation: TypeAnnotation, range: Range) -> Self {
        TypedParam {
            name,
            type_annotation,
            range,
            id: next_node_id(),
        }
    }
}

impl AmaroFile {
    pub fn new(blocks: Vec<Block>) -> Self {
        AmaroFile {
            blocks,
            id: next_node_id(),
        }
    }
}
