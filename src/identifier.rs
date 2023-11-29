use crate::Tick;

use bevy::{
    ecs::system::{Command, EntityCommands},
    prelude::*,
};
use serde::{Deserialize, Serialize};

/// A component to override the owner of an entity. When this is present the provided client_id is
/// used instead of the Identifier
#[derive(Component, Clone, Copy, Debug, Deref, PartialEq, Eq)]
pub struct Owner(pub u32);

/// This component keeps track of what this entity is, the values get synced across all
/// clients/servers. For example you could have entity type 2 for enemies, and it is the 8th enemy to be spawned so it gets id 8.
/// entity_type 0 is special and reserved for players, the id needs to match with the client
/// ids from renet
#[derive(
    Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Identifier {
    /// The type of the entity
    pub(crate) entity_type: u8,
    /// The ID within this type
    pub(crate) id: u32,
}

impl Identifier {
    /// Check if the [Identifier] is a client
    #[inline(always)]
    pub fn is_client(&self) -> bool {
        self.entity_type == 0
    }

    /// Get the type of the [Identifier]
    pub fn entity_type(&self) -> u8 {
        self.entity_type
    }

    /// Get the ID of the [Identifier]
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Construct an [Identifier] from an entity type and ID
    pub fn new(entity_type: impl Into<u8>, id: u32) -> Self {
        Self {
            entity_type: entity_type.into(),
            id,
        }
    }
}

/// An extention trait for Commands to spawn entities with an Identifier
pub trait CommandsSpawnIdentifierExt<'a, 'w, 's> {
    /// Spawn an entity with a client identifier
    fn spawn_client(
        &'a mut self,
        client_id: u32,
        bundle: impl Bundle,
    ) -> EntityCommands<'w, 's, 'a>;

    /// Spawn an entity with an identifier
    fn spawn_with_id(
        &'a mut self,
        id_type: impl Into<u8>,
        id: u32,
        bundle: impl Bundle,
    ) -> EntityCommands<'w, 's, 'a>;
}

impl<'a, 'w, 's> CommandsSpawnIdentifierExt<'a, 'w, 's> for Commands<'w, 's> {
    #[inline(always)]
    fn spawn_client(
        &'a mut self,
        client_id: u32,
        bundle: impl Bundle,
    ) -> EntityCommands<'w, 's, 'a> {
        self.spawn_with_id(0, client_id, bundle)
    }

    #[inline(always)]
    fn spawn_with_id(
        &'a mut self,
        id_type: impl Into<u8>,
        id: u32,
        bundle: impl Bundle,
    ) -> EntityCommands<'w, 's, 'a> {
        let id = Identifier::new(id_type, id);
        let entity = self.spawn((id, bundle)).id();

        self.add(SpawnIdentifierCommand { id, entity });
        self.entity(entity)
    }
}

/// An extention trait for Commands to spawn entities with an Identifier
pub trait EntityCommandsInsertIdentifierExt {
    /// Spawn an entity with a client identifier
    fn insert_client(&mut self, client_id: u32) -> &mut Self;

    /// Spawn an entity with an identifier
    fn insert_id(&mut self, id_type: impl Into<u8>, id: u32) -> &mut Self;
}

impl EntityCommandsInsertIdentifierExt for EntityCommands<'_, '_, '_> {
    #[inline(always)]
    fn insert_client(&mut self, client_id: u32) -> &mut Self {
        self.insert_id(0, client_id)
    }

    #[inline(always)]
    fn insert_id(&mut self, id_type: impl Into<u8>, id: u32) -> &mut Self {
        let id = Identifier::new(id_type, id);
        self.insert(id);

        let entity = self.id();
        self.commands().add(SpawnIdentifierCommand { id, entity });

        self
    }
}

/// An extention trait for World to spawn entities with an Identifier
pub trait WorldSpawnIdentifierExt<'w> {
    /// Spawn an entity with a client identifier
    fn spawn_client(&'w mut self, client_id: u32, bundle: impl Bundle) -> EntityWorldMut<'w>;

    /// Spawn an entity with an identifier
    fn spawn_with_id(
        &'w mut self,
        id_type: impl Into<u8>,
        id: u32,
        bundle: impl Bundle,
    ) -> EntityWorldMut<'w>;
}

impl<'w> WorldSpawnIdentifierExt<'w> for World {
    #[inline(always)]
    fn spawn_client(&'w mut self, client_id: u32, bundle: impl Bundle) -> EntityWorldMut<'w> {
        self.spawn_with_id(0, client_id, bundle)
    }

    #[inline(always)]
    fn spawn_with_id(
        &'w mut self,
        id_type: impl Into<u8>,
        id: u32,
        bundle: impl Bundle,
    ) -> EntityWorldMut<'w> {
        let id = Identifier::new(id_type, id);
        let e = self.spawn((id, bundle)).id();
        self.resource_mut::<IdentifierMap>().insert(id, e);
        self.entity_mut(e)
    }
}

/// A map that tracks teh relation between [Identifier]s and [Entity]s. When an entity is
/// despawned, this state is tracked until the tick of the despawn message happens
#[derive(Resource, Default)]
pub struct IdentifierMap {
    from_id: bevy::utils::HashMap<Identifier, EntityStatus>,
    to_id: bevy::utils::HashMap<Entity, Identifier>,
    unspawned: Vec<(Identifier, Entity)>,
}

/// An error occured when mapping [Identifier]s
#[derive(Debug)]
pub enum IdentifierError {
    /// The [Identifier] was despawned
    Despawned,
    /// The [Identifier] does not exist
    NonExistent,
}

/// A [Result] returning a [IdentifierError]
pub type IdentifierResult<T> = Result<T, IdentifierError>;

/// The entity status for an [Identifier]
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum EntityStatus {
    /// There is an [Entity] alive for the [Identifier]
    Alive(Entity),
    /// The [Identifier] has been despawned
    Despawned(Tick),
}

impl IdentifierMap {
    /// The number of tracked entities that are alive
    pub fn n_alive(&self) -> usize {
        self.to_id.len()
    }

    /// The total number of identifiers being tracked, including despawned ones
    pub fn n_total(&self) -> usize {
        self.from_id.len()
    }

    /// Insert the mapping from [Identifier] to [Entity]
    #[inline(always)]
    pub fn insert(&mut self, id: Identifier, entity: Entity) {
        self.from_id.insert(id, EntityStatus::Alive(entity));
        self.to_id.insert(entity, id);
    }

    /// Get the [EntityStatus] for an [Identifier], using [Tick] when checking for despawns
    #[inline(always)]
    pub fn get(&self, id: &Identifier, tick: Tick) -> IdentifierResult<EntityStatus> {
        let status = self.from_id.get(id);
        if let Some(EntityStatus::Despawned(despawned_at)) = status {
            if *despawned_at < tick {
                return Err(IdentifierError::Despawned);
            }
        }
        match status {
            Some(v) => Ok(*v),
            None => Err(IdentifierError::NonExistent),
        }
    }

    /// Get the [Entity] for an [Identifier], returning an error if it was despawned.
    /// This function has little use outside of tests
    #[inline(always)]
    pub fn get_alive(&self, id: &Identifier) -> IdentifierResult<Entity> {
        let status = self.from_id.get(id);
        if let Some(EntityStatus::Alive(entity)) = status {
            return Ok(*entity);
        }
        if let Some(EntityStatus::Despawned(_)) = status {
            return Err(IdentifierError::Despawned);
        }
        Err(IdentifierError::NonExistent)
    }

    /// Get the Entity for a id type and id
    #[inline(always)]
    pub fn get_id(&self, id_type: impl Into<u8>, id: u32) -> IdentifierResult<Entity> {
        self.get_alive(&Identifier::new(id_type, id))
    }

    /// Get the Entity for a client if the client is present
    #[inline(always)]
    pub fn get_client(&self, client_id: u32) -> IdentifierResult<Entity> {
        self.get_alive(&Identifier::new(0, client_id))
    }

    /// Check if an entity with [Identifier] is alive at the given Tick
    pub fn is_alive(&self, id: &Identifier, tick: Tick) -> bool {
        let status = self.from_id.get(id);
        if let Some(EntityStatus::Despawned(despawned_at)) = status {
            if *despawned_at < tick {
                return false;
            }
        }
        status.is_some()
    }

    /// Get the [Identifier] for a [Entity]
    pub fn from_entity(&self, entity: &Entity) -> IdentifierResult<Identifier> {
        match self.to_id.get(entity) {
            Some(id) => Ok(*id),
            None => Err(IdentifierError::NonExistent),
        }
    }

    fn reserve(&mut self, id: Identifier, entities: &bevy::ecs::entity::Entities) -> Entity {
        let entity = entities.reserve_entity();
        self.insert(id, entity);
        self.unspawned.push((id, entity));
        entity
    }

    pub(crate) fn spawn_reserved(&mut self, world: &mut World) {
        for (id, entity) in self.unspawned.drain(..) {
            world.get_or_spawn(entity).unwrap().insert(id);
        }
    }

    /// Mark an [Identifier] as despawned at [Tick]
    pub fn despawn(&mut self, id: &Identifier, entity: &Entity, tick: Tick) {
        self.from_id.insert(*id, EntityStatus::Despawned(tick));
        self.to_id.remove(entity);
    }

    // TODO: Is this still necessary?
    /// Remove the mapping for an [Entity], returning the [Identifier] if it existed
    pub fn remove_entity(&mut self, entity: &Entity) -> Option<Identifier> {
        let id = self.to_id.remove(entity);
        if let Some(id) = id {
            self.from_id.remove(&id);
        }
        id
    }
}

/// A [Command] to insert an [Identifier]-[Entity] binding into the [IdentifierMap]
pub struct SpawnIdentifierCommand {
    id: Identifier,
    entity: Entity,
}

impl Command for SpawnIdentifierCommand {
    fn apply(self, world: &mut World) {
        world
            .resource_mut::<IdentifierMap>()
            .insert(self.id, self.entity);
    }
}

/// A manager for Identifiers
pub enum IdentifierManager<'a> {
    /// Actions will be ignored, because the current packet will be discarded
    Ignore,
    /// Actions will be fully executed, including spawning entities for unknown identifiers
    Full(&'a bevy::ecs::entity::Entities, &'a mut IdentifierMap),
}

impl<'a> IdentifierManager<'a> {
    /// Get the [EntityStatus] for an [Identifier] at the current [Tick].
    pub fn get(&mut self, id: &Identifier, tick: Tick) -> IdentifierResult<EntityStatus> {
        use IdentifierManager::*;
        match self {
            Ignore => Ok(EntityStatus::Alive(Entity::PLACEHOLDER)),
            Full(entities, id_map) => match id_map.get(id, tick) {
                Ok(status) => Ok(status),
                Err(_) => Ok(EntityStatus::Alive(id_map.reserve(*id, entities))),
            },
        }
    }

    /// Get the [Entity] for an [Identifier], returning an error if it was despawned.
    /// This function has little use outside of tests
    #[inline(always)]
    pub fn get_alive(&mut self, id: &Identifier) -> IdentifierResult<Entity> {
        use IdentifierManager::*;
        match self {
            Ignore => Ok(Entity::PLACEHOLDER),
            Full(entities, id_map) => match id_map.get_alive(id) {
                Ok(status) => Ok(status),
                Err(_) => Ok(id_map.reserve(*id, entities)),
            },
        }
    }

    /// Get the Entity for a id type and id
    #[inline(always)]
    pub fn get_id(&mut self, id_type: impl Into<u8>, id: u32) -> IdentifierResult<Entity> {
        let id_type = id_type.into();
        use IdentifierManager::*;
        match self {
            Ignore => Ok(Entity::PLACEHOLDER),
            Full(entities, id_map) => match id_map.get_id(id_type, id) {
                Ok(status) => Ok(status),
                Err(_) => {
                    let id = Identifier::new(id_type, id);
                    Ok(id_map.reserve(id, entities))
                }
            },
        }
    }

    /// Get the Entity for a client if the client is present
    #[inline(always)]
    pub fn get_client(&mut self, client_id: u32) -> IdentifierResult<Entity> {
        use IdentifierManager::*;
        match self {
            Ignore => Ok(Entity::PLACEHOLDER),
            Full(entities, id_map) => match id_map.get_client(client_id) {
                Ok(status) => Ok(status),
                Err(_) => {
                    let id = Identifier::new(0, client_id);
                    Ok(id_map.reserve(id, entities))
                }
            },
        }
    }

    /// Get the Identifier for an Entity, if it has one
    pub fn from_entity(&self, entity: Entity) -> IdentifierResult<Identifier> {
        use IdentifierManager::*;
        match self {
            Ignore => Ok(Identifier::new(0, 0)),
            Full(_, id_map) => id_map.from_entity(&entity),
        }
    }
}
