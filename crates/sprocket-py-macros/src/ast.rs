//! TODO

use proc_macro2::Ident;
use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::format_ident;
use quote::quote;
use syn::Error;
use syn::Fields;
use syn::Generics;
use syn::ItemStruct;
use syn::LitStr;
use syn::Result;
use syn::parse::Parser;
use syn::parse_quote;

/// Arguments to the `#[ast]` attribute.
#[derive(PartialEq, Debug)]
pub(crate) struct Args {
    /// The module the type is defined in, from Python's perspective.
    ///
    /// This is forwarded to `#[pyclass(module = ...)]`.
    pub(crate) module: LitStr,
}

impl Args {
    pub(crate) fn parse(args_stream: TokenStream) -> Result<Self> {
        let mut args = Self::default();

        let args_parser = syn::meta::parser(|meta| {
            if meta.path.is_ident("module") {
                args.module = meta.value()?.parse()?;
                return Ok(());
            }

            Err(meta.error("unknown `#[ast]` argument"))
        });

        args_parser.parse2(args_stream).map(move |_| args)
    }
}

impl Default for Args {
    fn default() -> Self {
        Self {
            module: LitStr::new("sprocket_bio.ast.v1", Span::call_site()),
        }
    }
}

/// Represents whether an AST element is a node or token.
#[derive(Clone, Copy, PartialEq, Debug)]
enum AstKind {
    Node,
    Token,
}

/// Gives the Python binding equivalent of an AST struct.
pub(crate) fn build(original: &ItemStruct, args: Args) -> Result<TokenStream> {
    let mut py_struct = original.clone();

    build_ident(&mut py_struct, original);

    let ast_kind = ast_kind(original)?;

    // Remove generics.
    py_struct.generics = Generics::default();

    // Only copy over doc comments, remove all other attributes.
    py_struct.attrs.retain(|attr| attr.path().is_ident("doc"));

    py_struct.attrs.extend_from_slice(&[
        // Make generated struct a pyclass.
        {
            let module = args.module;
            let class_name = LitStr::new(&original.ident.to_string(), original.ident.span());
            let extends = Ident::new(
                match ast_kind {
                    AstKind::Node => "PyAstNode",
                    AstKind::Token => "PyAstToken",
                },
                Span::call_site(),
            );

            parse_quote!(#[::pyo3::pyclass(module = #module, name = #class_name, extends = crate::#extends, frozen, skip_from_py_object, eq)])
        },
        // `#[pymethods]` relies on the Python struct being cloneable. We additionally derive
        // `PartialEq` for parity with the original struct.
        parse_quote!(#[derive(Clone, PartialEq)]),
        // `Debug` is purposefully not implemented, silence the lint.
        parse_quote!(#[allow(missing_debug_implementations)]),
    ]);

    // Verify fields.
    if let Fields::Unnamed(ref fields) = original.fields {
        if fields.unnamed.len() != 1 {
            return Err(Error::new_spanned(
                fields,
                "`#[ast]` only supports structs with a single field",
            ));
        }
    } else {
        return Err(Error::new_spanned(
            &original.fields,
            "`#[ast]` only supports tuple structs",
        ));
    }

    // Replace generic node / token field with thread-safe variant.
    py_struct.fields = Fields::Unnamed(match ast_kind {
        AstKind::Node => parse_quote!((crate::python::ThreadSafeSyntaxNode)),
        AstKind::Token => parse_quote!((crate::python::ThreadSafeSyntaxToken)),
    });

    // Used for quote formatting.
    let ident = &original.ident;
    let py_ident = &py_struct.ident;
    let base_class = Ident::new(
        match ast_kind {
            AstKind::Node => "PyAstNode",
            AstKind::Token => "PyAstToken",
        },
        Span::call_site(),
    );

    Ok(quote! {
        #py_struct

        // Converting from the original struct to the Python struct. This is used in the original
        // struct's `IntoPyObject` impl.
        impl ::std::convert::From<#ident> for #py_ident {
            fn from(value: #ident) -> Self {
                Self(value.0.into())
            }
        }

        // Converting from the Python struct to the original struct. This is used by Python methods
        // to call their original counterparts.
        impl ::std::convert::From<#py_ident> for #ident {
            fn from(value: #py_ident) -> Self {
                Self(value.0.into())
            }
        }

        // Let the original struct be converted directly into a Python object. This lets the
        // original struct be returned directly from Python methods.
        impl<'py> ::pyo3::conversion::IntoPyObject<'py> for #ident {
            type Target = ::pyo3::types::PyAny;
            type Output = ::pyo3::Bound<'py, Self::Target>;
            type Error = ::pyo3::PyErr;

            fn into_pyobject(self, py: ::pyo3::marker::Python<'py>) -> Result<Self::Output, Self::Error> {
                use ::pyo3::prelude::*;

                // Convert `self` to its Python counterpart, make it a subclass of `PyAstNode` or
                // `PyAstToken`, allocate it on Python's heap, then cast it to `PyAny`.
                Bound::new(py, PyClassInitializer::from(crate::#base_class).add_subclass(#py_ident::from(self)))
                    .map(Bound::into_any)
            }
        }
    })
}

/// Modifies the [`Ident`] of `py_struct` to its Python name.
fn build_ident(py_struct: &mut ItemStruct, original: &ItemStruct) {
    py_struct.ident = format_ident!("Py{}", original.ident);
}

/// Determines if the AST element is a node or token by inspecting its generic
/// types.
fn ast_kind(original: &ItemStruct) -> Result<AstKind> {
    if original.generics == parse_quote!(<N: TreeNode = SyntaxNode>) {
        Ok(AstKind::Node)
    } else if original.generics == parse_quote!(<T: TreeToken = SyntaxToken>) {
        Ok(AstKind::Token)
    } else {
        return Err(Error::new_spanned(
            &original.generics,
            "`#[ast]` requires that struct generics be either `<N: TreeNode = SyntaxNode>` or \
             `<T: TreeToken = SyntaxToken>`",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_args() {
        let args_stream = quote!();
        let args = Args::parse(args_stream).unwrap();

        assert_eq!(args, Args::default());
    }

    #[test]
    fn module_arg() {
        let args_stream = quote!(module = "sprocket_bio.super_cool_module");
        let args = Args::parse(args_stream).unwrap();

        assert_eq!(args.module.value(), "sprocket_bio.super_cool_module");
    }

    #[test]
    fn unknown_arg() {
        let args_stream = quote!(spooky = "👻");
        let result = Args::parse(args_stream);

        assert!(
            result.is_err(),
            "did not error on unknown argument: {result:?}"
        );
    }

    #[test]
    fn ident() {
        let original: ItemStruct = parse_quote! { struct Foo; };
        let mut py_struct = original.clone();

        build_ident(&mut py_struct, &original);

        assert_eq!(py_struct.ident.to_string(), "PyFoo");
    }

    #[test]
    fn ast_kind_node() {
        let original: ItemStruct = parse_quote! { struct Foo<N: TreeNode = SyntaxNode>(N); };
        assert_eq!(ast_kind(&original).unwrap(), AstKind::Node);
    }

    #[test]
    fn ast_kind_token() {
        let original: ItemStruct = parse_quote! { struct Foo<T: TreeToken = SyntaxToken>(T); };
        assert_eq!(ast_kind(&original).unwrap(), AstKind::Token);
    }

    #[test]
    fn ast_kind_invalid_generics() {
        let missing_generics: ItemStruct = parse_quote! { struct Foo; };
        let result = ast_kind(&missing_generics);

        assert!(
            result.is_err(),
            "did not error on missing generics: {result:?}"
        );

        let different_name: ItemStruct =
            parse_quote! { struct Foo<BAD: TreeToken = SyntaxToken>(BAD); };
        let result = ast_kind(&different_name);

        assert!(
            result.is_err(),
            "did not error on incorrect generic name: {result:?}"
        );

        let different_trait: ItemStruct = parse_quote! { struct Foo<N: Display = String>(N); };
        let result = ast_kind(&different_trait);

        assert!(
            result.is_err(),
            "did not error on incorrect generic trait bound: {result:?}"
        );

        let missing_default: ItemStruct = parse_quote! { struct Foo<N: TreeNode>(N); };
        let result = ast_kind(&missing_default);

        assert!(
            result.is_err(),
            "did not error on missing generic default: {result:?}"
        );
    }
}
