use crate::{
    buffer::{RecipientData, WriteBuffer},
    client_authority::{HeldAuthority, Identity},
    Authority, Buffers, Connections, EntityStatus, Identifier, IdentifierMap, LastUpdate, SendRule,
    Tick,
};

use bevy::{ecs::system::Command, prelude::*};

/// The channel on which despawn messages are sent
#[derive(Resource, Deref)]
pub struct DespawnChannel(pub u8);

pub(crate) fn send_despawns(
    mut removed: RemovedComponents<Identifier>,
    mut map: ResMut<IdentifierMap>,
    mut buffers: ResMut<Buffers>,
    connections: Res<Connections>,
    held: Res<HeldAuthority>,
    our_ident: Res<Identity>,
    tick: Res<Tick>,
    mut buf: ResMut<WriteBuffer>,
    despawn_channel: Res<DespawnChannel>,
) {
    for entity in removed.read() {
        let Some(ident) = map.remove_entity(&entity) else {
            continue;
        };
        if *our_ident != Identity::Server && !held.contains(&entity) {
            continue;
        }

        let mut buffer = buffers.take(
            *tick,
            **despawn_channel,
            bevy::ecs::component::Tick::new(0),
            connections
                .iter()
                .map(|i| (i.ident, RecipientData::default())),
        );

        buf.push(0);
        bincode::serialize_into(&mut *buf, &ident).unwrap();

        buffer.send(SendRule::All, &mut buf);
        buffer.fragment();
    }
}

pub(crate) fn handle_despawns(
    world: &mut World,
    ident: Identity,
    tick: Tick,
    cursor: &mut std::io::Cursor<&[u8]>,
) {
    let Ok(identifier) = bincode::deserialize_from(cursor) else {
        return;
    };

    let map = world.resource::<IdentifierMap>();
    let Ok(EntityStatus::Alive(entity)) = map.get(&identifier, tick) else {
        return;
    };
    let entity = *entity;
    if let Identity::Client(client_id) = ident {
        if !world
            .entity(entity)
            .get::<Authority>()
            .cloned()
            .unwrap_or_default()
            .can_claim(client_id)
        {
            return;
        }
    }
    if tick
        < world
            .entity(entity)
            .get::<LastUpdate<()>>()
            .map(|t| **t)
            .unwrap_or_default()
    {
        return;
    }

    DespawnRecursive { entity }.apply(world);
    let mut map = world.resource_mut::<IdentifierMap>();
    map.despawn(&identifier, &entity, tick);
}
