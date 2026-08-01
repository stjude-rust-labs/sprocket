//! TODO

use proc_macro2::TokenStream;
use quote::format_ident;
use quote::quote;
use syn::Error;
use syn::FnArg;
use syn::Generics;
use syn::Ident;
use syn::ImplItem;
use syn::ImplItemFn;
use syn::ItemImpl;
use syn::PathArguments;
use syn::Receiver;
use syn::ReceiverKind;
use syn::Result;
use syn::ReturnType;
use syn::Type;
use syn::TypeArray;
use syn::TypeGroup;
use syn::TypeParamBound;
use syn::TypeParen;
use syn::TypePtr;
use syn::TypeReference;
use syn::TypeSlice;
use syn::Visibility;
use syn::parse::Nothing;
use syn::parse_quote;
use syn::spanned::Spanned;

/// Represents whether an AST element is a node or token.
#[derive(PartialEq, Debug)]
enum AstKind {
    /// An AST node.
    Node { generic_ident: Ident },
    /// An AST token.
    Token { generic_ident: Ident },
}

enum SpecialCase {
    /// Where a Python method returns `impl Iterator`, and needs to be
    /// translated to return a `PyList` instead.
    ImplIterator,
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

    // Remove second generic and add "Py" prefix (`impl Ast<N>` into `impl PyAst`).
    make_py_self_ty(&mut py_impl.self_ty)?;

    py_impl.items =
        original
            .items
            .iter()
            .filter_map(filter_py_method)
            .cloned()
            .map(|mut py_fn| -> Result<(ImplItemFn, Option<SpecialCase>)> {
                if let Some(
                    AstKind::Node { ref generic_ident } | AstKind::Token { ref generic_ident },
                ) = ast_kind
                    && let ReturnType::Type(_, ref mut type_) = py_fn.sig.output
                {
                    // If the return type is `impl Iterator<...>`, replace it with `PyList` and
                    // mark this method as a special case.
                    if let Type::ImplTrait(ref type_impl_trait) = **type_
                        && type_impl_trait.bounds.len() == 1
                        && let Some(bound) = type_impl_trait.bounds.first()
                        && let TypeParamBound::Trait(trait_bound) = bound
                        && trait_bound.path.is_ident("Iterator")
                    {
                        return Ok((py_fn, Some(SpecialCase::ImplIterator)));
                    }

                    strip_path_generic(type_, generic_ident.clone())?;
                }

                Ok((py_fn, None))
            })
            .map(|result| {
                let (mut py_fn, special_case) = result?;

                if let Some(SpecialCase::ImplIterator) = special_case {
                    // Make private.
                    py_fn.vis = Visibility::Inherited;

                    // Add `'py` lifetime.
                    py_fn.sig.generics.params.push(parse_quote!('py));

                    // Add `py: Python<'py>` argument.
                    py_fn.sig.inputs.push(parse_quote!(py: ::pyo3::marker::Python<'py>));

                    // Set return type to `PyResult<Bound<'py, PyList>>`.
                    py_fn.sig.output = parse_quote!(-> ::pyo3::PyResult<::pyo3::Bound<'py, ::pyo3::types::PyList>>);

                    // Set body.
                    py_fn.block = match py_fn.sig.inputs.first() {
                        // Method that takes `self` by reference
                        Some(FnArg::Receiver(Receiver {
                            kind: ReceiverKind::Reference(..), ..
                        })) => parse_quote!({
                            ::pyo3::types::PyList::new(py, Original::from(self.clone()).iterator())
                        }),

                        // Method that consumes `self`
                        Some(FnArg::Receiver(Receiver {
                            kind: ReceiverKind::Value, ..
                        })) => parse_quote!({
                            ::pyo3::types::PyList::new(py, Original::from(self).iterator())
                        }),

                        // Associated function
                        Some(FnArg::Typed(_)) | None => parse_quote!({
                            ::pyo3::types::PyList::new(py, Original::iterator())
                        }),

                        Some(FnArg::Receiver(receiver)) => return Err(Error::new_spanned(receiver, "`#[ast_methods]` does not support this kind of receiver")),
                    };

                    return Ok(ImplItem::Fn(py_fn));
                }

                // TODO

                Ok(ImplItem::Fn(py_fn))
            })
            .collect::<Result<_>>()?;

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

/// Adds the "Py" prefix to the `self_ty`'s ident and removes its generic
/// parameters.
fn make_py_self_ty(self_ty: &mut Type) -> Result<()> {
    let type_path = match self_ty {
        // When encountering a group or parenthesized type, recurse into the inner element.
        Type::Group(TypeGroup { elem, .. }) | Type::Paren(TypeParen { elem, .. }) => {
            *self_ty = *elem.clone();
            return make_py_self_ty(self_ty);
        }
        Type::Path(type_path) => type_path,
        _ => {
            return Err(Error::new_spanned(
                &self_ty,
                "type not supported by `#[ast_methods]`",
            ));
        }
    };

    if let Some(ref qself) = type_path.qself {
        return Err(Error::new(
            qself.span(),
            "qualified paths are not supported by `#[ast_methods]`",
        ));
    }

    let last_segment = type_path
        .path
        .segments
        .last_mut()
        .expect("type paths should contain at least one segment");

    last_segment.ident = format_ident!("Py{}", last_segment.ident);

    // TODO: Verify only argument is `AstKind` ident.
    last_segment.arguments = PathArguments::None;

    Ok(())
}

/// Filters [`ImplItem`]s based on criteria for becoming Python methods.
///
/// In order for an [`ImplItem`] to become a Python method, it must:
///
/// - Be a function.
/// - Be public.
/// - Not be annotated with `#[skip]`.
fn filter_py_method(original: &ImplItem) -> Option<&ImplItemFn> {
    if let ImplItem::Fn(original_fn) = original
        && let Visibility::Public(..) = original_fn.vis
    {
        // TODO: Check for `#[skip]`.
        Some(original_fn)
    } else {
        None
    }
}

/// Recursively removes a generic parameter from all paths in a type.
fn strip_path_generic(type_: &mut Type, generic_ident: Ident) -> Result<()> {
    match type_ {
        Type::Path(type_path) => {
            let path_argument = PathArguments::AngleBracketed(parse_quote!(<#generic_ident>));

            for segments in type_path.path.segments.iter_mut() {
                if segments.arguments == path_argument {
                    segments.arguments = PathArguments::None;
                }
            }

            if let Some(ref mut qself) = type_path.qself {
                strip_path_generic(&mut qself.ty, generic_ident)?;
            }

            Ok(())
        }
        Type::Array(TypeArray { elem, .. })
        | Type::Group(TypeGroup { elem, .. })
        | Type::Paren(TypeParen { elem, .. })
        | Type::Ptr(TypePtr { elem, .. })
        | Type::Slice(TypeSlice { elem, .. })
        | Type::Reference(TypeReference { elem, .. }) => strip_path_generic(elem, generic_ident),
        Type::Tuple(type_tuple) => {
            for elem in type_tuple.elems.iter_mut() {
                strip_path_generic(elem, generic_ident.clone())?;
            }

            Ok(())
        }
        Type::FnPtr(type_fn_ptr) => {
            for elem in type_fn_ptr.inputs.iter_mut() {
                strip_path_generic(&mut elem.ty, generic_ident.clone())?;
            }

            if let ReturnType::Type(_, ref mut elem) = type_fn_ptr.output {
                strip_path_generic(elem, generic_ident)?;
            }

            Ok(())
        }
        //  While `impl Trait` and `dyn Trait` may contain paths with generics, we don't support
        // stripping them.
        Type::ImplTrait(_)
        | Type::TraitObject(_)
        | Type::Infer(_)
        | Type::Macro(_)
        | Type::Never(_) => Ok(()),
        unsupported => Err(Error::new(
            unsupported.span(),
            "`#[ast_methods]` does not support this return type",
        )),
    }
}

#[cfg(test)]
mod tests {
    use proc_macro2::Span;
    use quote::ToTokens;
    use syn::Path;
    use syn::TypeGroup;
    use syn::TypePath;
    use syn::punctuated::Punctuated;
    use syn::token::Group;

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

    #[test]
    fn self_ty() {
        let mut self_ty: Type = parse_quote!(Ast<T>);

        make_py_self_ty(&mut self_ty).unwrap();

        let expected: Type = parse_quote!(PyAst);

        assert_eq!(self_ty, expected);
    }

    #[test]
    fn self_ty_group() {
        let mut self_ty: Type = parse_quote!(Ast<T>);
        self_ty = Type::Group(TypeGroup {
            attrs: Vec::new(),
            group_token: Group::default(),
            elem: Box::new(self_ty),
        });

        make_py_self_ty(&mut self_ty).unwrap();

        let expected: Type = parse_quote!(PyAst);

        assert_eq!(self_ty, expected);
    }

    #[test]
    fn self_ty_paren() {
        let mut self_ty: Type = parse_quote!((Ast<T>));

        make_py_self_ty(&mut self_ty).unwrap();

        let expected: Type = parse_quote!(PyAst);

        assert_eq!(self_ty, expected);
    }

    #[test]
    fn self_ty_unsupported_type() {
        let mut self_ty: Type = parse_quote!([Ast<T>; 2]);

        let result = make_py_self_ty(&mut self_ty);

        assert!(
            result.is_err(),
            "did not error on unsupported `self_ty`: {self_ty:?}"
        );
    }

    #[test]
    fn self_ty_qself() {
        let mut self_ty: Type = parse_quote!(<SyntaxNode as TreeNode>::Token);

        let result = make_py_self_ty(&mut self_ty);

        assert!(
            result.is_err(),
            "did not error on qualified path: {self_ty:?}"
        );
    }

    #[test]
    #[should_panic = "type paths should contain at least one segment"]
    fn self_ty_empty_path() {
        // Manually construct the empty path, as it's not possible with
        // `parse_quote!()`.
        let mut self_ty = Type::Path(TypePath {
            attrs: Vec::new(),
            qself: None,
            path: Path {
                leading_colon: None,
                segments: Punctuated::new(),
            },
        });

        let _ = make_py_self_ty(&mut self_ty);
    }

    #[test]
    fn filter_pub_method() {
        let original: ImplItem = parse_quote! { pub fn foo(&self) {} };
        assert!(filter_py_method(&original).is_some());
    }

    #[test]
    fn filter_priv_method() {
        let original: ImplItem = parse_quote! { fn foo(&self) {} };
        let result = filter_py_method(&original);
        assert!(
            result.is_none(),
            "did not filter out private method: {result:?}"
        );
    }

    #[test]
    fn filter_pub_const() {
        let original: ImplItem = parse_quote! { pub const FOO: &str = "hello"; };
        let result = filter_py_method(&original);
        assert!(result.is_none(), "did not filter out const: {result:?}");
    }

    #[test]
    fn strip_path_generic() {
        let generic_ident = Ident::new("N", Span::call_site());

        // The first item in each tuple will be fed into `strip_path_generic()`, and the
        // result will be compared with the second item.
        let test_cases: [(Type, Type); _] = [
            // Paths
            (parse_quote!(Ast), parse_quote!(Ast)),
            (parse_quote!(Ast<N>), parse_quote!(Ast)),
            (parse_quote!(Ast<T>), parse_quote!(Ast<T>)),
            (parse_quote!(super::Ast<N>), parse_quote!(super::Ast)),
            (
                parse_quote!(Ast<N>::Associated),
                parse_quote!(Ast::Associated),
            ),
            (
                parse_quote!(<Ast<N>>::Associated),
                parse_quote!(<Ast>::Associated),
            ),
            (
                parse_quote!(<Ast<N> as Foo<N>>::Associated),
                parse_quote!(<Ast as Foo>::Associated),
            ),
            // Types that wrap another type.
            (parse_quote!([Ast<N>; 3]), parse_quote!([Ast; 3])),
            (
                Type::Group(TypeGroup {
                    attrs: Vec::new(),
                    group_token: Group::default(),
                    elem: parse_quote!(Ast<N>),
                }),
                Type::Group(TypeGroup {
                    attrs: Vec::new(),
                    group_token: Group::default(),
                    elem: parse_quote!(Ast),
                }),
            ),
            (parse_quote!((Ast<N>)), parse_quote!((Ast))),
            (parse_quote!(*const Ast<N>), parse_quote!(*const Ast)),
            (parse_quote!(*mut Ast<N>), parse_quote!(*mut Ast)),
            (parse_quote!(&[Ast<N>]), parse_quote!(&[Ast])),
            (parse_quote!(&Ast<N>), parse_quote!(&Ast)),
            (parse_quote!(&mut Ast<N>), parse_quote!(&mut Ast)),
            // Tuples
            (
                parse_quote!((foo::Ast<N>, Ast<T>, (baz::Bar, Ast<N>))),
                parse_quote!((foo::Ast, Ast<T>, (baz::Bar, Ast))),
            ),
            // Function pointers
            (
                parse_quote!(fn(Ast<N>) -> Ast<N>),
                parse_quote!(fn(Ast) -> Ast),
            ),
            // Untouched types
            (
                parse_quote!(impl Iterator<Item = Ast<N>>),
                parse_quote!(impl Iterator<Item = Ast<N>>),
            ),
            (parse_quote!(dyn Foo<N>), parse_quote!(dyn Foo<N>)),
            (parse_quote!(_), parse_quote!(_)),
            (parse_quote!(Token![=]), parse_quote!(Token![=])),
            (parse_quote!(!), parse_quote!(!)),
        ];

        for (input, expected) in test_cases {
            let mut output = input.clone();
            super::strip_path_generic(&mut output, generic_ident.clone()).unwrap();
            pretty_assertions::assert_eq!(
                output,
                expected,
                "stripping `{}` of generic <{generic_ident}> did not result in expected type path",
                input.to_token_stream()
            );
        }
    }
}
