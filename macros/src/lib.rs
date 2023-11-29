use bevy_macro_utils::get_named_struct_fields;
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

struct BundleField {
    skip: bool,
    send: bool,
    optional: bool,
    networked_as: Option<syn::Ident>,
    update_with: Option<syn::Ident>,
}

impl Default for BundleField {
    fn default() -> Self {
        Self {
            skip: false,
            send: true,
            optional: false,
            networked_as: None,
            update_with: None,
        }
    }
}

// TODO: Add some verification to prevent stupid errors about missing field impls

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
                    if ident == NETWORKED_ATTRIBUTE_SKIP_NAME {
                        self.skip = true;
                    } else if ident == NETWORKED_ATTRIBUTE_NO_SEND_NAME {
                        self.send = false;
                    } else if ident == NETWORKED_ATTRIBUTE_OPTIONAL_NAME {
                        self.optional = true;
                    } else if ident == NETWORKED_ATTRIBUTE_AS_NAME {
                        self.networked_as = Some(parse_ident(&mut token_iter, ident)?);
                    } else if ident == NETWORKED_ATTRIBUTE_UPDATE_NAME {
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

const NETWORKED_ATTRIBUTE_NAME: &str = "networked";
const NETWORKED_ATTRIBUTE_SKIP_NAME: &str = "skip";
const NETWORKED_ATTRIBUTE_NO_SEND_NAME: &str = "no_send";
const NETWORKED_ATTRIBUTE_OPTIONAL_NAME: &str = "optional";
const NETWORKED_ATTRIBUTE_AS_NAME: &str = "as";
const NETWORKED_ATTRIBUTE_UPDATE_NAME: &str = "update";

// TODO: Add option for alternative default function for non-networked fields

#[proc_macro_derive(NetworkedBundle, attributes(networked))]
pub fn derive_bundle(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let import_path = import_path();

    let named_fields = match get_named_struct_fields(&ast.data) {
        Ok(fields) => &fields.named,
        Err(e) => return e.into_compile_error().into(),
    };

    let mut field_info = Vec::with_capacity(named_fields.len());

    for field in named_fields.iter() {
        let mut bundle_field = BundleField::default();
        for attr in field
            .attrs
            .iter()
            .filter(|a| a.path().is_ident(NETWORKED_ATTRIBUTE_NAME))
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
    let mut spawn_only_type = Vec::new();
    let mut component_var = Vec::new();
    let mut write_component = Vec::new();
    let mut new_component = Vec::new();
    let mut update_component = Vec::new();
    let mut filters = Vec::new();

    for ((field_type, field_info), field) in
        field_type.iter().zip(field_info.iter()).zip(field.iter())
    {
        if field_info.skip {
            continue;
        }

        if field_info.send {
            // TODO: Handle sending optional fields

            let var = syn::Ident::new(&(String::from("field_") + &field.to_string()), field.span());
            component_var.push(quote! {
                #var
            });
            component_type.push(quote! {
                #field_type
            });

            let new;
            if let Some(ref networked_as) = field_info.networked_as {
                let networked_as = networked_as.clone();
                write_component.push(quote! {
                    #networked_as::write_data(&#var, &mut buffer, tick, id_map).unwrap()
                });
                new = quote! {
                    #networked_as::read_new(&mut buffer, tick, id_map).unwrap()
                };
            } else {
                write_component.push(quote! {
                    <#field_type as #import_path::NetworkedComponent>
                        ::write_data(&#var, &mut buffer, tick, id_map)
                        .unwrap()
                });
                new = quote! {
                    <#field_type as #import_path::NetworkedComponent>
                        ::read_new(&mut buffer, tick, id_map)
                        .unwrap()
                };
            }

            if let Some(ref update_with) = field_info.update_with {
                update_component.push(quote! {
                    #update_with(&mut #var, #new);
                });
            } else if let Some(ref networked_as) = field_info.networked_as {
                let networked_as = networked_as.clone();
                update_component.push(quote! {
                    #networked_as::read_in_place(&mut #var, &mut buffer, tick, id_map).unwrap()
                });
            } else {
                update_component.push(quote! {
                    <#field_type as #import_path::NetworkedComponent>
                        ::read_in_place(&mut #var, &mut buffer, tick, id_map)
                        .unwrap()
                });
            }
            new_component.push(new);
        } else {
            spawn_only_type.push(quote! {
                #field_type
            });

            if !field_info.optional {
                filters.push(quote! {
                    #field_type
                });
            }
        }
    }

    let n_filters = component_type.len() + filters.len();

    let generics = ast.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let struct_name = &ast.ident;

    TokenStream::from(quote! {
        #[allow(clippy::too_many_arguments, clippy::type_complexity)]
        impl #impl_generics #struct_name #ty_generics #where_clause {
            fn send_changes<const CHANNEL: u8, Method: #import_path::SendMethod>(
                id: #import_path::Identifier,
                owner: Option<&#import_path::Owner>,
                authority: Option<&#import_path::Authority>,
                entity: &bevy::ecs::world::EntityRef,

                our_identity: #import_path::Identity,

                packet_id: u8,

                buffers: &mut #import_path::TakenBuffers,
                mut buffer: &mut #import_path::WriteBuffer,
                id_map: &#import_path::IdentifierMap,
                tick: #import_path::Tick,
                last_run: bevy::ecs::component::Tick,
                this_run: bevy::ecs::component::Tick,
            ) {
                let Some(mut rule) = Method::should_send(our_identity, authority, owner, id) else {return;};
                #( let #component_var = entity.get_ref::<#component_type>().unwrap(); )*

                let mut changed = last_run;
                #(
                    let c = #component_var.last_changed();
                    if c.is_newer_than(changed, this_run) {
                        changed = c;
                    }
                )*

                buffer.push(packet_id);
                #( #write_component; )*
                buffers.send_filtered(#import_path::WriteFilters {
                    rule,
                    changed,
                }, buffer);
            }

            fn consume(id_map: &mut #import_path::IdentifierMap, tick: #import_path::Tick, mut buffer: &mut std::io::Cursor<&[u8]>) {
                #(
                    _ = #new_component;
                )*
            }

            fn spawn(
                entity: &mut bevy::ecs::world::EntityWorldMut,
                id_map: &mut #import_path::IdentifierMap,
                ident: #import_path::Identity,
                tick: #import_path::Tick,
                mut buffer: &mut std::io::Cursor<&[u8]>
            ) {
                entity.insert((
                    #import_path::LastUpdate::<Self>::new(tick),
                    #(
                        #new_component,
                    )*
                    #(
                        #spawn_only_type::default(),
                    )*
                ));
            }

            fn apply_changes(
                entity: &mut bevy::ecs::world::EntityWorldMut,
                id_map: &mut #import_path::IdentifierMap,
                ident: #import_path::Identity,
                tick: #import_path::Tick,
                mut buffer: &mut std::io::Cursor<&[u8]>
            ) {
                let bundle_tick = entity.get::<#import_path::LastUpdate<Self>>().map(|t| **t).unwrap_or_default();
                if bundle_tick >= tick {
                    Self::consume(id_map, tick, buffer);
                    return;
                }

                let auth = entity.get::<#import_path::Authority>().map(|a| *a).unwrap_or_default();
                if let #import_path::Identity::Client(client_id) = ident {
                    if !auth.can_claim(client_id) {
                        Self::consume(id_map, tick, buffer);
                        return;
                    }
                    entity.insert(#import_path::Authority::Client(client_id));
                }
                entity.insert(#import_path::LastUpdate::<Self>::new(tick));
                let entity_tick = entity.get::<#import_path::LastUpdate<()>>().map(|t| **t).unwrap_or_default();
                if tick > entity_tick {
                    entity.insert(#import_path::LastUpdate::<()>::new(tick));
                }

                #(
                    match entity.get_mut::<#import_path::Remote<#component_type>>() {
                        Some(mut remote) => {
                            let mut #component_var = remote.update(tick);
                            #update_component;
                        },
                        None => {
                            match entity.get_mut::<#component_type>() {
                                Some(mut #component_var) => {#update_component}
                                None => {
                                    entity.insert(#new_component);
                                }
                            }
                        }
                    }
                )*
                #(
                    match entity.get::<#spawn_only_type>() {
                        Some(_) => {}
                        None => {entity.insert(#spawn_only_type::default());}
                    }
                )*
            }
        }

        impl #impl_generics #import_path::NetworkedBundle for #struct_name #ty_generics #where_clause {
            fn get_component_ids(world: &mut World) -> Vec<bevy::ecs::component::ComponentId> {
                let mut list = Vec::with_capacity(#n_filters);
                #( list.push(world.init_component::<#component_type>()); )*
                #( list.push(world.init_component::<#filters>()); )*
                list
            }

            fn serializer<const CHANNEL: u8, Method: #import_path::SendMethod>() -> #import_path::SendChangeFn {
                Self::send_changes::<CHANNEL, Method>
            }

            fn updater() -> #import_path::ApplyEntityChangeFn {
                Self::apply_changes
            }

            fn spawner() -> #import_path::ApplyEntityChangeFn {
                Self::spawn
            }

            fn consumer() -> #import_path::ConsumeFn {
                Self::consume
            }
        }
    })
}
