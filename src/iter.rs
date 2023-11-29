use super::{
    buffer::{RecipientData, SendRule, WriteBuffer},
    Authority, Buffers, Connections, Direction, Identifier, IdentifierMap, Identity, Owner, Packet,
    RegisteredBundle, RegistryDir, Tick,
};

use bevy::{
    ecs::{
        archetype::{ArchetypeGeneration, ArchetypeId, Archetypes},
        component::ComponentId,
    },
    prelude::*,
};

pub fn iterate_world<Dir: Direction>(world: &mut World) {
    let connections = world.remove_resource::<Connections>().unwrap();
    if connections.is_empty() {
        world.insert_resource(connections);
        return;
    }
    let new_clients = connections.iter().any(|c| c.new);

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

    update_archetype_cache::<Dir>(world, archetypes, cache.generation, &mut cache);
    cache.generation = archetypes.generation();

    let mut taken = buffers.take(
        tick,
        0, // TODO: Allow configuration of channel that gets entities
        this_run,
        connections.iter().map(|i| {
            (
                i.ident,
                RecipientData {
                    last_ack: if i.new { None } else { Some(last_run) },
                },
            )
        }),
    );

    for entry in cache.list.iter() {
        let archetype = archetypes.get(entry.archetype).unwrap();

        for entity in archetype.entities().iter() {
            let entity = world.entity(entity.entity());
            let id = entity.get::<Identifier>().unwrap();
            let owner = match entry.has_owner {
                true => entity.get::<Owner>(),
                false => None,
            };
            let authority = match entry.has_authority {
                true => entity.get::<Authority>(),
                false => None,
            };

            if !new_clients
                && !entry.components.iter().any(|c| {
                    entity
                        .get_change_ticks_by_id(*c)
                        .unwrap()
                        .is_changed(last_run, this_run)
                })
            {
                continue;
            }

            taken.overhead(1 + 1 + 4 + 1);
            buf.push(Packet::ENTITY);
            buf.push(id.entity_type);
            buf.extend_from_slice(&id.id.to_le_bytes());
            taken.send(SendRule::All, &mut buf);
            for bundle in entry.bundles.iter() {
                (bundle.serialize)(
                    *id,
                    owner,
                    authority,
                    &entity,
                    ident,
                    bundle.packet_id,
                    &mut taken,
                    &mut buf,
                    &id_map,
                    tick,
                    last_run,
                    this_run,
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
    has_authority: bool,
    components: Vec<ComponentId>,
    bundles: Vec<RegisteredBundle>,
}

fn update_archetype_cache<Dir: Direction>(
    world: &World,
    archetypes: &Archetypes,
    since: ArchetypeGeneration,
    cache: &mut ArchetypeCache,
) {
    let Some(marker_id) = world.component_id::<Identifier>() else {
        return;
    };
    let owner_id = world.component_id::<Owner>();
    let authority_id = world.component_id::<Authority>();
    let reg = world.resource::<RegistryDir<Dir>>();

    for archetype in &archetypes[since..] {
        if !archetype.contains(marker_id) {
            continue;
        }

        let mut bundles = Vec::with_capacity(5);
        let mut components = Vec::with_capacity(20);
        'bundle_loop: for (_, bundle) in reg.bundles.iter() {
            for comp in bundle.component_ids.iter() {
                if !archetype.contains(*comp) {
                    continue 'bundle_loop;
                }
            }
            for comp in bundle.component_ids.iter() {
                if !components.contains(comp) {
                    components.push(*comp);
                }
            }
            bundles.push(bundle.clone());
        }

        if bundles.is_empty() {
            continue;
        }

        cache.list.push(ArchetypeCacheEntry {
            archetype: archetype.id(),
            has_owner: owner_id.is_some() && archetype.contains(owner_id.unwrap()),
            has_authority: authority_id.is_some() && archetype.contains(authority_id.unwrap()),
            components,
            bundles,
        });
    }
}
