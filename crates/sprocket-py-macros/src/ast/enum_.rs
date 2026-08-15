//! The `#[ast]` implementation for enums.

use proc_macro2::TokenStream;
use quote::format_ident;
use quote::quote;
use syn::Arm;
use syn::Error;
use syn::Fields;
use syn::FieldsUnnamed;
use syn::Generics;
use syn::Ident;
use syn::ItemEnum;
use syn::LitStr;
use syn::PathArguments;
use syn::Result;
use syn::Token;
use syn::Type;
use syn::parse_quote;
use syn::punctuated::Punctuated;
use syn::token::Paren;

use super::Args;

/// Builds the Python binding equivalent of an AST enum.
pub(super) fn build(original: &ItemEnum, args: Args) -> Result<TokenStream> {
    let mut py_enum = original.clone();

    py_enum.ident = super::make_py_ident(&original.ident);

    // Remove generics.
    py_enum.generics = Generics::default();

    let only_unit_variants = has_only_unit_variants(&original);

    make_py_attrs(&mut py_enum, original, &args, only_unit_variants);

    // We only need to modify the variants if some of them aren't unit variants.
    if !only_unit_variants {
        // Strip generics from all variants, add "Py" prefix to all contained types,
        // convert unit variants to empty tuple variants.
        make_py_variants(&mut py_enum)?;
    }

    // Used for quote formatting.
    let ident = &original.ident;
    let py_ident = &py_enum.ident;
    let (original_to_py_match_arms, py_to_original_match_arms) =
        conversion_match_arms(original, py_ident, only_unit_variants);
    let partial_eq_impl = if args.eq {
        quote! {
            // Implements `PartialEq` for the Python enum by relying on the original enum's
            // impl. We can't derive `PartialEq`, as it will compare `ThreadSafeSyntax{Node,Token}`
            // and ignore custom implementations.
            impl ::std::cmp::PartialEq for #py_ident {
                fn eq(&self, other: &#py_ident) -> bool {
                    #ident::from(self.clone()) == #ident::from(other.clone())
                }
            }
        }
    } else {
        TokenStream::new()
    };
    let display_impl = if args.str_ {
        quote! {
            // Implements `Display` for the Python enum by relying on the original enum's
            // impl. This is only needed for `#[ast(str)]`, as `Display` is not needed on the
            // Python enum when `str` is omitted.
            impl ::std::fmt::Display for #py_ident {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    <#ident as ::std::fmt::Display>::fmt(&self.clone().into(), f)
                }
            }
        }
    } else {
        TokenStream::new()
    };

    Ok(quote! {
        #py_enum

        // Converting from the original enum to the Python enum. This is used in the original
        // enum's `IntoPyObject` impl.
        impl ::std::convert::From<#ident> for #py_ident {
            fn from(value: #ident) -> Self {
                match value {
                    #original_to_py_match_arms
                }
            }
        }

        // Converting from the Python enum to the original enum. This is used by Python methods
        // to call their original counterparts.
        impl ::std::convert::From<#py_ident> for #ident {
            fn from(value: #py_ident) -> Self {
                match value {
                    #py_to_original_match_arms
                }
            }
        }

        // Let the original enum be converted directly into a Python object. This lets the original
        // enum be returned directly from Python methods.
        impl<'py> ::pyo3::conversion::IntoPyObject<'py> for #ident {
            type Target = <#py_ident as ::pyo3::conversion::IntoPyObject<'py>>::Target;
            type Output = <#py_ident as ::pyo3::conversion::IntoPyObject<'py>>::Output;
            type Error = <#py_ident as ::pyo3::conversion::IntoPyObject<'py>>::Error;

            // We qualify `Self::Output` because this would be ambiguous for enums with a variant
            // named `Output`.
            fn into_pyobject(self, py: ::pyo3::marker::Python<'py>) -> ::std::result::Result<<Self as ::pyo3::conversion::IntoPyObject<'py>>::Output, Self::Error> {
                #py_ident::from(self).into_pyobject(py)
            }
        }

        #partial_eq_impl

        #display_impl
    })
}

/// Returns true if the enum is composed of only unit variants.
fn has_only_unit_variants(original: &ItemEnum) -> bool {
    original
        .variants
        .iter()
        .all(|variant| matches!(variant.fields, Fields::Unit))
}

/// Modifies the attributes of `py_enum`.
///
/// This first removes all of `py_enum`'s attributes except for its doc
/// comments. Then, it appends attributes like `#[pyclass]` that are necessary
/// for making it a Python type.
fn make_py_attrs(
    py_enum: &mut ItemEnum,
    original: &ItemEnum,
    args: &Args,
    only_unit_variants: bool,
) {
    // Only copy over doc comments, remove all other attributes.
    py_enum.attrs.retain(|attr| attr.path().is_ident("doc"));

    py_enum.attrs.extend_from_slice(&[
        // Make generated enum a pyclass.
        {
            let module = &args.module;
            let class_name = LitStr::new(&original.ident.to_string(), original.ident.span());

            parse_quote!(#[::pyo3::pyclass(module = #module, name = #class_name, frozen, from_py_object)])
        },
        // We rely on the Python enum being cloneable in `#[ast_methods]` and several generated
        // implementations.
        parse_quote!(#[derive(Clone)]),
        // `Debug` is purposefully not implemented, silence the lint.
        parse_quote!(#[allow(missing_debug_implementations)]),
    ]);

    if args.eq {
        py_enum.attrs.push(parse_quote!(#[pyo3(eq)]));
    }

    if args.str_ {
        py_enum.attrs.push(parse_quote!(#[pyo3(str)]));
    }

    if only_unit_variants {
        py_enum
            .attrs
            .push(parse_quote!(#[pyo3(rename_all = "SCREAMING_SNAKE_CASE")]));
    }
}

/// Modifies the Python enum's variants.
///
/// This function performs three transformations:
///
/// 1. Converts unit variants to empty tuple variants, so that `#[pyclass]`
///    doesn't raise an error. (Ex. `MyEnum::Foo` turns into `MyEnum::Foo()`).
/// 2. Prefixes variant field type names with "Py". (Ex. `MyEnum::Foo(Ast<N>)`
///    turns into `MyEnum::Foo(PyAst<N>)`).
/// 3. Removes generics from variant field types. (Ex. `MyEnum::Foo(PyAst<N>)`
///    turns into `MyEnum::Foo(PyAst)`).
///
/// This function assumes that the enum's variants only contain types annotated
/// with `#[ast]`.
///
/// This function is only intended to be run on enums that mix tuple and unit
/// variants. If the enum only has unit variants (e.g.
/// [`has_only_unit_variants()`] returns true), this function should not be run.
///
/// # Errors
///
/// This function will return an error if the enum has a struct variant or if a
/// single variant has more than one field.
fn make_py_variants(py_enum: &mut ItemEnum) -> Result<()> {
    for variant in &mut py_enum.variants {
        let fields = match variant.fields {
            Fields::Unnamed(ref mut fields) => fields,
            Fields::Unit => {
                // PyO3 doesn't support mixing tuple and unit variants in the same struct, so we
                // convert unit variants to tuple variants.
                variant.fields = Fields::Unnamed(FieldsUnnamed {
                    paren_token: Paren::default(),
                    unnamed: Punctuated::new(),
                });
                continue;
            }
            Fields::Named(_) => {
                return Err(Error::new_spanned(
                    &variant.fields,
                    "`#[ast]` does not support struct variants",
                ));
            }
        };

        if fields.unnamed.is_empty() {
            continue;
        }

        if fields.unnamed.len() > 1 {
            return Err(Error::new_spanned(
                fields,
                "`#[ast]` requires enums variants have zero or one fields",
            ));
        }

        if let Type::Path(ref mut type_path) = fields.unnamed.first_mut().unwrap().ty {
            let segment = type_path
                .path
                .segments
                .last_mut()
                .expect("paths are expected to have at least one segment");

            segment.ident = format_ident!("Py{}", segment.ident);
            segment.arguments = PathArguments::None;
        }
    }

    Ok(())
}

/// Generates the `match` arms necessary to convert between the original and
/// Python enum.
///
/// The first list returned should be used when converting from the
/// origin enum to the Python enum. The second list returned should be
/// used when converting from the Python enum back to the original enum.
///
/// # Panics
///
/// This function assumes that `original` has no struct variants, and will panic
/// if it finds one. Furthermore, if `only_unit_variants` is true, it will also
/// panic if it finds a tuple variant.
fn conversion_match_arms(
    original: &ItemEnum,
    py_ident: &Ident,
    only_unit_variants: bool,
) -> (Punctuated<Arm, Token![,]>, Punctuated<Arm, Token![,]>) {
    let ident = &original.ident;

    if only_unit_variants {
        return original
            .variants
            .iter()
            .map::<(Arm, Arm), _>(move |variant| {
                let variant_ident = &variant.ident;

                match variant.fields {
                    Fields::Unit => (
                        // Original to Python
                        parse_quote!(#ident::#variant_ident => #py_ident::#variant_ident),
                        // Python to original
                        parse_quote!(#py_ident::#variant_ident => #ident::#variant_ident),
                    ),
                    Fields::Named(_) => {
                        unreachable!("struct variants were previously filtered out")
                    }
                    Fields::Unnamed(_) => unreachable!(
                        "unnamed variants should not occur when `only_unit_variants` is true"
                    ),
                }
            })
            .unzip();
    }

    original
        .variants
        .iter()
        .map::<(Arm, Arm), _>(move |variant| {
            let variant_ident = &variant.ident;

            match variant.fields {
                Fields::Unnamed(ref fields_unnamed) => {
                    let lhs = (0..fields_unnamed.unnamed.len()).map(|i| format_ident!("_{i}"));
                    let rhs = lhs.clone().map(|i| quote! { #i.into() });

                    let (lhs2, rhs2) = (lhs.clone(), rhs.clone());

                    (
                        // Original to Python
                        parse_quote!(#ident::#variant_ident(#(#lhs),*) => #py_ident::#variant_ident(#(#rhs),*)),
                        // Python to original
                        parse_quote!(#py_ident::#variant_ident(#(#lhs2),*) => #ident::#variant_ident(#(#rhs2),*)),
                    )
                },
                // Convert unit variant to empty tuple variant.
                Fields::Unit => (
                    // Original to Python
                    parse_quote!(#ident::#variant_ident => #py_ident::#variant_ident()),
                    // Python to original
                    parse_quote!(#py_ident::#variant_ident() => #ident::#variant_ident),
                ),
                Fields::Named(_) => unreachable!("struct variants were previously filtered out"),
            }
        })
        .unzip()
}

#[cfg(test)]
mod tests {
    use proc_macro2::Span;

    use super::*;

    #[test]
    fn only_unit_variants() {
        let unit_variants = parse_quote! {
            enum MyEnum {
                Foo,
                Bar,
            }
        };

        assert!(has_only_unit_variants(&unit_variants));

        let tuple_variants = parse_quote! {
            enum MyEnum {
                Foo(),
                Bar,
            }
        };

        assert!(!has_only_unit_variants(&tuple_variants));

        let struct_variants = parse_quote! {
            enum MyEnum {
                Foo,
                Bar {},
            }
        };

        assert!(!has_only_unit_variants(&struct_variants));
    }

    #[test]
    fn attrs() {
        let original: ItemEnum = parse_quote! {
            /// Doc comment
            /** Another doc comment */
            #[derive(Hash)]
            enum Foo {}
        };
        let mut py_enum = original.clone();

        make_py_attrs(&mut py_enum, &original, &Args::default(), false);

        let expected = [
            parse_quote!(#[doc = r" Doc comment"]),
            parse_quote!(#[doc = r" Another doc comment "]),
            parse_quote!(#[::pyo3::pyclass(module = "sprocket_bio.ast.v1", name = "Foo", frozen, from_py_object)]),
            parse_quote!(#[derive(Clone)]),
            parse_quote!(#[allow(missing_debug_implementations)]),
        ];

        pretty_assertions::assert_eq!(py_enum.attrs, expected);
    }

    #[test]
    fn attrs_with_args() {
        let original: ItemEnum = parse_quote! {
            /// Hello, there!
            enum Bar {}
        };
        let mut py_enum = original.clone();

        make_py_attrs(
            &mut py_enum,
            &original,
            &Args {
                module: LitStr::new("sprocket_bio.custom_module", Span::call_site()),
                eq: true,
                str_: true,
            },
            false,
        );

        let expected = [
            parse_quote!(#[doc = r" Hello, there!"]),
            parse_quote!(#[::pyo3::pyclass(module = "sprocket_bio.custom_module", name = "Bar", frozen, from_py_object)]),
            parse_quote!(#[derive(Clone)]),
            parse_quote!(#[allow(missing_debug_implementations)]),
            parse_quote!(#[pyo3(eq)]),
            parse_quote!(#[pyo3(str)]),
        ];

        pretty_assertions::assert_eq!(py_enum.attrs, expected);
    }

    #[test]
    fn attrs_only_unit_variants() {
        let original: ItemEnum = parse_quote! {
            enum MyEnum {
                Foo,
                Bar,
            }
        };
        let mut py_enum = original.clone();

        make_py_attrs(&mut py_enum, &original, &Args::default(), true);

        let expected = [
            parse_quote!(#[::pyo3::pyclass(module = "sprocket_bio.ast.v1", name = "MyEnum", frozen, from_py_object)]),
            parse_quote!(#[derive(Clone)]),
            parse_quote!(#[allow(missing_debug_implementations)]),
            parse_quote!(#[pyo3(rename_all = "SCREAMING_SNAKE_CASE")]),
        ];

        pretty_assertions::assert_eq!(py_enum.attrs, expected);
    }

    #[test]
    fn variants_unit_to_tuple() {
        let mut py_enum = parse_quote! {
            enum MyEnum {
                Foo(),
                Bar,
                Baz,
            }
        };

        make_py_variants(&mut py_enum).unwrap();

        for variant in py_enum.variants {
            assert!(
                matches!(&variant.fields, Fields::Unnamed(FieldsUnnamed { unnamed, .. }) if unnamed.is_empty()),
                "found a non-unit variant: {variant:?}",
            );
        }
    }

    #[test]
    fn variants_fields_to_py_type() {
        let mut py_enum = parse_quote! {
            enum Type {
                Map(MapType<N>),
                Array(ArrayType<N>),
                Pair(PairType<N>),
                Object(ObjectType<N>),
                Ref(TypeRef<N>),
                Primitive(PrimitiveType<N>),
            }
        };

        make_py_variants(&mut py_enum).unwrap();

        let expected = parse_quote! {
            enum Type {
                Map(PyMapType),
                Array(PyArrayType),
                Pair(PyPairType),
                Object(PyObjectType),
                Ref(PyTypeRef),
                Primitive(PyPrimitiveType),
            }
        };

        pretty_assertions::assert_eq!(py_enum, expected);
    }

    #[test]
    fn variants_struct() {
        let mut py_enum = parse_quote! {
            enum MyEnum {
                Baz {},
            }
        };

        let result = make_py_variants(&mut py_enum);

        assert!(
            result.is_err(),
            "did not error on struct variant: {result:?}"
        );
    }

    #[test]
    fn variants_multiple_fields() {
        let mut py_enum = parse_quote! {
            enum MyEnum {
                Foo(Ast<N>, Ast<N>),
            }
        };

        let result = make_py_variants(&mut py_enum);

        assert!(
            result.is_err(),
            "did not error on multiple fields in a variant: {result:?}"
        );
    }

    #[test]
    fn conversion_arms() {
        let original = parse_quote! {
            enum MyEnum {
                Foo(Ast),
                Bar,
                Baz(Float),
            }
        };

        let (original_to_py, py_to_original) =
            conversion_match_arms(&original, &Ident::new("PyMyEnum", Span::call_site()), false);

        let expected_original_to_py = Punctuated::from_iter::<[Arm; _]>([
            parse_quote!(MyEnum::Foo(_0) => PyMyEnum::Foo(_0.into())),
            parse_quote!(MyEnum::Bar => PyMyEnum::Bar()),
            parse_quote!(MyEnum::Baz(_0) => PyMyEnum::Baz(_0.into())),
        ]);

        let expected_py_to_original = Punctuated::from_iter::<[Arm; _]>([
            parse_quote!(PyMyEnum::Foo(_0) => MyEnum::Foo(_0.into())),
            parse_quote!(PyMyEnum::Bar() => MyEnum::Bar),
            parse_quote!(PyMyEnum::Baz(_0) => MyEnum::Baz(_0.into())),
        ]);

        pretty_assertions::assert_eq!(original_to_py, expected_original_to_py);
        pretty_assertions::assert_eq!(py_to_original, expected_py_to_original);
    }

    #[test]
    fn conversion_arms_only_unit_variants() {
        let original = parse_quote! {
            enum MyEnum {
                Foo,
                Bar,
                Baz,
            }
        };

        let (original_to_py, py_to_original) =
            conversion_match_arms(&original, &Ident::new("PyMyEnum", Span::call_site()), true);

        let expected_original_to_py = Punctuated::from_iter::<[Arm; _]>([
            parse_quote!(MyEnum::Foo => PyMyEnum::Foo),
            parse_quote!(MyEnum::Bar => PyMyEnum::Bar),
            parse_quote!(MyEnum::Baz => PyMyEnum::Baz),
        ]);

        let expected_py_to_original = Punctuated::from_iter::<[Arm; _]>([
            parse_quote!(PyMyEnum::Foo => MyEnum::Foo),
            parse_quote!(PyMyEnum::Bar => MyEnum::Bar),
            parse_quote!(PyMyEnum::Baz => MyEnum::Baz),
        ]);

        pretty_assertions::assert_eq!(original_to_py, expected_original_to_py);
        pretty_assertions::assert_eq!(py_to_original, expected_py_to_original);
    }

    #[test]
    #[should_panic]
    fn conversion_arms_struct_variant() {
        let original = parse_quote! {
            enum MyEnum {
                Foo {},
            }
        };

        conversion_match_arms(&original, &Ident::new("PyMyEnum", Span::call_site()), false);
    }

    #[test]
    #[should_panic]
    fn conversion_arms_tuple_variant_when_only_unit() {
        let original = parse_quote! {
            enum MyEnum {
                Foo(),
            }
        };

        conversion_match_arms(&original, &Ident::new("PyMyEnum", Span::call_site()), true);
    }
}
