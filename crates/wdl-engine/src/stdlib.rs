//! Module for the WDL standard library implementation.

use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use wdl_analysis::stdlib::Binding;
use wdl_analysis::types::Type;
use wdl_analysis::types::TypeEq;
use wdl_analysis::types::Types;
use wdl_ast::Diagnostic;
use wdl_ast::Span;

use crate::Coercible;
use crate::EvaluationContext;
use crate::Value;

mod as_map;
mod as_pairs;
mod basename;
mod ceil;
mod chunk;
mod collect_by_key;
mod contains;
mod contains_key;
mod cross;
mod defined;
mod find;
mod flatten;
mod floor;
mod glob;
mod join_paths;
mod keys;
mod length;
mod matches;
mod max;
mod min;
mod prefix;
mod quote;
mod range;
mod read_boolean;
mod read_float;
mod read_int;
mod read_json;
mod read_lines;
mod read_map;
mod read_object;
mod read_objects;
mod read_string;
mod read_tsv;
mod round;
mod select_all;
mod select_first;
mod sep;
mod size;
mod squote;
mod stderr;
mod stdout;
mod sub;
mod suffix;
mod transpose;
mod unzip;
mod values;
mod write_json;
mod write_lines;
mod write_map;
mod write_object;
mod write_objects;
mod write_tsv;
mod zip;

/// Represents a function call argument.
pub struct CallArgument {
    /// The value of the argument.
    value: Value,
    /// The span of the expression of the argument.
    span: Span,
}

impl CallArgument {
    /// Constructs a new call argument given its value and span.
    pub const fn new(value: Value, span: Span) -> Self {
        Self { value, span }
    }

    /// Constructs a `None` call argument.
    pub const fn none() -> Self {
        Self {
            value: Value::None,
            span: Span::new(0, 0),
        }
    }
}

/// Represents function call context.
pub struct CallContext<'a> {
    /// The evaluation context for the call.
    context: &'a mut dyn EvaluationContext,
    /// The call site span.
    call_site: Span,
    /// The arguments to the call.
    arguments: &'a [CallArgument],
    /// The return type.
    return_type: Type,
}

impl<'a> CallContext<'a> {
    /// Constructs a new call context given the call arguments.
    pub fn new(
        context: &'a mut dyn EvaluationContext,
        call_site: Span,
        arguments: &'a [CallArgument],
        return_type: Type,
    ) -> Self {
        Self {
            context,
            call_site,
            arguments,
            return_type,
        }
    }

    /// Gets the types collection associated with the call.
    pub fn types(&self) -> &Types {
        self.context.types()
    }

    /// Gets the mutable types collection associated with the call.
    pub fn types_mut(&mut self) -> &mut Types {
        self.context.types_mut()
    }

    /// Gets the current working directory for the call.
    pub fn cwd(&self) -> &Path {
        self.context.cwd()
    }

    /// Gets the temp directory for the call.
    pub fn tmp(&self) -> &Path {
        self.context.tmp()
    }

    /// Gets the stdout value for the call.
    pub fn stdout(&self) -> Option<Value> {
        self.context.stdout()
    }

    /// Gets the stderr value for the call.
    pub fn stderr(&self) -> Option<Value> {
        self.context.stderr()
    }

    /// Coerces an argument to the given type.
    ///
    /// # Panics
    ///
    /// Panics if the given index is out of range or if the value fails to
    /// coerce to the given type.
    #[inline]
    fn coerce_argument(&self, index: usize, ty: impl Into<Type>) -> Value {
        self.arguments[index]
            .value
            .coerce(self.context.types(), ty.into())
            .expect("value should coerce")
    }

    /// Checks to see if the calculated return type equals the given type.
    ///
    /// This is only used in assertions made by the function implementations.
    #[cfg(debug_assertions)]
    fn return_type_eq(&self, ty: impl Into<Type>) -> bool {
        self.return_type.type_eq(self.context.types(), &ty.into())
    }
}

/// Represents a WDL function implementation callback.
type Callback = fn(context: CallContext<'_>) -> Result<Value, Diagnostic>;

/// Represents an implementation signature for a WDL standard library function.
#[derive(Debug, Clone, Copy)]
pub struct Signature {
    /// The display string of the signature.
    ///
    /// This is only used for unit tests.
    #[allow(unused)]
    display: &'static str,
    /// The implementation callback of the signature.
    callback: Callback,
}

impl Signature {
    /// Constructs a new signature given its display and callback.
    const fn new(display: &'static str, callback: Callback) -> Self {
        Self { display, callback }
    }
}

/// Represents a standard library function.
#[derive(Debug, Clone, Copy)]
pub struct Function {
    /// The signatures of the function.
    signatures: &'static [Signature],
}

impl Function {
    /// Constructs a new function given its signatures.
    const fn new(signatures: &'static [Signature]) -> Self {
        Self { signatures }
    }

    /// Calls the function given the binding and call context.
    #[inline]
    pub fn call(
        &self,
        binding: Binding<'_>,
        context: CallContext<'_>,
    ) -> Result<Value, Diagnostic> {
        (self.signatures[binding.index()].callback)(context)
    }
}

/// Represents the WDL standard library.
#[derive(Debug)]
pub struct StandardLibrary {
    /// The implementation functions for the standard library.
    functions: HashMap<&'static str, Function>,
}

impl StandardLibrary {
    /// Gets a function from the standard library.
    ///
    /// Returns `None` if the function isn't in the WDL standard library.
    #[inline]
    pub fn get(&self, name: &str) -> Option<Function> {
        self.functions.get(name).copied()
    }
}

/// Represents the mapping between function name and overload index to the
/// implementation callback.
pub static STDLIB: LazyLock<StandardLibrary> = LazyLock::new(|| {
    /// Helper macro for mapping a function name to its descriptor
    macro_rules! func {
        ($name:ident) => {
            (stringify!($name), $name::descriptor())
        };
    }

    StandardLibrary {
        functions: HashMap::from_iter([
            func!(floor),
            func!(ceil),
            func!(round),
            func!(min),
            func!(max),
            func!(find),
            func!(matches),
            func!(sub),
            func!(basename),
            func!(join_paths),
            func!(glob),
            func!(size),
            func!(stdout),
            func!(stderr),
            func!(read_string),
            func!(read_int),
            func!(read_float),
            func!(read_boolean),
            func!(read_lines),
            func!(write_lines),
            func!(read_tsv),
            func!(write_tsv),
            func!(read_map),
            func!(write_map),
            func!(read_json),
            func!(write_json),
            func!(read_object),
            func!(read_objects),
            func!(write_object),
            func!(write_objects),
            func!(prefix),
            func!(suffix),
            func!(quote),
            func!(squote),
            func!(sep),
            func!(range),
            func!(transpose),
            func!(cross),
            func!(zip),
            func!(unzip),
            func!(contains),
            func!(chunk),
            func!(flatten),
            func!(select_first),
            func!(select_all),
            func!(as_pairs),
            func!(as_map),
            func!(keys),
            func!(contains_key),
            func!(values),
            func!(collect_by_key),
            func!(defined),
            func!(length),
        ]),
    }
});

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_analysis::stdlib::STDLIB as ANALYSIS_STDLIB;
    use wdl_analysis::stdlib::TypeParameters;

    use super::*;

    /// A test to verify that the STDLIB function types from `wdl-analysis`
    /// aligns with the STDLIB implementation from `wdl-engine`.
    #[test]
    fn verify_stdlib() {
        for (name, func) in ANALYSIS_STDLIB.functions() {
            match STDLIB.functions.get(name) {
                Some(imp) => match func {
                    wdl_analysis::stdlib::Function::Monomorphic(f) => {
                        assert_eq!(
                            imp.signatures.len(),
                            1,
                            "signature count mismatch for function `{name}`"
                        );
                        assert_eq!(
                            f.signature()
                                .display(
                                    ANALYSIS_STDLIB.types(),
                                    &TypeParameters::new(f.signature().type_parameters())
                                )
                                .to_string(),
                            imp.signatures[0].display,
                            "signature mismatch for function `{name}`"
                        );
                    }
                    wdl_analysis::stdlib::Function::Polymorphic(f) => {
                        assert_eq!(
                            imp.signatures.len(),
                            f.signatures().len(),
                            "signature count mismatch for function `{name}`"
                        );
                        for (i, sig) in f.signatures().iter().enumerate() {
                            assert_eq!(
                                sig.display(
                                    ANALYSIS_STDLIB.types(),
                                    &TypeParameters::new(sig.type_parameters())
                                )
                                .to_string(),
                                imp.signatures[i].display,
                                "signature mismatch for function `{name}` (index {i})"
                            );
                        }
                    }
                },
                None => panic!(
                    "missing function `{name}` in the engine's standard library implementation"
                ),
            }
        }
    }
}
