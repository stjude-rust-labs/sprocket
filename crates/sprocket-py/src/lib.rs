//! This crate is a Python extension that exposes a subset of `wdl`'s API
//! using [`pyo3`].
//!
//! This crate is not meant to be imported directly. Instead, import the
//! `sprocket_bio` Python package (located at `python/sprocket_bio`), which
//! bundles this extension.

use pyo3::prelude::*;

/// Python bindings to [Sprocket](https://sprocket.bio), a bioinformatics toolkit for Workflow
/// Description Language (WDL).
#[pymodule]
mod _sprocket_bio {
    use pyo3::prelude::*;
    use pyo3::types::PyString;

    #[pymodule]
    mod diagnostics {
        #[pymodule_export]
        use wdl_diagnostics::Mode;
        #[pymodule_export]
        use wdl_diagnostics::py_emit_diagnostics;
    }

    #[pymodule]
    mod grammar {
        use pyo3::prelude::*;
        #[pymodule_export]
        use wdl_grammar::Diagnostic;
        #[pymodule_export]
        use wdl_grammar::Label;
        #[pymodule_export]
        use wdl_grammar::Severity;
        #[pymodule_export]
        use wdl_grammar::Span;
        #[pymodule_export]
        use wdl_grammar::SyntaxKind;
        // Re-exported.
        #[pymodule_export]
        use wdl_grammar::version::SupportedVersion;

        #[pymodule]
        mod grammar {
            #[pymodule_export]
            use wdl_grammar::grammar::py_document;
        }

        #[pymodule]
        mod parser {
            #[pymodule_export]
            use wdl_grammar::parser::PyEvent;
        }

        #[pymodule]
        mod version {
            #[pymodule_export]
            use wdl_grammar::version::SupportedVersion;
            #[pymodule_export]
            use wdl_grammar::version::V1;
        }
    }

    #[pymodule]
    mod ast {
        use pyo3::prelude::*;
        #[pymodule_export]
        use wdl_ast::Directive;
        #[pymodule_export]
        use wdl_ast::ExceptRule;
        #[pymodule_export]
        use wdl_ast::PyAst;
        #[pymodule_export]
        use wdl_ast::PyAstNode;
        #[pymodule_export]
        use wdl_ast::PyAstToken;
        #[pymodule_export]
        use wdl_ast::PyComment;
        #[pymodule_export]
        use wdl_ast::PyCommentKind;
        #[pymodule_export]
        use wdl_ast::PyDirectiveKind;
        #[pymodule_export]
        use wdl_ast::PyDocument;
        #[pymodule_export]
        use wdl_ast::PyIdent;
        #[pymodule_export]
        use wdl_ast::PyTokenText;
        #[pymodule_export]
        use wdl_ast::PyVersion;
        #[pymodule_export]
        use wdl_ast::PyVersionStatement;
        #[pymodule_export]
        use wdl_ast::PyWhitespace;

        #[pymodule]
        mod v1 {
            #[pymodule_export]
            use wdl_ast::v1::PyAccessExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyAdditionExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyAfterKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyAliasKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyArrayType;
            #[pymodule_export]
            use wdl_ast::v1::PyArrayTypeKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyAsKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyAssignment;
            #[pymodule_export]
            use wdl_ast::v1::PyAst;
            #[pymodule_export]
            use wdl_ast::v1::PyAsterisk;
            #[pymodule_export]
            use wdl_ast::v1::PyBooleanTypeKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyBoundDecl;
            #[pymodule_export]
            use wdl_ast::v1::PyCallAfter;
            #[pymodule_export]
            use wdl_ast::v1::PyCallAlias;
            #[pymodule_export]
            use wdl_ast::v1::PyCallExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyCallInputItem;
            #[pymodule_export]
            use wdl_ast::v1::PyCallKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyCallStatement;
            #[pymodule_export]
            use wdl_ast::v1::PyCallTarget;
            #[pymodule_export]
            use wdl_ast::v1::PyCloseBrace;
            #[pymodule_export]
            use wdl_ast::v1::PyCloseBracket;
            #[pymodule_export]
            use wdl_ast::v1::PyCloseHeredoc;
            #[pymodule_export]
            use wdl_ast::v1::PyCloseParen;
            #[pymodule_export]
            use wdl_ast::v1::PyColon;
            #[pymodule_export]
            use wdl_ast::v1::PyComma;
            #[pymodule_export]
            use wdl_ast::v1::PyCommandKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyCommandPart;
            #[pymodule_export]
            use wdl_ast::v1::PyCommandSection;
            #[pymodule_export]
            use wdl_ast::v1::PyCommandText;
            #[pymodule_export]
            use wdl_ast::v1::PyConditionalStatement;
            #[pymodule_export]
            use wdl_ast::v1::PyConditionalStatementClause;
            #[pymodule_export]
            use wdl_ast::v1::PyConditionalStatementClauseKind;
            #[pymodule_export]
            use wdl_ast::v1::PyDecl;
            #[pymodule_export]
            use wdl_ast::v1::PyDefaultOption;
            #[pymodule_export]
            use wdl_ast::v1::PyDirectoryTypeKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyDivisionExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyDocumentItem;
            #[pymodule_export]
            use wdl_ast::v1::PyDot;
            #[pymodule_export]
            use wdl_ast::v1::PyDoubleQuote;
            #[pymodule_export]
            use wdl_ast::v1::PyElseKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyEnumChoice;
            #[pymodule_export]
            use wdl_ast::v1::PyEnumDefinition;
            #[pymodule_export]
            use wdl_ast::v1::PyEnumKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyEnumTypeParameter;
            #[pymodule_export]
            use wdl_ast::v1::PyEnvKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyEqual;
            #[pymodule_export]
            use wdl_ast::v1::PyEqualityExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyExclamation;
            #[pymodule_export]
            use wdl_ast::v1::PyExponentiation;
            #[pymodule_export]
            use wdl_ast::v1::PyExponentiationExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyFalseKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyFileTypeKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyFloat;
            #[pymodule_export]
            use wdl_ast::v1::PyFloatTypeKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyFromKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyGreater;
            #[pymodule_export]
            use wdl_ast::v1::PyGreaterEqual;
            #[pymodule_export]
            use wdl_ast::v1::PyGreaterEqualExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyGreaterExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyHintsKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyIfExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyIfKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyImportAlias;
            #[pymodule_export]
            use wdl_ast::v1::PyImportForm;
            #[pymodule_export]
            use wdl_ast::v1::PyImportKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyImportMember;
            #[pymodule_export]
            use wdl_ast::v1::PyImportMembers;
            #[pymodule_export]
            use wdl_ast::v1::PyImportSource;
            #[pymodule_export]
            use wdl_ast::v1::PyImportStatement;
            #[pymodule_export]
            use wdl_ast::v1::PyInKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyIndexExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyInequalityExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyInputKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyInputSection;
            #[pymodule_export]
            use wdl_ast::v1::PyIntTypeKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyInteger;
            #[pymodule_export]
            use wdl_ast::v1::PyLess;
            #[pymodule_export]
            use wdl_ast::v1::PyLessEqual;
            #[pymodule_export]
            use wdl_ast::v1::PyLessEqualExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyLessExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralArray;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralBoolean;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralFloat;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralHints;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralHintsItem;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralInput;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralInputItem;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralInteger;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralMap;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralMapItem;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralNone;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralNull;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralObject;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralObjectItem;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralOutput;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralOutputItem;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralPair;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralString;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralStringKind;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralStringText;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralStruct;
            #[pymodule_export]
            use wdl_ast::v1::PyLiteralStructItem;
            #[pymodule_export]
            use wdl_ast::v1::PyLogicalAnd;
            #[pymodule_export]
            use wdl_ast::v1::PyLogicalAndExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyLogicalNotExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyLogicalOr;
            #[pymodule_export]
            use wdl_ast::v1::PyLogicalOrExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyMapType;
            #[pymodule_export]
            use wdl_ast::v1::PyMapTypeKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyMetaKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyMetadataArray;
            #[pymodule_export]
            use wdl_ast::v1::PyMetadataObject;
            #[pymodule_export]
            use wdl_ast::v1::PyMetadataObjectItem;
            #[pymodule_export]
            use wdl_ast::v1::PyMetadataSection;
            #[pymodule_export]
            use wdl_ast::v1::PyMetadataValue;
            #[pymodule_export]
            use wdl_ast::v1::PyMinus;
            #[pymodule_export]
            use wdl_ast::v1::PyModuloExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyMultiplicationExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyNameRefExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyNegationExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyNoneKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyNotEqual;
            #[pymodule_export]
            use wdl_ast::v1::PyNullKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyObjectKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyObjectType;
            #[pymodule_export]
            use wdl_ast::v1::PyObjectTypeKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyOpenBrace;
            #[pymodule_export]
            use wdl_ast::v1::PyOpenBracket;
            #[pymodule_export]
            use wdl_ast::v1::PyOpenHeredoc;
            #[pymodule_export]
            use wdl_ast::v1::PyOpenParen;
            #[pymodule_export]
            use wdl_ast::v1::PyOutputKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyOutputSection;
            #[pymodule_export]
            use wdl_ast::v1::PyPairType;
            #[pymodule_export]
            use wdl_ast::v1::PyPairTypeKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyParameterMetaKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyParameterMetadataSection;
            #[pymodule_export]
            use wdl_ast::v1::PyParenthesizedExpr;
            #[pymodule_export]
            use wdl_ast::v1::PyPercent;
            #[pymodule_export]
            use wdl_ast::v1::PyPlaceholder;
            #[pymodule_export]
            use wdl_ast::v1::PyPlaceholderOpen;
            #[pymodule_export]
            use wdl_ast::v1::PyPlaceholderOption;
            #[pymodule_export]
            use wdl_ast::v1::PyPlus;
            #[pymodule_export]
            use wdl_ast::v1::PyPrimitiveType;
            #[pymodule_export]
            use wdl_ast::v1::PyPrimitiveTypeKind;
            #[pymodule_export]
            use wdl_ast::v1::PyQuestionMark;
            #[pymodule_export]
            use wdl_ast::v1::PyRequirementsItem;
            #[pymodule_export]
            use wdl_ast::v1::PyRequirementsKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyRequirementsSection;
            #[pymodule_export]
            use wdl_ast::v1::PyRuntimeItem;
            #[pymodule_export]
            use wdl_ast::v1::PyRuntimeKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyRuntimeSection;
            #[pymodule_export]
            use wdl_ast::v1::PyScatterKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyScatterStatement;
            #[pymodule_export]
            use wdl_ast::v1::PySectionParent;
            #[pymodule_export]
            use wdl_ast::v1::PySepOption;
            #[pymodule_export]
            use wdl_ast::v1::PySingleQuote;
            #[pymodule_export]
            use wdl_ast::v1::PySlash;
            #[pymodule_export]
            use wdl_ast::v1::PyStringPart;
            #[pymodule_export]
            use wdl_ast::v1::PyStringText;
            #[pymodule_export]
            use wdl_ast::v1::PyStringTypeKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyStructDefinition;
            #[pymodule_export]
            use wdl_ast::v1::PyStructItem;
            #[pymodule_export]
            use wdl_ast::v1::PyStructKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PySubtractionExpr;
            #[pymodule_export]
            use wdl_ast::v1::PySymbolicModulePath;
            #[pymodule_export]
            use wdl_ast::v1::PyTaskDefinition;
            #[pymodule_export]
            use wdl_ast::v1::PyTaskHintsItem;
            #[pymodule_export]
            use wdl_ast::v1::PyTaskHintsSection;
            #[pymodule_export]
            use wdl_ast::v1::PyTaskItem;
            #[pymodule_export]
            use wdl_ast::v1::PyTaskKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyThenKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyTrueFalseOption;
            #[pymodule_export]
            use wdl_ast::v1::PyTrueKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyType;
            #[pymodule_export]
            use wdl_ast::v1::PyTypeRef;
            #[pymodule_export]
            use wdl_ast::v1::PyUnboundDecl;
            #[pymodule_export]
            use wdl_ast::v1::PyUnknown;
            #[pymodule_export]
            use wdl_ast::v1::PyVersionKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyWorkflowDefinition;
            #[pymodule_export]
            use wdl_ast::v1::PyWorkflowHintsArray;
            #[pymodule_export]
            use wdl_ast::v1::PyWorkflowHintsItem;
            #[pymodule_export]
            use wdl_ast::v1::PyWorkflowHintsItemValue;
            #[pymodule_export]
            use wdl_ast::v1::PyWorkflowHintsObject;
            #[pymodule_export]
            use wdl_ast::v1::PyWorkflowHintsObjectItem;
            #[pymodule_export]
            use wdl_ast::v1::PyWorkflowHintsSection;
            #[pymodule_export]
            use wdl_ast::v1::PyWorkflowItem;
            #[pymodule_export]
            use wdl_ast::v1::PyWorkflowKeyword;
            #[pymodule_export]
            use wdl_ast::v1::PyWorkflowStatement;
        }
    }

    /// Initializer that runs when the `_sprocket_bio` Python extension is
    /// imported for the first time. As `sprocket_bio/__init__.py` imports
    /// this Python extension, this initializer is implicitly run the first
    /// time any `sprocket_bio` module is imported.
    ///
    /// This initializer is used to support importing items from submodules
    /// directly. For example, running `from sprocket_bio.diagnostics import
    /// Mode` will make the Python interpreter look for
    /// `sprocket_bio/diagnostics.py` or `sprocket_bio/diagnostics/__init__.py`.
    /// Neither of these files exist, however, and importing will result in
    /// a `ModuleNotFoundError`. To fix this, we patch [`sys.modules`](https://docs.python.org/3.10/library/sys.html#sys.modules)
    /// in this initializer so that the Python interpreter can import these
    /// submodules even though they aren't represented on the file system. For
    /// more information, see [pyo3#759](https://github.com/PyO3/pyo3/issues/759).
    #[pymodule_init]
    fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
        /// Recursively visits every submodule in this Python extension and adds
        /// it to `sys.modules`.
        fn register_submodules(
            module: &Bound<'_, PyModule>,
            parent_name: &str,
            sys_modules: &Bound<'_, PyAny>,
        ) -> PyResult<()> {
            // Loop through the names of all items in the module.
            for item_name in module.index()? {
                // Cast name from `PyAny` to `PyString`.
                let item_name: &Bound<'_, PyString> = item_name.cast()?;
                // Get the actual item from its name.
                let item = module.getattr(item_name)?;

                // If the item is a submodule...
                if let Ok(submodule) = item.cast::<PyModule>() {
                    let submodule_name = format!("{parent_name}.{item_name}");

                    // ...add the submodule to `sys.modules`.
                    sys_modules.set_item(&submodule_name, submodule)?;

                    register_submodules(submodule, &submodule_name, sys_modules)?;
                }
            }

            Ok(())
        }

        register_submodules(
            module,
            "sprocket_bio",
            // Get the `sys.modules` dictionary.
            &module.py().import("sys")?.getattr("modules")?,
        )
    }
}
