//! This module implements the `GcObject` structure.
//!
//! The `GcObject` is a garbage collected Object.

use super::Object;
use crate::{
    builtins::{
        function::{create_unmapped_arguments_object, FunctionBody, ThisMode},
        ResultValue, Value,
    },
    environment::{
        function_environment_record::BindingStatus, lexical_environment::new_function_environment,
    },
    Executable, Interpreter,
};
use gc::{Finalize, Gc, GcCell, GcCellRef, GcCellRefMut, Trace};
use std::fmt::{self, Display};

/// Garbage collected `Object`.
#[derive(Debug, Trace, Finalize, Clone)]
pub struct GcObject(Gc<GcCell<Object>>);

impl GcObject {
    #[inline]
    pub(crate) fn new(object: Object) -> Self {
        Self(Gc::new(GcCell::new(object)))
    }

    #[inline]
    pub fn borrow(&self) -> GcCellRef<'_, Object> {
        self.try_borrow().expect("Object already mutably borrowed")
    }

    #[inline]
    pub fn borrow_mut(&self) -> GcCellRefMut<'_, Object> {
        self.try_borrow_mut().expect("Object already borrowed")
    }

    #[inline]
    pub fn try_borrow(&self) -> Result<GcCellRef<'_, Object>, BorrowError> {
        self.0.try_borrow().map_err(|_| BorrowError)
    }

    #[inline]
    pub fn try_borrow_mut(&self) -> Result<GcCellRefMut<'_, Object>, BorrowMutError> {
        self.0.try_borrow_mut().map_err(|_| BorrowMutError)
    }

    /// Checks if the garbage collected memory is the same.
    #[inline]
    pub fn equals(lhs: &Self, rhs: &Self) -> bool {
        std::ptr::eq(lhs.as_ref(), rhs.as_ref())
    }

    /// This will handle calls for both ordinary and built-in functions
    ///
    /// <https://tc39.es/ecma262/#sec-prepareforordinarycall>
    /// <https://tc39.es/ecma262/#sec-ecmascript-function-objects-call-thisargument-argumentslist>
    pub fn call(&self, this: &Value, args: &[Value], ctx: &mut Interpreter) -> ResultValue {
        let this_function_object = self.clone();
        let object = self.borrow();
        if let Some(function) = object.as_function() {
            if function.is_callable() {
                match function.body {
                    FunctionBody::BuiltIn(func) => func(this, args, ctx),
                    FunctionBody::Ordinary(ref body) => {
                        // Create a new Function environment who's parent is set to the scope of the function declaration (self.environment)
                        // <https://tc39.es/ecma262/#sec-prepareforordinarycall>
                        let local_env = new_function_environment(
                            this_function_object.into(),
                            if let ThisMode::Lexical = function.this_mode {
                                None
                            } else {
                                Some(this.clone())
                            },
                            function.environment.clone(),
                            // Arrow functions do not have a this binding https://tc39.es/ecma262/#sec-function-environment-records
                            if let ThisMode::Lexical = function.this_mode {
                                BindingStatus::Lexical
                            } else {
                                BindingStatus::Uninitialized
                            },
                        );

                        // Add argument bindings to the function environment
                        for (i, param) in function.params.iter().enumerate() {
                            // Rest Parameters
                            if param.is_rest_param() {
                                function.add_rest_param(param, i, args, ctx, &local_env);
                                break;
                            }

                            let value = args.get(i).cloned().unwrap_or_else(Value::undefined);
                            function.add_arguments_to_environment(param, value, &local_env);
                        }

                        // Add arguments object
                        let arguments_obj = create_unmapped_arguments_object(args);
                        local_env
                            .borrow_mut()
                            .create_mutable_binding("arguments".to_string(), false);
                        local_env
                            .borrow_mut()
                            .initialize_binding("arguments", arguments_obj);

                        ctx.realm.environment.push(local_env);

                        // Call body should be set before reaching here
                        let result = body.run(ctx);

                        // local_env gets dropped here, its no longer needed
                        ctx.realm.environment.pop();
                        result
                    }
                }
            } else {
                ctx.throw_type_error("function object is not callable")
            }
        } else {
            ctx.throw_type_error("not a function")
        }
    }
}

impl AsRef<GcCell<Object>> for GcObject {
    #[inline]
    fn as_ref(&self) -> &GcCell<Object> {
        &*self.0
    }
}

/// An error returned by [`GcObject::try_borrow`](struct.GcObject.html#method.try_borrow).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BorrowError;

impl Display for BorrowError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt("Object already mutably borrowed", f)
    }
}

/// An error returned by [`GcObject::try_borrow_mut`](struct.GcObject.html#method.try_borrow_mut).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BorrowMutError;

impl Display for BorrowMutError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt("Object already borrowed", f)
    }
}
