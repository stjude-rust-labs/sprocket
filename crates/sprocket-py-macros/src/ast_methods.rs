//! TODO

use proc_macro2::TokenStream;
use quote::quote;
use syn::Error;
use syn::Generics;
use syn::Ident;
use syn::ItemImpl;
use syn::Result;
use syn::parse::Nothing;
use syn::parse_quote;

/// Represents whether an AST element is a node or token.
#[derive(PartialEq, Debug)]
enum AstKind {
    /// An AST node.
    Node { generic_ident: Ident },
    /// An AST token.
    Token { generic_ident: Ident },
}

pub(crate) fn ast_methods(
    args_stream: TokenStream,
    impl_stream: TokenStream,
) -> Result<TokenStream> {
    syn::parse2::<Nothing>(args_stream)?;
    let original = syn::parse2::<ItemImpl>(impl_stream)?;

    let mut py_impl = original.clone();

    // Annotate the Python `impl` with `#[pymethods]`.
    py_impl.attrs.push(parse_quote!(#[::pyo3::pymethods]));

    // Determine if this AST element is a node or a token, and get the type
    // parameter's ident.
    let ast_kind = ast_kind(&original.generics)?;

    // Remove the first generic (`impl<N: TreeNode> Ast<N>` into `impl Ast<N>`).
    py_impl.generics = Generics::default();

    Ok(quote! {
        #original
        #py_impl
    })
}

/// Determines whether the AST element is a node or a token from its `impl`
/// generics, and gets the [`Ident`] of that generic type.
fn ast_kind(generics: &Generics) -> Result<Option<AstKind>> {
    if let Some(ref where_clause) = generics.where_clause {
        return Err(Error::new_spanned(
            where_clause,
            "`#[ast_methods]` does not support `where` clauses",
        ));
    }

    let mut type_params = generics.type_params();

    let Some(type_param) = type_params.next() else {
        // No type parameters.
        return Ok(None);
    };

    if let Some(extra_type_param) = type_params.next() {
        return Err(Error::new_spanned(
            extra_type_param,
            "`#[ast_methods]` only supports a single type parameter",
        ));
    }

    if type_param.bounds == parse_quote!(TreeNode) {
        Ok(Some(AstKind::Node {
            generic_ident: type_param.ident.clone(),
        }))
    } else if type_param.bounds == parse_quote!(TreeToken) {
        Ok(Some(AstKind::Token {
            generic_ident: type_param.ident.clone(),
        }))
    } else {
        Err(Error::new_spanned(
            &type_param.bounds,
            "`#[ast_methods]` requires that trait bounds be either `TreeNode` or `TreeToken`",
        ))
    }
}

#[cfg(test)]
mod tests {
    use proc_macro2::Span;

    use super::*;

    #[test]
    fn ast_kind_node() {
        let original: ItemImpl = parse_quote! { impl<N: TreeNode> Ast<N> {} };

        let ast_kind = ast_kind(&original.generics).unwrap().unwrap();

        assert_eq!(
            ast_kind,
            AstKind::Node {
                generic_ident: Ident::new("N", Span::call_site())
            }
        );
    }

    #[test]
    fn ast_kind_token() {
        let original: ItemImpl = parse_quote! { impl<T: TreeToken> Ast<T> {} };

        let ast_kind = ast_kind(&original.generics).unwrap().unwrap();

        assert_eq!(
            ast_kind,
            AstKind::Token {
                generic_ident: Ident::new("T", Span::call_site())
            }
        );
    }

    #[test]
    fn ast_kind_no_params() {
        let original: ItemImpl = parse_quote! { impl Ast {} };

        let ast_kind = ast_kind(&original.generics).unwrap();

        assert!(
            ast_kind.is_none(),
            "did not return `None` for zero type parameters: {ast_kind:?}"
        );
    }

    #[test]
    fn ast_kind_two_params() {
        let original: ItemImpl = parse_quote! { impl<N: TreeNode, T: TreeToken> Ast<N> {} };

        let result = ast_kind(&original.generics);

        assert!(
            result.is_err(),
            "did not error on multiple type parameters: {result:?}"
        );
    }

    #[test]
    fn ast_kind_where_clause() {
        let original: ItemImpl = parse_quote! { impl<N> Ast<N> where N: TreeNode {} };

        let result = ast_kind(&original.generics);

        assert!(result.is_err(), "did not error on where clause: {result:?}");
    }

    #[test]
    fn ast_kind_invalid_trait_bound() {
        let original: ItemImpl = parse_quote! { impl<T: Display> Ast<T> {} };

        let result = ast_kind(&original.generics);

        assert!(
            result.is_err(),
            "did not error on invalid trait bound: {result:?}"
        );
    }
}
