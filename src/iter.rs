use super::{
    Authority, Buffers, Direction, Identifier, IdentifierMap, Identity, Owner, RegisteredBundle,
    RegistryDir, Tick,
};

use bevy::{
    ecs::archetype::{ArchetypeGeneration, ArchetypeId, Archetypes},
    prelude::*,
};

pub fn iterate_world<Dir: Direction>(world: &mut World) {
    let mut new_clients = Vec::with_capacity(20);

    if let Some(mut events) = world.get_resource_mut::<Events<super::NewConnection>>() {
        new_clients.extend(events.drain().filter_map(|e| {
            let Identity::Client(client_id) = *e else {
                return None;
            };
            Some(client_id)
        }));
    }

    let Some(mut buffers) = world.remove_resource::<Buffers>() else {
        return;
    };
    let tick = world.remove_resource::<Tick>().unwrap();
    let id_map = world.remove_resource::<IdentifierMap>().unwrap();
    let ident = world.remove_resource::<Identity>().unwrap();

    let mut cache = world
        .remove_resource::<ArchetypeCache>()
        .unwrap_or_default();

    let archetypes = world.archetypes();

    update_archetype_cache::<Dir>(world, archetypes, cache.generation, &mut cache);
    cache.generation = archetypes.generation();

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

            for bundle in entry.bundles.iter() {
                (bundle.serialize)(
                    *id,
                    owner,
                    authority,
                    &entity,
                    bundle.packet_id,
                    &mut buffers,
                    &id_map,
                    tick,
                    ident,
                    &new_clients,
                );
            }
        }
    }

    world.insert_resource(cache);
    world.insert_resource(ident);
    world.insert_resource(id_map);
    world.insert_resource(tick);
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
        'bundle_loop: for (_, bundle) in reg.bundles.iter() {
            for comp in bundle.component_ids.iter() {
                if !archetype.contains(*comp) {
                    continue 'bundle_loop;
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
            bundles,
        });
    }
}
