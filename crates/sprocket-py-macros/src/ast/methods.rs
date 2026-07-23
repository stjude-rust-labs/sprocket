//! TODO

use proc_macro2::TokenStream;
use quote::ToTokens;
use quote::format_ident;
use syn::Error;
use syn::Generics;
use syn::ImplItem;
use syn::ItemImpl;
use syn::LitStr;
use syn::PathArguments;
use syn::Type;
use syn::parse_quote;

/// TODO
pub(crate) fn build(original: &mut ItemImpl) -> syn::Result<TokenStream> {
    let mut py_impl = original.clone();

    // Add `#[pymethods]` attribute to impl.
    py_impl.attrs.push(parse_quote!(#[::pyo3::pymethods]));

    // Remove first generic (`impl<N: TreeNode> Ast<N>` into `impl Ast<N>`)
    py_impl.generics = Generics::default();

    // Remove second generic and add "Py" prefix (`impl Ast<N>` into `impl PyAst`).
    if let Type::Path(ref mut type_path) = *py_impl.self_ty {
        let last_segment = type_path
            .path
            .segments
            .last_mut()
            .expect("type path should contain at least one segment");

        last_segment.ident = format_ident!("Py{}", last_segment.ident);
        last_segment.arguments = PathArguments::None;
    } else {
        return Err(Error::new_spanned(
            py_impl.self_ty,
            "type not supported by `#[ast_methods]`",
        ));
    }

    // Filter and process items.
    py_impl.items.retain_mut(|item| {
        // Only retain functions, we don't support anything else.
        let ImplItem::Fn(fn_) = item else {
            return false;
        };

        let mut is_py_method = false;

        // Search for `#[method]` or `#[staticmethod]` attribute.
        for i in 0..fn_.attrs.len() {
            let path = fn_.attrs[i].path();

            if path.is_ident("method") {
                // Remove `#[method]`, as it is not recognized by PyO3 unlike `#[staticmethod]`.
                fn_.attrs.remove(i);
                is_py_method = true;
                break;
            } else if path.is_ident("staticmethod") {
                // Retain `#[staticmethod]` so that PyO3 can see it.
                is_py_method = true;
                break;
            }
        }

        // If the method isn't annotated with `#[method]` or `#[staticmethod]`, do not retain it.
        if !is_py_method {
            return false;
        }

        // Add `#[pyo3(name = "foo")]`. This makes the method in Python have its original Rust
        // name, before we add the "py_" prefix.
        fn_.attrs.push({
            let name = LitStr::new(&fn_.sig.ident.to_string(), fn_.sig.ident.span());
            parse_quote!(#[pyo3(name = #name)])
        });

        // Prefix function name with "py_" (`fn foo(&self) -> Bar<N>` into `fn py_foo(&self) -> Bar<N>`).
        fn_.sig.ident = format_ident!("py_{}", fn_.sig.ident);

        // TODO
        fn_.block = parse_quote!({
            // Ast::from(self.clone()).foo()
            todo!();
        });

        true
    });

    // Remove `#[method]` and `#[staticmethod]` attributes from original impl.
    for item in original.items.iter_mut() {
        if let ImplItem::Fn(fn_) = item {
            fn_.attrs.retain(|attr| {
                !(attr.path().is_ident("method") || attr.path().is_ident("staticmethod"))
            });
        }
    }

    Ok(py_impl.to_token_stream())
}
