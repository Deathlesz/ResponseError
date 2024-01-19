extern crate proc_macro;

use darling::FromVariant;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned};
use syn::{parse_macro_input, spanned::Spanned as _, Data, DataEnum, DeriveInput, Fields};

#[derive(Default, FromVariant)]
#[darling(default, attributes(response))]
struct Attributes {
    status_code: Option<u16>,
    error_code: Option<String>,
    transparent: bool,
}

#[proc_macro_derive(ResponseError, attributes(response))]
pub fn derive(input: TokenStream) -> TokenStream {
    let DeriveInput {
        ident: name,
        data,
        generics,
        ..
    } = parse_macro_input!(input);

    let mut impl_expr = TokenStream2::new();

    match data {
        Data::Enum(data) => derive_enum(data, &mut impl_expr),
        _ => {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "ResponseError should be derived only on enums",
            )
            .to_compile_error()
            .into()
        }
    }

    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    let expanded = quote! {
        impl axum::response::IntoResponse for #impl_generics #name #type_generics #where_clause {
            #impl_expr
        }
    };

    TokenStream::from(expanded)
}

fn derive_enum(data: DataEnum, impl_expr: &mut TokenStream2) {
    let mut match_expr = TokenStream2::new();

    for variant in data.variants {
        let variant_name = &variant.ident;

        let attributes = Attributes::from_variant(&variant).unwrap_or_default();
        let fields = match variant.fields {
            Fields::Unit => quote_spanned!( variant.span()=> ),
            Fields::Unnamed(ref fields) if fields.unnamed.len() == 1 && attributes.transparent => {
                quote_spanned!( variant.span()=> (error))
            }
            Fields::Unnamed(_) => quote_spanned!( variant.span()=> (..)),
            Fields::Named(_) => quote_spanned!( variant.span()=> {..}),
        };

        // This is ugly but I'm too lazy to do it properly.
        let response = if !attributes.transparent {
            match (attributes.status_code, attributes.error_code) {
                (None, None) => quote_spanned!( variant.span()=> "-1".into_response()),
                (None, Some(error_code)) => {
                    quote_spanned!( variant.span()=> #error_code.into_response())
                }
                (Some(status_code), None) => {
                    quote_spanned!( variant.span()=> axum::http::StatusCode::from_u16(#status_code).expect("invalid status_code").into_response())
                }
                (Some(status_code), Some(error_code)) => {
                    quote_spanned!( variant.span()=> (axum::http::StatusCode::from_u16(#status_code).expect("invalid status_code"), #error_code).into_response())
                }
            }
        } else {
            quote_spanned!( variant.span()=> error.into_response())
        };

        match_expr.extend(quote_spanned! { variant.span()=>
            Self::#variant_name #fields => #response,
        });
    }

    *impl_expr = quote! {
        fn into_response(self) -> axum::response::Response {
            match self {
                #match_expr
            }
        }
    }
}
