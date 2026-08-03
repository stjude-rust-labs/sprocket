//! TODO

use proc_macro2::TokenStream;
use quote::format_ident;
use quote::quote;
use syn::Error;
use syn::Expr;
use syn::ExprArray;
use syn::ExprPath;
use syn::ExprTuple;
use syn::FnArg;
use syn::Generics;
use syn::Ident;
use syn::ImplItem;
use syn::ImplItemFn;
use syn::ItemImpl;
use syn::Pat;
use syn::PatIdent;
use syn::Path;
use syn::PathArguments;
use syn::Receiver;
use syn::ReceiverKind;
use syn::Result;
use syn::ReturnType;
use syn::Token;
use syn::TraitBound;
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
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::Bracket;
use syn::token::Paren;

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

    // Get the identifier of the `TreeNode` / `TreeToken` generic parameter.
    let ast_generic_ident = ast_generic_ident(&original.generics)?;

    // Remove the first generic (`impl<N: TreeNode> Ast<N>` into `impl Ast<N>`).
    py_impl.generics = Generics::default();

    // Remove second generic and add "Py" prefix (`impl Ast<N>` into `impl PyAst`).
    let original_type_path = make_py_self_ty(&mut py_impl.self_ty)?;

    py_impl.items = original
        .items
        .iter()
        .filter_map(filter_py_method)
        .cloned()
        .map(|mut py_fn| -> Result<(ImplItemFn, Option<SpecialCase>)> {
            if let ReturnType::Type(_, ref mut type_) = py_fn.sig.output {
                // If the return type is `impl Iterator<...>`, mark this method as a special
                // case.
                if is_impl_iterator(type_)? {
                    return Ok((py_fn, Some(SpecialCase::ImplIterator)));
                }

                if let Some(ref generic_ident) = ast_generic_ident {
                    strip_path_generic(type_, generic_ident.clone())?;
                }
            }

            Ok((py_fn, None))
        })
        .map(move |result| {
            let (mut py_fn, special_case) = result?;

            let py_ident = format_ident!("py_{}", py_fn.sig.ident);
            let original_method_ident = std::mem::replace(&mut py_fn.sig.ident, py_ident);

            // Make private.
            py_fn.vis = Visibility::Inherited;

            // Set method body.
            if let Some(SpecialCase::ImplIterator) = special_case {
                make_py_method_body_impl_iterator(
                    &mut py_fn,
                    &original_type_path,
                    original_method_ident,
                )?;
            } else {
                make_py_method_body(&mut py_fn, &original_type_path, original_method_ident)?;
            }

            Ok(ImplItem::Fn(py_fn))
        })
        .collect::<Result<_>>()?;

    Ok(quote! {
        #original
        #py_impl
    })
}

/// Gets the [`Ident`] of the generic type bound by the `TreeNode` or
/// `TreeToken` trait.
///
/// If there are no generic types, this will return [`None`].
///
/// # Examples
///
/// - The generics `<N: TreeNode>` will return the ident `N`.
/// - The generics `<T: TreeToken>` will return the ident `T`.
/// - [`Generics::default()`] (empty generics) will return [`None`].
///
/// # Errors
///
/// This function will return an error when:
///
/// - There is a `where` clause.
/// - There are multiple type parameters.
/// - The type parameter is not bound by any traits (ex. `<T>`) or bound by more
///   than one trait (ex. `<T: TreeToken + Display>`).
fn ast_generic_ident(generics: &Generics) -> Result<Option<Ident>> {
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

    if type_param.bounds == parse_quote!(TreeNode) || type_param.bounds == parse_quote!(TreeToken) {
        Ok(Some(type_param.ident.clone()))
    } else {
        Err(Error::new_spanned(
            &type_param.bounds,
            "`#[ast_methods]` requires that trait bounds be exactly `TreeNode` or `TreeToken`",
        ))
    }
}

/// Adds the "Py" prefix to the `self_ty`'s ident and removes its generic
/// parameters. Returns the original ident without its generics.
fn make_py_self_ty(self_ty: &mut Type) -> Result<Path> {
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

    let mut original_path = type_path.path.clone();
    original_path
        .segments
        .last_mut()
        .expect("type paths should contain at least one segment")
        .arguments = PathArguments::None;

    let last_segment = type_path
        .path
        .segments
        .last_mut()
        .expect("type paths should contain at least one segment");

    // TODO: Verify only argument is `AstKind` ident.
    last_segment.arguments = PathArguments::None;
    last_segment.ident = format_ident!("Py{}", last_segment.ident);

    Ok(original_path)
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

/// Returns true if the given [`Type`] is of the form `impl Iterator<...>`.
///
/// Due to implementation details, only the first trait bound will be checked.
///
/// # Examples
///
/// - `impl Iterator<Item = ()>` is accepted.
/// - `impl std::iter::Iterator<Item = Ast<N>>` is accepted.
/// - `impl Iterator<Item = Ast<N>> + use<'_, N>` is accepted.
/// - `impl Debug` is rejected.
/// - `impl use<'_, N> + Iterator<Item = Ast<N>>` is rejected (`Iterator` must
///   be first).
/// - `impl for<'a> Iterator<Item = &'a Ast<N>>` is rejected (binders are not
///   allowed).
///
/// # Errors
///
/// This function will return an error if unstable
/// [`TraitBoundModifiers`](syn::TraitBoundModifiers) are used.
fn is_impl_iterator(type_: &Type) -> Result<bool> {
    if let Type::ImplTrait(type_impl_trait) = type_
        && !type_impl_trait.bounds.is_empty()
        // We only check the first type parameter bound for `Iterator`. A type like `impl Debug +
        // Iterator<...>` will be rejected.
        && let Some(bound) = type_impl_trait.bounds.first()
        // Filter by trait bounds that do not use binders and do not have a `?` prefix. Types like
        // `impl for<'a> Iterator<...>` and `impl ?Iterator<...>` will be rejeceted.
        && let TypeParamBound::Trait(TraitBound { path, modifiers, lifetimes: None, maybe: None, .. }) = bound
    {
        // Reject types that use unstable modifiers.
        modifiers.require_empty()?;

        // Accept `impl Iterator<...>`;
        if path.segments.len() == 1 && path.segments.first().unwrap().ident == "Iterator" {
            return Ok(true);
        }

        // Accept `impl std::iter::Iterator<...>`.
        if path.segments.len() == 3
            && path.segments.get(0).unwrap().ident == "std"
            && path.segments.get(1).unwrap().ident == "iter"
            && path.segments.get(2).unwrap().ident == "Iterator"
        {
            return Ok(true);
        }
    }

    Ok(false)
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

/// Sets the body of a Python method to call the original method.
///
/// # Examples
///
/// For methods that take `&self` or `&mut self`, this will set the Python
/// method body to:
///
/// ```
/// # #[derive(Clone)]
/// # struct Struct;
/// # impl Struct {
/// #     fn method(&self) {}
/// #     fn py_method(&self) {
/// Struct::from(self.clone()).method(/* ... */)
/// #     }
/// # }
/// ```
///
/// For methods that take `self` or `self: Box<Self>`, this will set the Python
/// method body to:
///
/// ```
/// # struct Struct;
/// # impl Struct {
/// #     fn method(self) {}
/// #     fn py_method(self) {
/// Struct::from(self).method(/* ... */)
/// #     }
/// # }
/// ```
///
/// For associated functions, this will set the Python method body to:
///
/// ```
/// # struct Struct;
/// # impl Struct {
/// #     fn method() {}
/// #     fn py_method() {
/// Struct::method(/* ...  */)
/// #     }
/// # }
/// ```
///
/// # Errors
///
/// This calls [`fn_inputs_to_args()`] internally and forwards any returned
/// errors.
///
/// Additionally, this will return an error when it encounters an unknown
/// receiver kind such as `&pin self`.
fn make_py_method_body(
    py_fn: &mut ImplItemFn,
    original_type_path: &Path,
    original_method_ident: Ident,
) -> Result<()> {
    // Convert function inputs into function arguments. (Ex. turn `a: usize, b:
    // String` into `a, b`.)
    let method_args = fn_inputs_to_args(&py_fn.sig.inputs)?;

    py_fn.block = match py_fn.sig.inputs.first() {
        // Method that takes `self` by reference
        Some(FnArg::Receiver(Receiver {
            kind: ReceiverKind::Reference(..),
            ..
        })) => parse_quote!({
            #original_type_path::from(self.clone()).#original_method_ident(#method_args)
        }),

        // Method that consumes `self`
        Some(FnArg::Receiver(Receiver {
            kind: ReceiverKind::Value | ReceiverKind::Typed(..),
            ..
        })) => parse_quote!({
            #original_type_path::from(self).#original_method_ident(#method_args)
        }),

        // Associated function
        Some(FnArg::Typed(_)) | None => parse_quote!({
            #original_type_path::#original_method_ident(#method_args)
        }),

        Some(FnArg::Receiver(receiver)) => {
            return Err(Error::new_spanned(
                receiver,
                "`#[ast_methods]` does not support this kind of receiver",
            ));
        }
    };

    Ok(())
}

/// A variant of [`make_py_method_body()`] that adapts methods returning `impl
/// Iterator` to instead return `PyList`.
///
/// This function should only be called on Python methods whose return type
/// passes [`is_impl_iterator()`]. If [`is_impl_iterator()`] returns false, call
/// [`make_py_method_body()`] instead.
///
/// This function performs the following actions:
///
/// 1. Appends `'py` as a lifetime generic parameter.
/// 2. Appends `py: Python<'py>` as a function parameter.
/// 3. Sets the return type to `PyResult<Bound<'py, PyList>>`.
/// 4. Makes the function body call the original method and pass the returned
///    iterator to `PyList::new()`.
///
/// # Examples
///
/// Given the following method:
///
/// ```ignore
/// fn py_method(&self) -> impl Iterator<Item = ...> {
///     // ...
/// }
/// ```
///
/// This function will modify it into the following:
///
/// ```
/// fn py_method<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
///     PyList::new(py, Struct::from(self.clone()).method())
/// }
/// ```
///
/// # Errors
///
/// This function has the exact same error conditions as
/// [`make_py_method_body()`].
fn make_py_method_body_impl_iterator(
    py_fn: &mut ImplItemFn,
    original_type_path: &Path,
    original_method_ident: Ident,
) -> Result<()> {
    // Convert function inputs into function arguments. (Ex. turn `a: usize, b:
    // String` into `a, b`.) This is purposefully called before we add `py:
    // Python<'py>` to the function input, as the Python marker token is not passed
    // to the original method.
    let method_args = fn_inputs_to_args(&py_fn.sig.inputs)?;

    // Add `'py` lifetime.
    py_fn.sig.generics.params.push(parse_quote!('py));

    // Add `py: Python<'py>` argument.
    py_fn
        .sig
        .inputs
        .push(parse_quote!(py: ::pyo3::marker::Python<'py>));

    // Set return type to `PyResult<Bound<'py, PyList>>`.
    py_fn.sig.output = parse_quote!(-> ::pyo3::PyResult<::pyo3::Bound<'py, ::pyo3::types::PyList>>);

    // Set body.
    py_fn.block = match py_fn.sig.inputs.first() {
        // Method that takes `self` by reference
        Some(FnArg::Receiver(Receiver {
            kind: ReceiverKind::Reference(..),
            ..
        })) => parse_quote!({
            ::pyo3::types::PyList::new(py, #original_type_path::from(self.clone()).#original_method_ident(#method_args))
        }),

        // Method that consumes `self`
        Some(FnArg::Receiver(Receiver {
            kind: ReceiverKind::Value | ReceiverKind::Typed(..),
            ..
        })) => parse_quote!({
            ::pyo3::types::PyList::new(py, #original_type_path::from(self).#original_method_ident(#method_args))
        }),

        // Associated function
        Some(FnArg::Typed(_)) | None => parse_quote!({
            ::pyo3::types::PyList::new(py, #original_type_path::#original_method_ident(#method_args))
        }),

        Some(FnArg::Receiver(receiver)) => {
            return Err(Error::new_spanned(
                receiver,
                "`#[ast_methods]` does not support this kind of receiver",
            ));
        }
    };

    Ok(())
}

/// Converts function signature inputs into function call arguments.
///
/// This function skips receiver inputs like `&self`, assuming they will be
/// handled by the caller.
///
/// This function only supports the following input pattern kinds:
///
/// - Idents (`foo: usize`)
/// - Parentheseses (`(foo): usize`)
/// - Slices (`[a, b]: [usize; 2]`)
/// - Tuples (`(a, b, c): (usize, usize, usize)`)
///
/// Input patterns that discard data, like `_` and `..`, as well as more complex
/// patterns, like `Struct { a, b }`, are not supported.
///
/// # Examples
///
/// - `a: usize, b: String` will result in `a, b`.
/// - `&self, a: bool` will result in `a`.
/// - `[a, b]: [usize; 2]` will result in `[a, b]`.
/// - `(a, b, c): (usize, usize, usize)` will result in `(a, b, c)`.
/// - `(a, [b, c]): (usize, [usize; 2])` will result in `(a, [b, c])`.
///
/// # Errors
///
/// This function will return an error if it encounters an unsupported input
/// pattern.
fn fn_inputs_to_args(inputs: &Punctuated<FnArg, Token![,]>) -> Result<Punctuated<Expr, Token![,]>> {
    fn inner(pat_type: &Pat) -> Result<Expr> {
        match pat_type {
            Pat::Ident(PatIdent {
                ident,
                by_ref: None,
                ..
            }) => Ok(Expr::Path(ExprPath {
                attrs: Vec::new(),
                qself: None,
                path: Path::from(ident.clone()),
            })),
            Pat::Paren(pat_paren) => inner(&pat_paren.pat),
            Pat::Slice(pat_slice) => Ok(Expr::Array(ExprArray {
                attrs: Vec::new(),
                bracket_token: Bracket::default(),
                elems: pat_slice.elems.iter().map(inner).collect::<Result<_>>()?,
            })),
            Pat::Tuple(pat_tuple) => Ok(Expr::Tuple(ExprTuple {
                attrs: Vec::new(),
                paren_token: Paren::default(),
                elems: pat_tuple.elems.iter().map(inner).collect::<Result<_>>()?,
            })),
            unexpected => Err(Error::new_spanned(
                unexpected,
                "`#[ast_methods]` does not support this pattern in arguments",
            )),
        }
    }

    inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(pat_type) => Some(&*pat_type.pat),
            // Discard the receiver, which is usually `&self`.
            FnArg::Receiver(_) => None,
        })
        .map(inner)
        .collect()
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
    fn generic_ident_node() {
        let original: ItemImpl = parse_quote! { impl<N: TreeNode> Ast<N> {} };

        let ast_kind = ast_generic_ident(&original.generics).unwrap().unwrap();

        assert_eq!(ast_kind, Ident::new("N", Span::call_site()));
    }

    #[test]
    fn generic_ident_token() {
        let original: ItemImpl = parse_quote! { impl<T: TreeToken> Ast<T> {} };

        let ast_kind = ast_generic_ident(&original.generics).unwrap().unwrap();

        assert_eq!(ast_kind, Ident::new("T", Span::call_site()));
    }

    #[test]
    fn generic_ident_no_params() {
        let original: ItemImpl = parse_quote! { impl Ast {} };

        let ast_kind = ast_generic_ident(&original.generics).unwrap();

        assert!(
            ast_kind.is_none(),
            "did not return `None` for zero type parameters: {ast_kind:?}"
        );
    }

    #[test]
    fn generic_ident_two_params() {
        let original: ItemImpl = parse_quote! { impl<N: TreeNode, T: TreeToken> Ast<N> {} };

        let result = ast_generic_ident(&original.generics);

        assert!(
            result.is_err(),
            "did not error on multiple type parameters: {result:?}"
        );
    }

    #[test]
    fn generic_ident_where_clause() {
        let original: ItemImpl = parse_quote! { impl<N> Ast<N> where N: TreeNode {} };

        let result = ast_generic_ident(&original.generics);

        assert!(result.is_err(), "did not error on where clause: {result:?}");
    }

    #[test]
    fn generic_ident_invalid_trait_bound() {
        let original: ItemImpl = parse_quote! { impl<T: Display> Ast<T> {} };

        let result = ast_generic_ident(&original.generics);

        assert!(
            result.is_err(),
            "did not error on invalid trait bound: {result:?}"
        );
    }

    #[test]
    fn generic_ident_multiple_trait_bounds() {
        let original: ItemImpl = parse_quote! { impl<T: TreeToken + Display> Ast<T> {} };

        let result = ast_generic_ident(&original.generics);

        assert!(
            result.is_err(),
            "did not error on multiple trait bound: {result:?}"
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
    fn impl_iterator() {
        let type_: Type = parse_quote!(impl Iterator<Item = ()>);
        assert!(
            is_impl_iterator(&type_).unwrap(),
            "type was incorrectly marked as not an `impl Iterator`: {type_:?}"
        );
    }

    #[test]
    fn impl_iterator_qualified() {
        let type_: Type = parse_quote!(impl std::iter::Iterator<Item = Ast<N>>);
        assert!(
            is_impl_iterator(&type_).unwrap(),
            "type was incorrectly marked as not an `impl Iterator`: {type_:?}"
        );
    }

    #[test]
    fn impl_iterator_use_bound() {
        let type_: Type = parse_quote!(impl Iterator<Item = Ast<N>> + use<'_, N>);
        assert!(
            is_impl_iterator(&type_).unwrap(),
            "type was incorrectly marked as not an `impl Iterator`: {type_:?}"
        );
    }

    #[test]
    fn impl_iterator_incorrect_trait() {
        let type_: Type = parse_quote!(impl Debug);
        assert!(
            !is_impl_iterator(&type_).unwrap(),
            "type was incorrectly marked as an `impl Iterator`: {type_:?}"
        );
    }

    #[test]
    fn impl_iterator_incorrect_order() {
        let type_: Type = parse_quote!(impl use<'_, N> + Iterator<Item = Ast<N>>);
        assert!(
            !is_impl_iterator(&type_).unwrap(),
            "type was incorrectly marked as an `impl Iterator`: {type_:?}"
        );
    }

    #[test]
    fn impl_iterator_binder() {
        let type_: Type = parse_quote!(impl for<'a> Iterator<Item = &'a Ast<N>>);
        assert!(
            !is_impl_iterator(&type_).unwrap(),
            "type was incorrectly marked as an `impl Iterator`: {type_:?}"
        );
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

    #[test]
    fn py_method_body_reference_receiver() {
        let mut py_fn = parse_quote!(
            fn py_method(&self, a: usize) {}
        );

        make_py_method_body(&mut py_fn, &parse_quote!(Ast), parse_quote!(method)).unwrap();

        let expected = parse_quote! {
            fn py_method(&self, a: usize) {
                Ast::from(self.clone()).method(a)
            }
        };

        pretty_assertions::assert_eq!(py_fn, expected);
    }

    #[test]
    fn py_method_body_mut_reference_receiver() {
        let mut py_fn = parse_quote!(
            fn py_method(&mut self) {}
        );

        make_py_method_body(&mut py_fn, &parse_quote!(Ast), parse_quote!(method)).unwrap();

        let expected = parse_quote! {
            fn py_method(&mut self) {
                Ast::from(self.clone()).method()
            }
        };

        pretty_assertions::assert_eq!(py_fn, expected);
    }

    #[test]
    fn py_method_body_value_receiver() {
        let mut py_fn = parse_quote!(
            fn py_method(self, [a, b]: [usize; 2]) {}
        );

        make_py_method_body(&mut py_fn, &parse_quote!(Ast), parse_quote!(method)).unwrap();

        let expected = parse_quote! {
            fn py_method(self, [a, b]: [usize; 2]) {
                Ast::from(self).method([a, b])
            }
        };

        pretty_assertions::assert_eq!(py_fn, expected);
    }

    #[test]
    fn py_method_body_associated_args() {
        let mut py_fn = parse_quote!(
            fn py_method(a: String) -> bool {}
        );

        make_py_method_body(&mut py_fn, &parse_quote!(Ast), parse_quote!(method)).unwrap();

        let expected = parse_quote! {
            fn py_method(a: String) -> bool {
                Ast::method(a)
            }
        };

        pretty_assertions::assert_eq!(py_fn, expected);
    }

    #[test]
    fn py_method_body_associated_no_args() {
        let mut py_fn = parse_quote!(
            fn py_method() -> bool {}
        );

        make_py_method_body(&mut py_fn, &parse_quote!(Ast), parse_quote!(method)).unwrap();

        let expected = parse_quote! {
            fn py_method() -> bool {
                Ast::method()
            }
        };

        pretty_assertions::assert_eq!(py_fn, expected);
    }

    #[test]
    fn inputs_to_args_idents() {
        let input = parse_quote!(a: usize, b: String);
        let expected = parse_quote!(a, b);
        assert_eq!(fn_inputs_to_args(&input).unwrap(), expected);
    }

    #[test]
    fn inputs_to_args_receiver() {
        let input = parse_quote!(&self, a: usize);
        let expected = parse_quote!(a);
        assert_eq!(fn_inputs_to_args(&input).unwrap(), expected);
    }

    #[test]
    fn inputs_to_args_paren() {
        let input = parse_quote!((a): usize);
        let expected = parse_quote!(a);
        assert_eq!(fn_inputs_to_args(&input).unwrap(), expected);
    }

    #[test]
    fn inputs_to_args_slice() {
        let input = parse_quote!([a, b]: [usize; 2]);
        let expected = parse_quote!([a, b]);
        assert_eq!(fn_inputs_to_args(&input).unwrap(), expected);
    }

    #[test]
    fn inputs_to_args_tuple() {
        let input = parse_quote!((a, b, c): (usize, usize, usize));
        let expected = parse_quote!((a, b, c));
        assert_eq!(fn_inputs_to_args(&input).unwrap(), expected);
    }

    #[test]
    fn inputs_to_args_nested() {
        let input = parse_quote!((a, [b, c]): (usize, [usize; 2]));
        let expected = parse_quote!((a, [b, c]));
        assert_eq!(fn_inputs_to_args(&input).unwrap(), expected);
    }

    #[test]
    fn inputs_to_args_rest() {
        let input = parse_quote!([..]: &[usize]);
        let result = fn_inputs_to_args(&input);
        assert!(result.is_err(), "did not error on rest pattern: {result:?}");
    }

    #[test]
    fn inputs_to_args_struct() {
        let input = parse_quote!(Struct { a, b }: Struct);
        let result = fn_inputs_to_args(&input);
        assert!(
            result.is_err(),
            "did not error on struct pattern: {result:?}"
        );
    }

    #[test]
    fn inputs_to_args_wildcard() {
        let input = parse_quote!(_: usize);
        let result = fn_inputs_to_args(&input);
        assert!(
            result.is_err(),
            "did not error on wildcard pattern: {result:?}"
        );
    }
}
