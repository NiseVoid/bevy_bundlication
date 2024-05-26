use bevy_macro_utils::get_struct_fields;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

fn import_path() -> syn::Path {
    syn::parse(
        "bevy_bundlication::macro_export"
            .parse::<TokenStream>()
            .unwrap(),
    )
    .unwrap()
}

struct BundleAttributes {
    priority: Option<proc_macro2::Literal>,
}

impl Default for BundleAttributes {
    fn default() -> Self {
        Self { priority: None }
    }
}

impl syn::parse::Parser for BundleAttributes {
    type Output = Self;

    fn parse2(mut self, tokens: proc_macro2::TokenStream) -> syn::Result<Self::Output> {
        let mut token_iter = tokens.into_iter();
        while let Some(token) = token_iter.next() {
            match token {
                proc_macro2::TokenTree::Ident(ident) => {
                    if ident == BUNDLICATION_ATTRIBUTE_PRIORITY_NAME {
                        self.priority = Some(parse_literal(&mut token_iter, ident)?);
                    } else {
                        return Err(syn::Error::new(ident.span(), "unknown ident"));
                    }
                }
                proc_macro2::TokenTree::Punct(punct) => {
                    return Err(syn::Error::new(punct.span(), "unexpected punctuation"));
                }
                proc_macro2::TokenTree::Group(group) => {
                    return Err(syn::Error::new(group.span(), "unexpected group"));
                }
                proc_macro2::TokenTree::Literal(lit) => {
                    return Err(syn::Error::new(lit.span(), "unexpected literal"));
                }
            }

            if let Some(token) = token_iter.next() {
                let proc_macro2::TokenTree::Punct(punct) = token else {
                    return Err(syn::Error::new(token.span(), "expected ,"));
                };
                if punct.as_char() != ',' {
                    return Err(syn::Error::new(punct.span(), "expected ,"));
                }
            }
        }

        Ok(self)
    }
}

struct BundleField {
    skip: bool,
    send: bool,
    networked_as: Option<syn::Ident>,
    update_with: Option<syn::Ident>,
}

impl Default for BundleField {
    fn default() -> Self {
        Self {
            skip: false,
            send: true,
            networked_as: None,
            update_with: None,
        }
    }
}

fn parse_literal(
    token_iter: &mut impl Iterator<Item = proc_macro2::TokenTree>,
    ident: proc_macro2::Ident,
) -> syn::Result<proc_macro2::Literal> {
    // Parse in format " = lit"
    let Some(next) = token_iter.next() else {
        return Err(syn::Error::new(
            ident.span(),
            "expected to be followed by =",
        ));
    };
    let proc_macro2::TokenTree::Punct(punct) = next else {
        return Err(syn::Error::new(next.span(), "expected ="));
    };
    if punct.as_char() != '=' {
        return Err(syn::Error::new(punct.span(), "expected ="));
    }
    let Some(next) = token_iter.next() else {
        return Err(syn::Error::new(
            punct.span(),
            "expected to be followed by literal",
        ));
    };
    let proc_macro2::TokenTree::Literal(lit) = next else {
        return Err(syn::Error::new(next.span(), "expected literal"));
    };

    Ok(lit)
}

fn parse_ident(
    token_iter: &mut impl Iterator<Item = proc_macro2::TokenTree>,
    ident: proc_macro2::Ident,
) -> syn::Result<syn::Ident> {
    // Parse in format " = ident"
    let Some(next) = token_iter.next() else {
        return Err(syn::Error::new(
            ident.span(),
            "expected to be followed by =",
        ));
    };
    let proc_macro2::TokenTree::Punct(punct) = next else {
        return Err(syn::Error::new(next.span(), "expected ="));
    };
    if punct.as_char() != '=' {
        return Err(syn::Error::new(punct.span(), "expected ="));
    }
    let Some(next) = token_iter.next() else {
        return Err(syn::Error::new(
            punct.span(),
            "expected to be followed by ident",
        ));
    };
    let proc_macro2::TokenTree::Ident(ident) = next else {
        return Err(syn::Error::new(next.span(), "expected ident"));
    };

    Ok(ident)
}

impl syn::parse::Parser for BundleField {
    type Output = Self;

    fn parse2(mut self, tokens: proc_macro2::TokenStream) -> syn::Result<Self::Output> {
        let mut token_iter = tokens.into_iter();
        while let Some(token) = token_iter.next() {
            match token {
                proc_macro2::TokenTree::Ident(ident) => {
                    if ident == BUNDLICATION_ATTRIBUTE_SKIP_NAME {
                        self.skip = true;
                    } else if ident == BUNDLICATION_ATTRIBUTE_NO_SEND_NAME {
                        self.send = false;
                    } else if ident == BUNDLICATION_ATTRIBUTE_AS_NAME {
                        self.networked_as = Some(parse_ident(&mut token_iter, ident)?);
                    } else if ident == BUNDLICATION_ATTRIBUTE_UPDATE_NAME {
                        self.update_with = Some(parse_ident(&mut token_iter, ident)?);
                    } else {
                        return Err(syn::Error::new(ident.span(), "unknown ident"));
                    }
                }
                proc_macro2::TokenTree::Punct(punct) => {
                    return Err(syn::Error::new(punct.span(), "unexpected punctuation"));
                }
                proc_macro2::TokenTree::Group(group) => {
                    return Err(syn::Error::new(group.span(), "unexpected group"));
                }
                proc_macro2::TokenTree::Literal(lit) => {
                    return Err(syn::Error::new(lit.span(), "unexpected literal"));
                }
            }

            if let Some(token) = token_iter.next() {
                let proc_macro2::TokenTree::Punct(punct) = token else {
                    return Err(syn::Error::new(token.span(), "expected ,"));
                };
                if punct.as_char() != ',' {
                    return Err(syn::Error::new(punct.span(), "expected ,"));
                }
            }
        }

        Ok(self)
    }
}

const BUNDLICATION_ATTRIBUTE_NAME: &str = "bundlication";
const BUNDLICATION_ATTRIBUTE_PRIORITY_NAME: &str = "priority";
const BUNDLICATION_ATTRIBUTE_SKIP_NAME: &str = "skip";
const BUNDLICATION_ATTRIBUTE_NO_SEND_NAME: &str = "no_send";
const BUNDLICATION_ATTRIBUTE_AS_NAME: &str = "as";
const BUNDLICATION_ATTRIBUTE_UPDATE_NAME: &str = "update";

// TODO: Add option for alternative default function for non-sent fields

#[proc_macro_derive(NetworkedBundle, attributes(bundlication))]
pub fn derive_bundle(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let import_path = import_path();

    let mut attributes = BundleAttributes::default();
    for attr in ast
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident(BUNDLICATION_ATTRIBUTE_NAME))
    {
        match attr.parse_args_with(attributes) {
            Ok(new_attributes) => attributes = new_attributes,
            Err(e) => {
                return e.into_compile_error().into();
            }
        }
    }

    let named_fields = match get_struct_fields(&ast.data) {
        Ok(fields) => fields,
        Err(e) => return e.into_compile_error().into(),
    };

    let mut field_info = Vec::with_capacity(named_fields.len());

    for field in named_fields.iter() {
        let mut bundle_field = BundleField::default();
        for attr in field
            .attrs
            .iter()
            .filter(|a| a.path().is_ident(BUNDLICATION_ATTRIBUTE_NAME))
        {
            match attr.parse_args_with(bundle_field) {
                Ok(new_field) => bundle_field = new_field,
                Err(e) => {
                    return e.into_compile_error().into();
                }
            }
        }
        field_info.push(bundle_field);
    }

    let field = named_fields
        .iter()
        .map(|field| field.ident.as_ref().unwrap())
        .collect::<Vec<_>>();
    let field_type = named_fields
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();

    let mut component_type = Vec::new();
    let mut component_var = Vec::new();
    let mut component_serialize = Vec::new();
    let mut component_deserialize_new = Vec::new();
    let mut component_deserialize_in_place = Vec::new();
    let mut component_info = Vec::new();
    let mut write_component = Vec::new();
    let mut new_component = Vec::new();
    let mut update_component = Vec::new();

    for ((field_type, field_info), field) in
        field_type.iter().zip(field_info.iter()).zip(field.iter())
    {
        if field_info.skip {
            continue;
        }

        component_type.push(quote! {
            #field_type
        });
        let var = syn::Ident::new(&(String::from("field_") + &field.to_string()), field.span());
        component_var.push(quote! {
            #var
        });
        let info = syn::Ident::new(&(String::from("info_") + &field.to_string()), field.span());
        component_info.push(quote! {
            #info
        });

        let serialize = syn::Ident::new(
            &(String::from("__serialize_") + &field.to_string()),
            field.span(),
        );
        component_serialize.push(quote! {
            #serialize
        });
        let deserialize_new = syn::Ident::new(
            &(String::from("__deserialize_new_") + &field.to_string()),
            field.span(),
        );
        component_deserialize_new.push(quote! {
            #deserialize_new
        });
        let deserialize_in_place = syn::Ident::new(
            &(String::from("__deserialize_in_place_") + &field.to_string()),
            field.span(),
        );
        component_deserialize_in_place.push(quote! {
            #deserialize_in_place
        });

        if field_info.send {
            let new;
            if let Some(ref networked_as) = field_info.networked_as {
                let networked_as = networked_as.clone();
                write_component.push(quote! {
                    <#networked_as as #import_path::NetworkedWrapper<#field_type>>::write_data(&#var, &mut cursor, ctx)?
                });
                new = quote! {
                    <#networked_as as #import_path::NetworkedWrapper<#field_type>>::read_new(&mut cursor, ctx)?
                };
            } else {
                write_component.push(quote! {
                    <#field_type as #import_path::NetworkedComponent>
                        ::write_data(&#var, &mut cursor, ctx)?
                });
                new = quote! {
                    <#field_type as #import_path::NetworkedComponent>
                        ::read_new(&mut cursor, ctx)?
                };
            }

            if let Some(ref update_with) = field_info.update_with {
                update_component.push(quote! {
                    #update_with(#var, #new);
                });
            } else if let Some(ref networked_as) = field_info.networked_as {
                let networked_as = networked_as.clone();
                update_component.push(quote! {
                    <#networked_as as #import_path::NetworkedWrapper<#field_type>>::read_in_place(#var, &mut cursor, ctx)?
                });
            } else {
                update_component.push(quote! {
                    <#field_type as #import_path::NetworkedComponent>::read_in_place(#var, &mut cursor, ctx)?
                });
            }
            new_component.push(new);
        } else {
            write_component.push(quote! {_ = #var});
            new_component.push(quote! {#field_type::default()});
            update_component.push(quote! {_ = #var});
        }
    }

    let set_priority = match attributes.priority {
        Some(priority) => quote! {rule.priority = #priority},
        None => quote! {},
    };

    let generics = ast.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let struct_name = &ast.ident;

    TokenStream::from(quote! {
        #[allow(clippy::too_many_arguments, clippy::type_complexity, clippy::needless_question_mark)]
        impl #impl_generics #struct_name #ty_generics #where_clause {#(
            fn #component_serialize(
                ctx: &#import_path::SerializeCtx,
                #component_var: &#component_type,
                mut cursor: &mut #import_path::Cursor<Vec<u8>>,
            ) -> #import_path::bincode::Result<()> {
                #write_component;
                Ok(())
            }

            fn #component_deserialize_new(
                ctx: &mut #import_path::DeserializeCtx,
                mut cursor: &mut #import_path::Cursor<&[u8]>,
            ) -> #import_path::bincode::Result<#component_type> {
                Ok(#new_component)
            }

            fn #component_deserialize_in_place(
                _: #import_path::DeserializeFn<#component_type>,
                ctx: &mut #import_path::DeserializeCtx,
                #component_var: &mut #component_type,
                mut cursor: &mut #import_path::Cursor<&[u8]>,
            ) -> #import_path::bincode::Result<()> {
                #update_component;
                Ok(())
            }
        )*}

        #[allow(clippy::too_many_arguments, clippy::type_complexity)]
        impl #impl_generics #import_path::GroupReplication for #struct_name #ty_generics #where_clause {
            fn register(
                world: &mut #import_path::World,
                replication_fns: &mut #import_path::ReplicationRegistry
            ) -> #import_path::ReplicationRule {
                #(
                    let #component_info = replication_fns.register_rule_fns(
                        world,
                        #import_path::RuleFns::new(Self::#component_serialize, Self::#component_deserialize_new)
                            .with_in_place(Self::#component_deserialize_in_place),
                    );
                )*

                let mut rule = #import_path::ReplicationRule::new(vec![#(#component_info, )*]);
                #set_priority;
                rule
            }
        }
    })
}
