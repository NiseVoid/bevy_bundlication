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
    let mut network_type = Vec::new();
    let mut to_networked = Vec::new();
    let mut from_networked = Vec::new();
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

            if let Some(ref networked_as) = field_info.networked_as {
                let networked_as = networked_as.clone();
                network_type.push(quote! {
                    #networked_as
                });
                to_networked.push(quote! {
                    #networked_as::from_component(tick, map, #var)
                });
                from_networked.push(quote! {
                    comp.to_component(tick, map)
                });
            } else {
                network_type.push(quote! {
                    <#field_type as #import_path::NetworkedComponent>::As
                });
                to_networked.push(quote! {
                    <#field_type as #import_path::NetworkedComponent>::to_networked(#var, tick, map)
                });
                from_networked.push(quote! {
                    <#field_type as #import_path::NetworkedComponent>::from_networked(tick, map, comp)
                });
            }

            if let Some(ref update_with) = field_info.update_with {
                update_component.push(quote! {
                    #update_with(&mut c, #var);
                });
            } else {
                update_component.push(quote! {
                    *c = #var;
                });
            }
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
            #[inline(always)]
            fn send(buffers: &mut #import_path::Buffers, packet_id: u8, tick: #import_path::Tick, channel: u8, rule: #import_path::SendRule, map: &#import_path::IdentifierMap, identifier: #import_path::Identifier, #(#component_var: &#component_type, )*) {
                #(
                    let Ok(#component_var) = #to_networked else {return;};
                )*

                let packet_size = 1 +
                    #(#import_path::bincode::serialized_size(&#component_var).unwrap() +)*
                    #import_path::bincode::serialized_size(&identifier).unwrap();

                let mut buf = buffers.reserve_mut(#import_path::BufferKey::new(channel, rule), packet_size as usize, tick);

                buf.push(packet_id);
                #import_path::bincode::serialize_into(&mut buf, &identifier).unwrap();
                #(
                    #import_path::bincode::serialize_into(&mut buf, &#component_var).unwrap();
                )*
            }

            fn send_changes<const CHANNEL: u8, Method: #import_path::SendMethod>(
                id: #import_path::Identifier,
                owner: Option<&#import_path::Owner>,
                authority: Option<&#import_path::Authority>,
                entity: &bevy::ecs::world::EntityRef,

                packet_id: u8,

                buffers: &mut #import_path::Buffers,
                map: &#import_path::IdentifierMap,

                tick: #import_path::Tick,
                our_identity: #import_path::Identity,
                new_clients: &[u32],
            ) {
                let Some(mut rule) = Method::should_send(our_identity, authority, owner, id) else {return;};
                #( let #component_var = entity.get_ref::<#component_type>().unwrap(); )*

                let mut changed = #( #component_var.is_changed() || )* false;

                if !changed {
                    let Some(new_rule) = rule.filter_to(new_clients) else {return;};
                    rule = new_rule;
                }

                Self::send(buffers, packet_id, tick, CHANNEL, rule, &map, id, #(&*#component_var, )*);
            }

            fn apply_changes(world: &mut World, ident: #import_path::Identity, tick: #import_path::Tick, mut cursor: &mut std::io::Cursor<&[u8]>) {
                let Ok(identifier) = #import_path::bincode::deserialize_from(&mut cursor) else {return;};
                let map = world.resource::<#import_path::IdentifierMap>();
                let entity = match map.get(&identifier, tick) {
                    Ok(#import_path::EntityStatus::Alive(entity)) => Some(*entity),
                    Ok(#import_path::EntityStatus::Despawned(_)) => {return;},
                    Err(e) => {None},
                };

                #(
                    let Ok(comp) = #import_path::bincode::deserialize_from::<_, #network_type>(&mut cursor) else {return;};
                    let Ok(#component_var): #import_path::IdentifierResult<#component_type> = #from_networked else {return;};
                )*

                if let Some(mut entity) = entity.and_then(|entity| world.get_entity_mut(entity)) {
                    let bundle_tick = entity.get::<#import_path::LastUpdate<Self>>().map(|t| **t).unwrap_or_default();
                    if bundle_tick >= tick {
                        return;
                    }

                    let auth = entity.get::<#import_path::Authority>().map(|a| *a).unwrap_or_default();
                    if let #import_path::Identity::Client(client_id) = ident {
                        if !auth.can_claim(client_id) {
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
                            Some(mut remote) => {remote.update(#component_var, tick)},
                            None => {
                                match entity.get_mut::<#component_type>() {
                                    Some(mut c) => {#update_component}
                                    None => {entity.insert(#component_var);}
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
                } else if ident == #import_path::Identity::Server {
                    let entity = world.spawn((
                        identifier,
                        #import_path::LastUpdate::<Self>::new(tick),
                        #import_path::LastUpdate::<()>::new(tick),
                        #(
                            #component_var,
                        )*
                        #(
                            #spawn_only_type::default(),
                        )*
                    )).id();
                    let mut map = world.resource_mut::<#import_path::IdentifierMap>();
                    map.insert(identifier, entity);
                }
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

            fn handler() -> #import_path::ApplyChangeFn {
                Self::apply_changes
            }
        }
    })
}
