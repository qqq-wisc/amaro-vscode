use std::collections::HashMap;

use tower_lsp::lsp_types::{Position, Range};

use crate::ast::{
    UnaryOperator, AmaroFile, BinaryOperator, BlockContent, BlockItem, Expr, ExprKind, NodeId, TypeAnnotation,
};

/// Each expression has a type, which is uniquely representable by an enum here.
#[derive(Clone, PartialEq)]
enum Type {
    /// Indicates that there really is no type, or the type could not be
    /// resolved. Almost always this means some sort of error.
    None,
    /// "Struct" type. Has fields and functions attached.
    TypeDef(TypeDef),
    /// Type of a function, containing arg types and a resulting type.
    Function(Function),
    /// Primitive, built-in types, like Int, Float, Vec, Tuple.
    Primitive(Primitive),
}

// #[derive(Clone, PartialEq)]
// enum Block {
//     RouteInfo,
//     TransitionInfo,
//     ArchInfo,
//     StateInfo,
// }

/// The possible structs in Amaro.
#[derive(Clone, PartialEq)]
enum TypeDef {
    GateRealization,
    Arch,
    State,
    Step,
    Architecture,
    Transition,
    Location,
    /// Not sure if Qubit has been phased out in favor of Location.
    Qubit
}

/// Type for functions.
/// Includes the argument types and the return type.
#[derive(Clone, PartialEq)]
struct Function {
    /// The types of the arguments for the function.
    args: Vec<Type>,
    /// The return type of the function.
    result: Box<Type>,
}

/// Type for primitive values.
#[derive(Clone, PartialEq)]
enum Primitive {
    /// Integer type
    Int,
    /// Floating point type
    Float,
    /// String type
    String,
    /// Boolean type
    Bool,
    /// Vec type. Vecs can only contain one type
    Vec(Box<Type>),
    /// Tuple type. Each entry in the tuple has its own type
    Tuple(Vec<Type>),
}


struct TypeDefFields {
    type_def: TypeDef,
    fields: HashMap<String, Type>,
}

impl TypeDefFields {}

fn type_annotation_to_primitive(type_annotation: &TypeAnnotation) -> Option<Primitive> {
    match type_annotation {
        TypeAnnotation::Simple(str) => {
            if str == "Int" {
                Some(Primitive::Int)
            } else if str == "Float" {
                Some(Primitive::Float)
            } else if str == "Bool" {
                Some(Primitive::Bool)
            } else if str == "String" {
                Some(Primitive::String)
            } else {
                None
            }
        }
        TypeAnnotation::Generic(str, type_annotations) => {
            if str == "Vec" {
                if type_annotations.len() != 1 {
                    None // issue! we only know of Vec with generics right now.
                // TODO figure out if other things can be generic'd!
                } else if let Some(prim) = type_annotation_to_primitive(&type_annotations[0]) {
                    Some(Primitive::Vec(Box::new(Type::Primitive(prim))))
                } else if let Some(typedef) = type_annotation_to_typedef(&type_annotations[0]) {
                    Some(Primitive::Vec(Box::new(Type::TypeDef(typedef))))
                } else {
                    None // not primitive and not typedef.. what could it be?
                }
            } else {
                None // we only know of Vec rn!
            }
        }
        TypeAnnotation::Tuple(type_annotations) => {
            let tuple_vec: Vec<Type> = type_annotations
                .iter()
                .map(|anno| {
                    if let Some(prim) = type_annotation_to_primitive(anno) {
                        Type::Primitive(prim)
                    } else if let Some(typedef) = type_annotation_to_typedef(anno) {
                        Type::TypeDef(typedef)
                    } else {
                        Type::None
                    }
                })
                .collect();

            Some(Primitive::Tuple(tuple_vec))
        }
    }
}

fn string_to_typedef(str: &str) -> Option<TypeDef> {
    if str == "GateRealization" {
        Some(TypeDef::GateRealization)
    } else if str == "Arch" {
        Some(TypeDef::Arch)
    } else if str == "Architecture" {
        Some(TypeDef::Architecture)
    } else if str == "State" {
        Some(TypeDef::State)
    } else if str == "Step" {
        Some(TypeDef::Step)
    } else if str == "Transition" {
        Some(TypeDef::Transition)
    } else {
        None
    }
}

fn type_annotation_to_typedef(type_annotation: &TypeAnnotation) -> Option<TypeDef> {
    match type_annotation {
        TypeAnnotation::Simple(str) => string_to_typedef(str),
        TypeAnnotation::Generic(_, _) => None,
        TypeAnnotation::Tuple(_) => None,
    }
}

struct TypeDefCollection {
    entries: Vec<TypeDefFields>,
}

impl TypeDefCollection {
    pub fn find(&self, typedef: &TypeDef) -> Option<&TypeDefFields> {
        self.entries.iter().find(|&elt| elt.type_def == *typedef)
    }
}

impl Default for TypeDefCollection {
    /// Sets up the TypeDefCollection to include all the standard, implicit
    /// functions/fields that are available on structs. For instance, Arch.width
    /// and Arch.contains_edge()
    fn default() -> Self {

        let mut fields = Vec::new();
        // this will be ugly. no getting around that.
        // we should just store a copy of the default or something, idk.
        fields.push(
            TypeDefFields { 
                type_def: TypeDef::Architecture, fields: HashMap::from([
                    (String::from("locations"), Type::Function(Function { 
                        args: Vec::new(), result: Box::new(Type::Primitive(Primitive::Vec(Box::new(Type::TypeDef(TypeDef::Location))))) 
                    })) ,
                ]) 
            });

        fields.push(
            TypeDefFields { 
                type_def: TypeDef::Transition, fields: HashMap::from([
                    (String::from("apply"), Type::Function(Function { 
                        args: vec![ Type::TypeDef(TypeDef::Step)],
                        result: Box::new(Type::TypeDef(TypeDef::Step)) 
                    })) ,

                    (String::from("repr"), Type::Function(Function { 
                        args: Vec::new(),
                        result: Box::new(Type::Primitive(Primitive::String)) 
                    })) ,

                    (String::from("cost"), Type::Function(Function { 
                        args: vec![ Type::TypeDef(TypeDef::Architecture)],
                        result: Box::new(Type::Primitive(Primitive::Float)) 
                    })) ,
                ]) 
            });

        fields.push(
            TypeDefFields { 
                type_def: TypeDef::Arch, fields: HashMap::from([
                    (String::from("width"), Type::Primitive(Primitive::Int)) ,
                    (String::from("height"), Type::Primitive(Primitive::Int)) ,
                ]) 
            });
        

        Self { entries: fields }
    }
}

/// Struct used to represent scope.
/// Oftentimes, identifiers only mean something for a limited period of time.
/// For instance, in a map or fold expression.
/// Or with let bindings.
/// This is how we remember and represent these identifiers.
/// 
/// # Usage
/// - Create with ::default()
/// - Scope up with .scope_up()
/// - Scope down with .scope_down()
/// - Add identifier-type mappings to the current scope with .add()
/// - Get type mappings from identifiers at the current scope with .get()
struct IdentifierScope {
    /// Each entry defines an identifier, type pair.
    /// Later entries in the list indicate that they are deeper scope, and thus
    /// should be used first.
    /// When we scope up or scope down, we push/pop the entries in this list.
    entries: Vec<(String, Type)>,
    /// This tells us the goal length of the entries vec at different scopes.
    /// When we scope up, we remove the last element of this list and then 
    /// shrink the entries vec to match the length of this popped element.
    /// When we scope up, we remember the size of the entries vec prior to
    /// scoping up and add it to this list so we can return later.
    scopes: Vec<usize>,
}

impl Default for IdentifierScope {
    /// Cre
    fn default() -> Self {
        // TODO do we need to put in things like Arch? I dont think so. I think
        // that goes in typedefs and stuff.
        Self {
            entries: Vec::new(),
            scopes: Vec::new(),
        }
    }
}

impl IdentifierScope {
    /// Increases level of scope by 1.
    pub fn scope_up(&mut self) {
        self.scopes.push(self.entries.len());
    }
    /// Decreases level of scope by 1.
    /// This removes all identifier-type mappings from the current scope.
    /// Don't call this if you can't scope down (i.e are at global scope).
    /// This will result in a panic.
    /// If you do this somehow, that's on YOU!
    /// Realistically, this just shouldn't happen!
    pub fn scope_down(&mut self) {
        let goal_num = self.scopes.pop().unwrap();
        while self.entries.len() > goal_num {
            self.entries.pop();
        }
    }
    
    /// Add an identifier-type mapping at the current scope.
    pub fn add(&mut self, new_identifier: String, new_type: Type) {
        self.entries.push((new_identifier, new_type));
    }

    /// Get the highest scope, available type mapping for this identifier.
    pub fn get(&self, identifier: &str) -> Option<&Type> {
        self.entries.iter().rev().find_map(|elt| {
            if elt.0 == identifier {
                Some(&elt.1)
            } else {
                None
            }
        })
    }
}

/// From a file, resolves all the global struct definitions.
/// For instance, if the user defines Transition, its fields are recognized
/// through this method.
fn resolve_typedefs(file: &AmaroFile) -> Vec<TypeDefFields> {
    let mut type_def_fields_vec: Vec<TypeDefFields> = Vec::new();

    for block in &file.blocks {
        let items = match &block.content {
            BlockContent::Fields(block_items) => block_items,
        };

        items
            .iter()
            .filter_map(|elt| {
                if let BlockItem::StructDef(def) = elt {
                    Some(def)
                } else {
                    None
                }
            })
            .filter_map(|def| match string_to_typedef(&def.name) {
                None => None,
                Some(typedef) => {
                    let mut hash_map = HashMap::new();
                    def.fields.iter().for_each(|field| {
                        let field_type = if let Some(prim) =
                            type_annotation_to_primitive(&field.type_annotation)
                        {
                            Type::Primitive(prim)
                        } else if let Some(typedef) =
                            type_annotation_to_typedef(&field.type_annotation)
                        {
                            Type::TypeDef(typedef)
                        } else {
                            Type::None
                        };

                        hash_map.insert(field.name.clone(), field_type);
                    });

                    Some(TypeDefFields {
                        type_def: typedef,
                        fields: hash_map,
                    })
                }
            })
            .for_each(|elt| type_def_fields_vec.push(elt));
    }
    type_def_fields_vec
}

// before this, we'll need something that goes through and resolves all the
// typedef and function stuff, so we can easily see the expected types of
// things
fn resolve_expr_type(
    expr: &Expr,
    type_map: &mut HashMap<NodeId, Type>,
    typedefs: &TypeDefCollection,
    identifier_scope: &mut IdentifierScope,
) {
    match &expr.kind {
        ExprKind::Identifier(identifier) => {
            match identifier_scope.get(identifier) {
                Some(t) => type_map.insert(expr.id, t.clone()),
                None => type_map.insert(expr.id, Type::None), // error??
            }
        }
        ExprKind::IntLiteral(_) => type_map.insert(expr.id, Type::Primitive(Primitive::Int)),
        ExprKind::FloatLiteral(_) => type_map.insert(expr.id, Type::Primitive(Primitive::Float)),
        ExprKind::StringLiteral(_) => type_map.insert(expr.id, Type::Primitive(Primitive::String)),
        ExprKind::BoolLiteral(_) => type_map.insert(expr.id, Type::Primitive(Primitive::Bool)),
        ExprKind::List(exprs) => {
            if exprs.len() == 0 {
                type_map.insert(
                    expr.id,
                    Type::Primitive(Primitive::Vec(Box::new(Type::None))),
                )
            } else {
                // add all the types
                for other_expr in exprs {
                    resolve_expr_type(other_expr, type_map, typedefs, identifier_scope);
                }
                type_map.insert(
                    expr.id,
                    Type::Primitive(Primitive::Vec(Box::new(
                        type_map.get(&exprs[0].id).unwrap().clone(),
                    ))),
                )
            }
        }
        ExprKind::Tuple(exprs) => {
            if exprs.len() == 0 {
                type_map.insert(expr.id, Type::Primitive(Primitive::Tuple(Vec::new())))
            } else {
                let mut tuple_type_vec: Vec<Type> = Vec::new();
                // add all the types
                for other_expr in exprs {
                    resolve_expr_type(other_expr, type_map, typedefs, identifier_scope);
                    tuple_type_vec.push(type_map.get(&other_expr.id).unwrap().clone())
                }
                type_map.insert(expr.id, Type::Primitive(Primitive::Tuple(tuple_type_vec)))
            }
        }
        ExprKind::StructLiteral { name, fields } => {
            // TODO this one is a mess.

            // find the associated struct
            if let Some(typedef) = string_to_typedef(name) {
                if let Some(map) = typedefs.find(&typedef) {
                    // TODO map is bad name
                    fields.iter().for_each(|(field_name, field_expr)| {
                        // get type of field expr
                        resolve_expr_type(field_expr, type_map, typedefs, identifier_scope);
                        // get expected type by name
                        match map.fields.get(field_name) {
                            None => {
                                // ?? so they tried putting an invalid field?
                                // what to even do w this?
                            }
                            Some(found_type) => {
                                let inner_expr_type = type_map.get(&field_expr.id).unwrap();

                                if *found_type != *inner_expr_type {
                                    // TODO error here
                                    // how should we track errors? should we?
                                }
                            }
                        }
                    });
                    type_map.insert(expr.id, Type::TypeDef(typedef))
                } else {
                    type_map.insert(expr.id, Type::None)
                }
            } else {
                type_map.insert(expr.id, Type::None) // error, not a valid struct.
            }
        }
        // function call is hard.
        // the function expression could be a function that is literally detailed
        // like with a lambda.
        // it could also be one of the existing functions
        // but wait, that actually doesnt make this very tricky
        ExprKind::FunctionCall { function, args } => {
            resolve_expr_type(function, type_map, typedefs, identifier_scope);

            let function_type = match type_map.get(&function.id).unwrap() {
                Type::Function(func) => func.clone(),
                _ => {
                    type_map.insert(expr.id, Type::None);
                    return;
                }
            };

            // TODO compare the sizes of the args and function_type.args vecs,
            // for error stuff

            // go thru and do the args, then compare?

            function_type
                .args
                .iter()
                .zip(args.iter())
                .for_each(|entry| {
                    resolve_expr_type(entry.1, type_map, typedefs, identifier_scope);
                    let found_arg_type = type_map.get(&entry.1.id).unwrap();
                    if *found_arg_type != *entry.0 {
                        // ERROR message here!
                        // TODO error... or we can do this later! idc!
                    }
                });

            let found_type = (*function_type.result).clone();
            type_map.insert(expr.id, found_type)
        }
        ExprKind::FieldAccess { object, field } => {
            // get the type of the left thing
            // should be a TypeDef
            resolve_expr_type(object, type_map, typedefs, identifier_scope);

            if let Type::TypeDef(typedef) = type_map.get(&object.id).unwrap() {
                // go thru and find the typedef
                match typedefs.find(typedef) {
                    Some(found) => {
                        match found.fields.get(field) {
                            None => type_map.insert(expr.id, Type::None), // field not there??
                            Some(found_type) => type_map.insert(expr.id, found_type.clone()),
                        }
                    }
                    None => type_map.insert(expr.id, Type::None), // it's referencing a non-typedef??? hmm...
                }
            } else {
                type_map.insert(expr.id, Type::None)
            }
        }
        ExprKind::IndexAccess { object, index } => {
            // need to make sure it's a vec, otherwise cant index access
            resolve_expr_type(object, type_map, typedefs, identifier_scope);

            if let Type::Primitive(Primitive::Vec(vtype)) = type_map.get(&object.id).unwrap() {
                type_map.insert(expr.id, (**vtype).clone())
            } else {
                type_map.insert(expr.id, Type::None) // untyped bc not a vec! cant do this.
            }
        }
        // THIS is tough.
        // we have no idea what the parameters represent in this way.
        // this depends on the context that the lambda is used in.
        // this will require more info being in this method. which sucks.
        ExprKind::Lambda { params, body } => todo!(),
        ExprKind::IfThenElse {
            condition,
            then_branch,
            else_branch,
        } => {
            // condition type
            resolve_expr_type(condition, type_map, typedefs, identifier_scope);
            resolve_expr_type(then_branch, type_map, typedefs, identifier_scope);
            resolve_expr_type(else_branch, type_map, typedefs, identifier_scope);
            // cool, so we have their types. let's just assume that this evals
            // to the then branch, bc we're not doing any verification that
            // the types make sense yet.
            type_map.insert(expr.id, type_map.get(&then_branch.id).unwrap().clone())
        }
        // this relies upon some kind of identifier storage place
        ExprKind::LetBinding { name, value, body } => {
            // get the type of value
            resolve_expr_type(value, type_map, typedefs, identifier_scope);

            identifier_scope.scope_up();
            identifier_scope.add(name.clone(), type_map.get(&value.id).unwrap().clone());

            resolve_expr_type(body, type_map, typedefs, identifier_scope);

            identifier_scope.scope_down();

            type_map.insert(expr.id, type_map.get(&body.id).unwrap().clone())
        }
        ExprKind::BinaryOp { op, left, right } => {
            resolve_expr_type(left, type_map, typedefs, identifier_scope);
            resolve_expr_type(right, type_map, typedefs, identifier_scope);

            let left_type = type_map.get(&left.id).unwrap();
            let right_type = type_map.get(&right.id).unwrap();

            // TODO is this accurate? that only two of the same type can have
            // binary op?
            if left_type != right_type {
                // TODO error
                type_map.insert(expr.id, Type::None);
                return;
            }

            match op {
                BinaryOperator::Add
                | BinaryOperator::Sub
                | BinaryOperator::Mul
                | BinaryOperator::Div
                | BinaryOperator::Mod => {
                    match left_type {
                        Type::Primitive(Primitive::Int) => type_map.insert(expr.id, Type::Primitive(Primitive::Int)),
                        Type::Primitive(Primitive::Float) => type_map.insert(expr.id, Type::Primitive(Primitive::Float)),
                        _ => type_map.insert(expr.id, Type::None)
                    }
                },

                BinaryOperator::Eq | BinaryOperator::Ne => {
                    type_map.insert(expr.id, Type::Primitive(Primitive::Bool))
                }

                BinaryOperator::Lt
                | BinaryOperator::Le
                | BinaryOperator::Gt 
                | BinaryOperator::Ge => {
                    match left_type {
                        Type::Primitive(Primitive::Int|Primitive::Float) => type_map.insert(expr.id, Type::Primitive(Primitive::Bool)),
                        _ => type_map.insert(expr.id, Type::None)
                    }
                },
                BinaryOperator::And
                | BinaryOperator::Or => {
                    match left_type {
                        Type::Primitive(Primitive::Bool) => type_map.insert(expr.id, Type::Primitive(Primitive::Bool)),
                        _ => type_map.insert(expr.id, Type::None)
                    }
                },
                BinaryOperator::Range => todo!(), // what is this? is this like 0..12 ?? So range is a type?
                BinaryOperator::Tensor => todo!(), // what is this?? TODO what is tensor?
            }
        }
        ExprKind::UnaryOp { op, operand } => {
            resolve_expr_type(operand, type_map, typedefs, identifier_scope);
            let operand_type = type_map.get(&operand.id).unwrap();
            let prim = match operand_type {
                Type::Primitive(prim) => prim,
                _ => {
                    type_map.insert(expr.id, Type::None); // only primitives allowed rn
                    return
                }
            };



            match (op, prim) {
                (UnaryOperator::Not, Primitive::Int) => type_map.insert(expr.id, Type::Primitive(Primitive::Int)),
                (UnaryOperator::Not, Primitive::Float) => type_map.insert(expr.id, Type::Primitive(Primitive::Float)),
                (UnaryOperator::Neg, Primitive::Bool) => type_map.insert(expr.id, Type::Primitive(Primitive::Bool)),
                _ => type_map.insert(expr.id, Type::None)
            }
        },
        // what? option?
        ExprKind::Some(expr) => todo!(),
        // what? option?
        ExprKind::None => todo!(),
        // huh
        ExprKind::TensorProduct { left, right } => todo!(),
        // this is tuple projection ok, should be easy
        ExprKind::Projection { index, tuple } => {
            resolve_expr_type(tuple, type_map, typedefs, identifier_scope);

            match type_map.get(&tuple.id).unwrap() {
                Type::Primitive(Primitive::Tuple(tuple_type)) => {
                    match tuple_type.get(*index) {
                        None => {
                            // TODO out of bounds error
                            type_map.insert(expr.id, Type::None)
                        }
                        Some(i) => type_map.insert(expr.id, i.clone()),
                    }
                }
                _ => type_map.insert(expr.id, Type::None),
            }
        }
    };

    todo!()
}

pub fn get_symbols_from_file(file: &AmaroFile) {
    // should prob add default typedefs?

    for block in &file.blocks {
        let items = match &block.content {
            BlockContent::Fields(block_items) => block_items,
        };
        for item in items {
            match item {
                BlockItem::Field(field) => {
                    // TODO what to do about the key?
                    // that the function? we could prob gather some info
                    // abt the function...
                    // in fact, we will have to. we will need to use the fcn
                    // name in order to determine which argument it is, for the
                    // maps and stuff.
                    let mut cur_expr = &field.value;
                }
                BlockItem::StructDef(struct_def) => todo!(),
            }
        }
    }

    todo!()
}
