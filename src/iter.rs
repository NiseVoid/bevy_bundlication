use super::{
    buffer::{RecipientData, SendRule, WriteBuffer},
    Authority, Buffers, Connections, Direction, Identifier, IdentifierMap, Identity, Owner, Packet,
    RegisteredBundle, RegistryDir, Tick,
};

use bevy::{
    ecs::archetype::{ArchetypeGeneration, ArchetypeId, Archetypes},
    prelude::*,
};

pub fn iterate_world<Dir: Direction>(world: &mut World) {
    let connections = world.remove_resource::<Connections>().unwrap();
    if connections.is_empty() {
        world.insert_resource(connections);
        return;
    }
    let new_clients = connections.iter().any(|c| c.new && c.replicate);

    let mut buffers = world.remove_resource::<Buffers>().unwrap();
    let mut buf = world.remove_resource::<WriteBuffer>().unwrap();

    let tick = world.remove_resource::<Tick>().unwrap();
    let id_map = world.remove_resource::<IdentifierMap>().unwrap();
    let ident = world.remove_resource::<Identity>().unwrap();

    let mut cache = world
        .remove_resource::<ArchetypeCache>()
        .unwrap_or_default();

    let last_run = world.last_change_tick();
    let this_run = world.change_tick();
    let archetypes = world.archetypes();

    update_archetype_cache::<Dir>(ident, world, archetypes, cache.generation, &mut cache);
    cache.generation = archetypes.generation();

    let mut taken = buffers.take(
        tick,
        0, // TODO: Allow configuration of channel that gets entities
        this_run,
        connections.iter().filter_map(|i| {
            if !i.replicate {
                return None;
            }
            Some((
                i.ident,
                RecipientData {
                    last_ack: if i.new { None } else { Some(last_run) },
                },
            ))
        }),
    );

    for entry in cache.list.iter_mut() {
        let archetype = archetypes.get(entry.archetype).unwrap();

        for entity in archetype.entities().iter() {
            let entity = world.entity(entity.entity());
            if let Identity::Client(client_id) = ident {
                if let Some(auth) = entity.get::<Authority>() {
                    if !auth.can_claim(client_id) {
                        continue;
                    }
                }
            }

            let mut changed = last_run;
            for (bundle, last_changed) in entry.bundles.iter().zip(entry.last_changed.iter_mut()) {
                *last_changed = last_run;
                for &id in bundle.component_ids.iter() {
                    let change = entity.get_change_ticks_by_id(id).unwrap();
                    if change.is_changed(*last_changed, this_run) {
                        *last_changed = change.last_changed_tick();
                        if change.is_changed(changed, this_run) {
                            changed = change.last_changed_tick();
                        }
                    }
                }
            }

            if !new_clients && changed == last_run {
                continue;
            }

            let id = entity.get::<Identifier>().unwrap();
            let owner = match entry.has_owner {
                true => Some(**entity.get::<Owner>().unwrap()),
                false => {
                    if id.is_client() {
                        Some(id.id)
                    } else {
                        None
                    }
                }
            };

            taken.overhead(1 + 1 + 4 + 1);
            buf.push(Packet::ENTITY);
            buf.push(id.entity_type);
            buf.extend_from_slice(&id.id.to_le_bytes());
            taken.send(SendRule::All, &mut buf);
            for (bundle, &changed) in entry.bundles.iter().zip(entry.last_changed.iter()) {
                if !new_clients && changed == last_run {
                    continue;
                }
                (bundle.serialize)(
                    &bundle.component_ids,
                    owner,
                    entity,
                    bundle.packet_id,
                    &mut taken,
                    &mut buf,
                    &id_map,
                    tick,
                    changed,
                );
            }
            buf.push(0);
            taken.send(SendRule::All, &mut buf);
            taken.fragment();
        }
    }

    drop(taken);
    world.insert_resource(cache);
    world.insert_resource(ident);
    world.insert_resource(id_map);
    world.insert_resource(tick);
    world.insert_resource(connections);
    world.insert_resource(buf);
    world.insert_resource(buffers);
}

#[derive(Resource)]
pub struct ArchetypeCache {
    list: Vec<ArchetypeCacheEntry>,
    generation: ArchetypeGeneration,
}

impl Default for ArchetypeCache {
    fn default() -> Self {
        Self {
            list: default(),
            generation: ArchetypeGeneration::initial(),
        }
    }
}

pub struct ArchetypeCacheEntry {
    archetype: ArchetypeId,
    has_owner: bool,
    bundles: Vec<RegisteredBundle>,
    last_changed: Vec<bevy::ecs::component::Tick>,
}

fn update_archetype_cache<Dir: Direction>(
    ident: Identity,
    world: &World,
    archetypes: &Archetypes,
    since: ArchetypeGeneration,
    cache: &mut ArchetypeCache,
) {
    let marker_id = world.component_id::<Identifier>().unwrap();
    let owner_id = world.component_id::<Owner>().unwrap();
    let authority_id = world.component_id::<Authority>().unwrap();
    let reg = world.resource::<RegistryDir<Dir>>();

    for archetype in &archetypes[since..] {
        if !archetype.contains(marker_id) {
            continue;
        }
        if ident != Identity::Server && !archetype.contains(authority_id) {
            // Client can't send bundles without authority so we can skip all of them
            continue;
        }

        let mut bundles = Vec::with_capacity(5);
        let mut last_changed = Vec::with_capacity(5);

        'bundle_loop: for (_, bundle) in reg.bundles.iter() {
            for comp in bundle.component_ids.iter() {
                if !archetype.contains(*comp) {
                    continue 'bundle_loop;
                }
            }
            bundles.push(bundle.clone());
            last_changed.push(bevy::ecs::component::Tick::new(0));
        }

        if bundles.is_empty() {
            continue;
        }

        cache.list.push(ArchetypeCacheEntry {
            archetype: archetype.id(),
            has_owner: archetype.contains(owner_id),
            bundles,
            last_changed,
        });
    }
}
