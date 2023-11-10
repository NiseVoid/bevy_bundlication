use crate::Identifier;

use bevy::prelude::*;

/// A component tracking who has authority over the enity, if it is not present it behaves as if it
/// was [Authority::Server]
#[derive(Component, Clone, Copy, PartialEq, Eq, Default, Debug)]
#[repr(u8)]
pub enum Authority {
    /// The server holds authority, this is the default
    #[default]
    Server,
    /// The authority is free for anyone to claim
    Free,
    /// Authority is held by a specific client
    Client(u32),
}

impl Authority {
    /// Check if the client can claim authority
    #[inline(always)]
    pub fn can_claim(&self, client_id: u32) -> bool {
        use Authority::*;
        match *self {
            Server => false,
            Free => true,
            Client(owner) => owner == client_id,
        }
    }
}

/// The identity of a connection
#[repr(u8)]
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Identity {
    /// The identity is the server
    Server,
    /// The identity is a client with the specified id
    Client(u32),
}

impl Identity {
    /// Check if an entity can be sent based on our local [Identity] and the entity's [Authority]
    #[inline(always)]
    pub fn can_send(self, auth: Option<&Authority>) -> bool {
        match self {
            Identity::Server => true,
            Identity::Client(id) => auth == Some(&Authority::Client(id)),
        }
    }

    /// Get the [Identifier] for the [Identity], panics if the value is [Identity::Server]
    pub fn as_identifier(&self) -> Identifier {
        let Identity::Client(client_id) = self else {
            panic!("Cannot call as_identifier on Identity::Server")
        };
        Identifier::new(0, *client_id)
    }
}

/// A [HashSet] keeping track of which entities we hold [Authority] for. Only updated for clients,
/// as the server always has authority to modify things
#[derive(Resource, Deref, DerefMut, Default)]
pub struct HeldAuthority(bevy::utils::HashSet<Entity>);

pub fn track_authority(
    query: Query<(Entity, &Authority), Changed<Authority>>,
    mut removed: RemovedComponents<Authority>,
    mut held: ResMut<HeldAuthority>,
    our_ident: Res<Identity>,
) {
    let Identity::Client(client_id) = *our_ident else {
        return;
    };
    for (entity, auth) in query.iter() {
        if auth.can_claim(client_id) {
            held.insert(entity);
        } else {
            held.remove(&entity);
        }
    }
    for entity in removed.read() {
        held.remove(&entity);
    }
}
