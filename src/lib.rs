use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemImpl, ItemStruct, LitStr, Type};
use syn::parse::{Parse, ParseStream};

/// Attribute to mark an OS-specific implementation for a trait.
/// Accepts a comma-separated list of OSes (e.g., "windows, linux").
#[proc_macro_attribute]
pub fn os_impl(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as OsArg);
    let os_list = args.os_list.value();
    let oses: Vec<String> = os_list.split(',').map(|s| s.trim().to_string()).collect();
    let impl_block = parse_macro_input!(input as ItemImpl);

    let trait_name = impl_block.trait_.as_ref().unwrap().1.clone();
    let _struct_name = impl_block.self_ty.clone();
    let trait_ident = trait_name.segments.last().unwrap().ident.clone();

    // Generate a unique identifier for this impl group
    let impl_ident = syn::Ident::new(
        &format!("os_impl_{}_{}", trait_ident, oses.join("_")),
        proc_macro2::Span::call_site(),
    );

    // Generate cfg condition for all OSes in the group
    let os_conditions = oses.iter().map(|os| {
        let os_lit = syn::LitStr::new(os, proc_macro2::Span::call_site());
        quote! { target_os = #os_lit }
    });

    let output = quote! {
        #[allow(non_upper_case_globals)]
        pub const #impl_ident: (&str, &[&str]) = (stringify!(#trait_ident), &[#(#oses),*]);
        #[cfg(any(#(#os_conditions),*))]
        #impl_block
    };
    output.into()
}

/// Attribute to enforce OS-specific implementations for a trait on a struct.
/// Syntax: #[enforce_os_support(TraitName("os1, os2, ..."))]
/// Can be applied multiple times for different traits.
#[proc_macro_attribute]
pub fn enforce_os_support(_args: TokenStream, input: TokenStream) -> TokenStream {
    let mut struct_item = parse_macro_input!(input as ItemStruct);
    let struct_name = &struct_item.ident.clone();

    let mut enforce_outputs = Vec::new();
    for attr in struct_item.attrs.iter().filter(|attr| attr.path().is_ident("enforce_os_support")) {
        let args = attr.parse_args::<EnforceArgs>().unwrap();
        let trait_name = args.trait_ty;
        let os_list = args.os_list.value();
        let required_oses: Vec<String> = os_list.split(',').map(|s| s.trim().to_string()).collect();

        let trait_ident = match &trait_name {
            Type::Path(type_path) => type_path.path.segments.last().unwrap().ident.clone(),
            _ => panic!("Trait must be a path type"),
        };

        let registry_path = quote! { include!(concat!(env!("OUT_DIR"), "/os_registry.rs")); };

        let os_conditions = required_oses.iter().map(|os| {
            let os_lit = syn::LitStr::new(os, proc_macro2::Span::call_site());
            quote! { target_os = #os_lit }
        });
        let error_msg = format!(
            "Trait {} must be implemented for one of: {}",
            trait_ident, required_oses.join(", ")
        );

        let validate_block = quote! {
            const #trait_ident: () = {
                let required = &[#(#required_oses),*];
                let implemented: Vec<&str> = OS_IMPLS
                    .iter()
                    .filter(|(t, _, _)| t == &stringify!(#trait_ident))
                    .flat_map(|(_, oses, _)| oses.iter())
                    .copied()
                    .collect();
                for req in required {
                    if !implemented.contains(&req) {
                        panic!(concat!("Missing implementation for OS: ", req, " for trait ", stringify!(#trait_ident)));
                    }
                }
            };
        };

        let enforce_block = quote! {
            #[cfg(not(any(#(#os_conditions),*)))]
            impl #trait_name for #struct_name {
                fn do_something(&self) -> String {
                    compile_error!(#error_msg);
                    unreachable!()
                }
            }
        };

        enforce_outputs.push(quote! {
            #registry_path
            #validate_block
            #enforce_block
        });
    }

    struct_item.attrs.retain(|attr| !attr.path().is_ident("enforce_os_support"));

    let output = quote! {
        #struct_item
        #(#enforce_outputs)*
    };
    output.into()
}

// Helper structs for parsing attribute arguments
struct OsArg {
    os_list: LitStr,
}

impl Parse for OsArg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let os_list = input.parse::<LitStr>()?;
        Ok(OsArg { os_list })
    }
}

struct EnforceArgs {
    trait_ty: Type,
    os_list: LitStr,
}

impl Parse for EnforceArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let trait_ty = input.parse::<Type>()?;
        let content;
        syn::parenthesized!(content in input);
        let os_list = content.parse::<LitStr>()?;
        Ok(EnforceArgs { trait_ty, os_list })
    }
}
