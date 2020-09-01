use crate::parsing;
use std::fmt;

pub fn input<T: Iterator<Item=parsing::Statement>>(stmts: T) -> Vec<Result<parsing::Statement, Failure>> {
    Checker::new(stmts).collect() // Collected so that checking happens immediately.
}

#[derive(Debug, PartialEq)]
pub enum Failure { // TODO: Show stream position in error messages.
    VariableNotInScope(String),
    FunctionNotInScope(String, Vec<super::Type>),
    VoidFunctionInExpr(String, Vec<super::Type>),
    UnexpectedType { expected: super::Type, encountered: super::Type }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Failure::VariableNotInScope(ident) => write!(f, "Reference made to variable with identifier `{}` which is either undefined and inaccessible from the current scope", ident),
            Failure::FunctionNotInScope(ident, params) => write!(f, "Call made to a function '{}' with parameters {:?} which is either undefined or inaccessible from the current scope", ident, params),
            Failure::VoidFunctionInExpr(ident, params) => write!(f, "Function '{}' with parameters {:?} has no return value and so cannot be used in an expression", ident, params),
            Failure::UnexpectedType { expected, encountered } => write!(f, "Expected type {} yet enountered {}", expected, encountered)
        }
    }
}

pub struct Checker<T: Iterator<Item=parsing::Statement>> {
    stmts: T,
    scope_stack: Vec<Scope>
}

impl<T: Iterator<Item=parsing::Statement>> Iterator for Checker<T> {
    type Item = Result<parsing::Statement, Failure>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.stmts.next() {
            Some(stmt) => {
                match self.check_stmt(&stmt) {
                    Err(e) => Some(Err(e)),
                    Ok(_) => Some(Ok(stmt))
                }
            }

            None => {
                log::trace!("Reached end of statement stream - ending program scope");
                self.end_scope();
                assert!(self.scope_stack.is_empty());
                None
            }
        }
    }
}

impl<T: Iterator<Item=parsing::Statement>> Checker<T> {
    fn new(stmts: T) -> Checker<T> {
        let mut this = Checker { stmts: stmts, scope_stack: Vec::new() };
        this.begin_new_scope();
        this
    }

    fn check_stmt(&mut self, stmt: &parsing::Statement) -> Result<(), Failure> {
        match stmt {
            parsing::Statement::If { condition, block } |
            parsing::Statement::While { condition, block } => {
                self.expect_expr_type(condition, super::Type::Bool)?;
                self.check_block(block)?; // The return type of the block is irrelevant.
                Ok(())
            }
            _ => unimplemented!()
        }
    }

    /// Iterate over the statements contained in a block, checking each. Should
    /// a return statement be encountered, the type of the returned expression
    /// is returned within `Ok(Some(...))`. If there are multiple return statements,
    /// then it will be ensured that they are all returning the same type.
    fn check_block(&mut self, block: &parsing::Block) -> Result<Option<super::Type>, Failure> {
        self.begin_new_scope();
        for stmt in block { self.check_stmt(stmt)? }
        self.end_scope();

        Ok(None) // TODO: temp
    }

    fn begin_new_scope(&mut self) {
        self.scope_stack.push(Scope { 
            variable_defs: Vec::new(),
            function_defs: Vec::new()
        });
    }

    fn end_scope(&mut self) {
        self.scope_stack.pop();
    }

    fn get_inner_scope(&mut self) -> &mut Scope { self.scope_stack.last_mut().unwrap() }

    /// Search the current accessible scopes for the variable definition with
    /// the given identifier.
    fn variable_lookup(&self, ident: &str) -> Result<&VariableDef, Failure> {
        // Reverse the iterator so that the inner most scope has priority (i.e.
        // automatically handle shadowing).
        for scope in self.scope_stack.iter().rev() {
            if let Some(var_def) = scope.find_variable_def(ident) {
                return Ok(var_def)
            }
        }
        Err(Failure::VariableNotInScope(ident.to_string()))
    }

    /// Introduce a new variable into the current inner most scope.
    fn introduce_variable(&mut self, ident: &str, var_type: super::Type) {
        self.get_inner_scope().variable_defs.push(VariableDef {
            identifier: ident.to_string(),
            var_type
        })
    }

    fn function_lookup(&self, ident: &str, params: &[super::Type]) -> Result<&FunctionDef, Failure> {
        for scope in self.scope_stack.iter().rev() {
            if let Some(func_def) = scope.find_function_def(ident, params) {
                return Ok(func_def)
            }
        }
        Err(Failure::FunctionNotInScope(ident.to_string(), params.to_vec()))
    }

    fn introduce_function(&mut self, ident: &str, params: &[super::Type], return_type: Option<super::Type>) {
        self.get_inner_scope().function_defs.push(FunctionDef {
            identifier: ident.to_string(),
            parameter_types: params.to_vec(),
            return_type
        })
    }

    fn check_expr(&self, expr: &parsing::Expression) -> Result<super::Type, Failure> {
        match expr {
            parsing::Expression::Variable { pos: _, identifier } => {
                log::trace!("Searching scope for the type of referenced variable with identifier '{}'", identifier);

                let definition = self.variable_lookup(identifier)?;
                Ok(definition.var_type.clone())
            }

            parsing::Expression::FunctionCall {pos: _, identifier, args } => {
                log::trace!("Searching scope for the return type of referenced function '{}' given arguments {:?}", identifier, args);

                let mut arg_types = Vec::new();
                for arg in args { arg_types.push(self.check_expr(arg)?) }

                let definition = self.function_lookup(identifier, arg_types.as_slice())?;
                
                match &definition.return_type {
                    Some(return_type) => Ok(return_type.clone()),
                    None => Err(Failure::VoidFunctionInExpr(identifier.to_string(), arg_types))
                }
            }

            parsing::Expression::Add(left, right) |
            parsing::Expression::Subtract(left, right) |
            parsing::Expression::Multiply(left, right) |
            parsing::Expression::Divide(left, right) => {
                log::trace!("Verifying types of arithmetic expression (addition, division, etc.) - Num type on both sides of operator expected");

                self.expect_expr_type(left, super::Type::Num)?;
                self.expect_expr_type(right, super::Type::Num)?;

                Ok(super::Type::Num)
            }

            parsing::Expression::GreaterThan(left, right) |
            parsing::Expression::LessThan(left, right) => {
                log::trace!("Verifying type of arithmetic comparison expression (greater than, less than) - Num type type on both sides expected");

                self.expect_expr_type(left, super::Type::Num)?;
                self.expect_expr_type(right, super::Type::Num)?;

                Ok(super::Type::Bool)
            }

            parsing::Expression::Equal(left, right) => {
                log::trace!("Verifying types of equality expression - types on both sides of the operator should be the same");

                let left_type = self.check_expr(left)?;
                let right_type = self.check_expr(right)?;

                if left_type == right_type {
                    Ok(super::Type::Bool)
                }
                else {
                    Err(Failure::UnexpectedType {
                        expected: left_type,
                        encountered: right_type
                    })
                }
            }

            parsing::Expression::BooleanNot(expr) => {
                log::trace!("Verifying type of expression to which boolean NOT operator is being applied - expecting Bool expression to right of operator");

                self.expect_expr_type(expr, super::Type::Bool)?;
                Ok(super::Type::Bool)
            }

            parsing::Expression::UnaryMinus(expr) => {
                self.expect_expr_type(expr, super::Type::Num)?;
                Ok(super::Type::Num)
            }

            parsing::Expression::Array(_) => unimplemented!(),
            parsing::Expression::StringLiteral { pos: _, value: _ } => unimplemented!(),

            parsing::Expression::NumberLiteral {pos: _, value: _ } => Ok(super::Type::Num),
            parsing::Expression::BooleanLiteral { pos: _, value: _ } => Ok(super::Type::Bool),
            parsing::Expression::CharLiteral { pos: _, value: _ } => Ok(super::Type::Char)
        }
    }

    fn expect_expr_type(&self, expr: &parsing::Expression, expected: super::Type) -> Result<(), Failure> {
        let expr_type = self.check_expr(expr)?;
        
        if expr_type == expected { Ok(()) }
        else { Err(Failure::UnexpectedType { expected, encountered: expr_type }) }
    }
}

struct Scope {
    variable_defs: Vec<VariableDef>,
    function_defs: Vec<FunctionDef>
}

impl Scope {
    fn find_variable_def(&self, ident: &str) -> Option<&VariableDef> {
        for def in &self.variable_defs {
            if def.identifier == ident { return Some(def) }
        }
        None
    }

    fn find_function_def(&self, ident: &str, params: &[super::Type]) -> Option<&FunctionDef> {
        for def in &self.function_defs {
            if def.identifier == ident && def.parameter_types.as_slice() == params {
                return Some(def)
            }
        }
        None
    }
}

#[derive(Debug, PartialEq)]
struct VariableDef {
    identifier: String,
    var_type: super::Type
}

#[derive(Debug, PartialEq)]
struct FunctionDef {
    identifier: String,
    parameter_types: Vec<super::Type>,
    return_type: Option<super::Type>
}

#[cfg(test)]
mod tests {
    use std::iter;
    use crate::{ parsing, checking, stream::Position };

    #[test]
    fn scoping() {
        let mut chkr = super::Checker::new(iter::empty());

        chkr.begin_new_scope();

        chkr.introduce_variable("outer", checking::Type::Num);
        assert_eq!(chkr.variable_lookup("outer"), Ok(&super::VariableDef {
            identifier: "outer".to_string(),
            var_type: checking::Type::Num
        }));

        chkr.begin_new_scope();

        chkr.introduce_variable("inner", checking::Type::Bool);

        assert!(chkr.variable_lookup("inner").is_ok());
        assert!(chkr.variable_lookup("outer").is_ok());

        chkr.end_scope();

        assert!(chkr.variable_lookup("inner").is_err());
        assert!(chkr.variable_lookup("outer").is_ok());
        assert!(chkr.variable_lookup("undefined").is_err());

        chkr.introduce_function("xyz", &[checking::Type::Char], Some(checking::Type::Num));
        
        assert_eq!(chkr.function_lookup("xyz", &[checking::Type::Char]), Ok(&super::FunctionDef {
            identifier: "xyz".to_string(),
            parameter_types: vec![checking::Type::Char],
            return_type: Some(checking::Type::Num)
        }));

        assert!(chkr.function_lookup("xyz", &[checking::Type::Num]).is_err());

        chkr.end_scope();
    }

    #[test]
    fn check_exprs() {
        let mut chkr = super::Checker::new(iter::empty());

        assert_eq!(
            chkr.check_expr(&parsing::Expression::NumberLiteral { pos: Position::new(), value: 10.5 }),
            Ok(checking::Type::Num)
        );

        assert_eq!(
            chkr.check_expr(&parsing::Expression::BooleanLiteral { pos: Position::new(), value: true }),
            Ok(checking::Type::Bool)
        );

        assert_eq!(
            chkr.check_expr(&parsing::Expression::CharLiteral { pos: Position::new(), value: '話' }),
            Ok(checking::Type::Char)
        );

        assert_eq!(
            chkr.check_expr(&parsing::Expression::Equal(
                Box::new(parsing::Expression::CharLiteral { pos: Position::new(), value: 'x' }),
                Box::new(parsing::Expression::CharLiteral { pos: Position::new(), value: 'y' })
            )),
            Ok(checking::Type::Bool)
        );

        assert_eq!(
            chkr.check_expr(&parsing::Expression::Equal(
                Box::new(parsing::Expression::NumberLiteral { pos: Position::new(), value: 1.5 }),
                Box::new(parsing::Expression::BooleanLiteral { pos: Position::new(), value: false })
            )),
            Err(super::Failure::UnexpectedType {
                encountered: checking::Type::Bool,
                expected: checking::Type::Num
            })
        );

        assert_eq!(
            chkr.check_expr(&parsing::Expression::Add(
                Box::new(parsing::Expression::NumberLiteral { pos: Position::new(), value: 10.0 }),
                Box::new(parsing::Expression::NumberLiteral { pos: Position::new(), value: 11.2 })
            )),
            Ok(checking::Type::Num)
        );

        assert_eq!(
            chkr.check_expr(&parsing::Expression::Divide(
                Box::new(parsing::Expression::CharLiteral { pos: Position::new(), value: 'x' }),
                Box::new(parsing::Expression::BooleanLiteral { pos: Position::new(), value: false })
            )),
            Err(super::Failure::UnexpectedType {
                encountered: checking::Type::Char,
                expected: checking::Type::Num
            })
        );
    }

    #[test]
    fn check_stmts() {
        // TODO: ...
    }
}