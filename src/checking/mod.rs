//! Contains structures and enumerations used during the checking of scoping and
//! typing rules, as well as ones for representing the final immediate representation
//! of a till program. For the actual checking code, see submodule `checker`.

pub mod checker;

use crate::stream;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Failure {
    NonexistentPrimitiveType(String),
    VariableNotInScope(stream::Position, String),
    FunctionUndefined(stream::Position, String, Vec<Type>),
    VoidFunctionInExpr(stream::Position, String, Vec<Type>),
    RedefinedExistingFunction(String, Vec<Type>),
    VoidFunctionReturnsValue(stream::Position, String, Vec<Type>, Type),
    FunctionUnexpectedReturnType {
        pos: stream::Position,
        identifier: String, params: Vec<Type>,
        expected: Type, encountered: Option<Type>,
    },
    VariableRedeclaredToDifferentType {
        identifier: String,
        expected: Type, encountered: Type
    },
    UnexpectedType { pos: stream::Position, expected: Type, encountered: Type },
    InvalidTopLevelStatement,
    NestedFunctions(stream::Position, String),
    MainUndefined
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Failure::NonexistentPrimitiveType(ident) =>
                write!(f, "The primitive type '{}' does not exist - please use either Num, Char or Bool", ident),

            Failure::VariableNotInScope(pos, ident) =>
                write!(f, "Reference made at {} to variable '{}' which is either undefined or inaccessible from the current scope",
                       pos, ident),

            Failure::FunctionUndefined(pos, ident, params) =>
                write!(f, "Call made at {} to function '{}' with parameter types {:?} which is not yet defined",
                       pos, ident, params),

            Failure::VoidFunctionInExpr(pos, ident, params) =>
                write!(f, "Function '{}' with parameter types {:?} has no return value and so cannot be used in an expression at {}",
                       ident, params, pos),

            Failure::RedefinedExistingFunction(ident, params) =>
                write!(f, "Function '{}' with parameter types {:?} has already been defined",
                       ident, params),

            Failure::VoidFunctionReturnsValue(pos, ident, params, ret_type) =>
                write!(f, "Function '{}' with parameter types {:?} at {} defined without return type yet has a block that returns a value of type {:?}",
                       ident, params, pos, ret_type),

            Failure::FunctionUnexpectedReturnType { pos, identifier, params, expected, encountered } => {
                let encountered_as_string = {
                    if let Some(encountered_type) = encountered { format!("{:?}", encountered_type) }
                    else { "nothing".to_string() }
                };
                write!(f, "Function '{}' with parameter types {:?} at {} expected to return a value of type {:?} yet found to return {}",
                       identifier, params, pos, expected, encountered_as_string)
            }

            Failure::VariableRedeclaredToDifferentType { identifier, expected, encountered } =>
                write!(f, "Attempt made to redeclare variable '{}' of type {:?} to different type {:?} in the same scope",
                       identifier, expected, encountered),

            Failure::UnexpectedType { pos, expected, encountered } =>
                write!(f, "Expected type {:?} yet enountered {:?} at {}",
                       expected, encountered, pos),
            
            Failure::InvalidTopLevelStatement =>
                write!(f, "Only global variable and function definition statements are allowed at the top-level"),

            Failure::NestedFunctions(pos, ident) =>
                write!(f, "Function '{}' at {} cannot be defined as it is contained within the body of another function", ident, pos),

            Failure::MainUndefined =>
                write!(f, "All till programs are required to have a main function yet such a function could not be found")
        }
    }
}

type Result<T> = std::result::Result<T, Failure>;

/// Represents the types available in till: `Char`, `Num`, and `Bool`.
#[derive(Clone, Debug, PartialEq)]
pub enum Type { Char, Num, Bool }

impl Type {
    fn from_identifier(ident: &str) -> Result<Type> {
        match ident {
            "Char" => Ok(Type::Char),
            "Num" => Ok(Type::Num),
            "Bool" => Ok(Type::Bool),
            _ => Err(Failure::NonexistentPrimitiveType(ident.to_string()))
        }
    }
}

/// Represents a scope within a till program. A new scope is created in the body
/// of a function definition, if statement, or while statement. Any variables
/// declared in a given scope will only be accessible from within that scope or
/// from a scope nested in it.
#[derive(Debug)]
struct Scope { variables: Vec<VariableDef> }

impl Scope {
    fn find_variable_def(&self, ident: &str) -> Option<&VariableDef> {
        for def in &self.variables {
            if def.identifier == ident { return Some(def) }
        }
        None
    }
}

pub type Id = usize;

/// Definition of a variable with a given identifier and type.
#[derive(Debug, PartialEq)]
struct VariableDef {
    identifier: String,
    var_type: Type,
    id: Id
}

/// Definition of a function with an identifier, set of parameters, and a return
/// type.
#[derive(Debug, PartialEq)]
struct FunctionDef {
    identifier: String,
    parameter_types: Vec<Type>,
    return_type: Option<Type>,
    label: String
}

#[derive(Debug, PartialEq)]
pub enum Value {
    /// Value is determined by that of the variable with the specified ID.
    Variable(Id),
    Num(f64),
    Char(char),
    Bool(bool)
}

/// Represents the simple, assembly-like instructions that make up the final
/// immediate representation of a till program.
#[derive(Debug, PartialEq)]
pub enum Instruction {
    /// Create a global variable with a given ID.
    //Global(Id),
    /// Create a function parameter with a given ID.
    Parameter(Id),
    /// Reserve stack space for a local variable with a given ID.
    Local(Id),
    /// Pop a value off the stack and store it in the specified variable.
    Store(Id),
    /// Push the specified value onto the stack.
    Push(Value),
    /// Identify a point in the series of instructions that can be jumped to (e.g.
    /// the beginning of a function or loop).
    Label(Id),
    /// Identify the start of a function which can be later called upon.
    Function { label: String, local_variable_count: usize },
    /// Jump to the function with the specified label, return here when return
    /// instruction encountered. The function called should not return a value.
    CallExpectingVoid(String),
    CallExpectingValue(String),
    /// Return from call, returning value on top of stack. Will also result in
    /// the deallocation of all variables allocated since the last begin scope
    /// instruction.
    ReturnValue,
    /// Return from call without including a value. Also deallocates all variables
    /// since the last begin scope instruction.
    ReturnVoid,
    /// Pop value off stack and display via stdout.
    Display { value_type: Type, line_number: u64 },
    /// Jump to a given label.
    Jump(Id),
    /// Pop a value off the stack, if that value is true then jump to the particular
    /// label indicated by the given ID.
    JumpIfTrue(Id),
    JumpIfFalse(Id),
    /// Pop 2 items off the stack, push true if they are equal, false otherwise.
    Equals,
    GreaterThan,
    LessThan,
    Add,
    Subtract,
    Multiply,
    Divide,
    /// Pop top of stack, perform boolean not, push result.
    Not
}