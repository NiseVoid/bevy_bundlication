//! Network replication based on bundles.
//!
//! Replication logic can be added to your app using [AppNetworkingExt].
//!
//! You can register bundles with [AppNetworkingExt::register_bundle], if the direction matches the
//! current app, any entity matching this bundle with an [Identifier] will then be sent over the network.
//! If the App is a client, it will only send packets if we have or can claim [Authority].
//! Direct updating of components can be avoided by adding the [Remote] on the entity, when this
//! component is around values will be stored there instead of the real field.
//!
//! You can register events with [AppNetworkingExt::register_event]. Events will be sent if the
//! direction matches, on the receiving side events are wrapped in [NetworkEvent]

// TODO: Crate a prelude, move code into a few separate files

#![warn(missing_docs)]

mod iter;

mod buffer;
pub use buffer::*;

pub use bevy_bundlication_macros::NetworkedBundle;

use std::any::{Any, TypeId};
use std::marker::PhantomData;

use bevy::{
    ecs::schedule::ScheduleLabel,
    ecs::system::{Command, EntityCommands},
    ecs::world::EntityWorldMut,
    prelude::*,
    reflect::TypePath,
    utils::{intern::Interned, HashMap},
};
pub use bincode as bincode_export;
#[cfg(not(any(test, feature = "test")))]
use renet::{RenetClient, RenetServer};
use serde::{Deserialize, Serialize};

/// A container for the remote values from synchronized bundles. If this component is around, then
/// updates for T will be stored here instead of being applied directly
#[derive(Component, Deref)]
pub struct Remote<T: Component> {
    tick: Tick,
    #[deref]
    value: T,
}

impl<T: Component + Default> Default for Remote<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: Component> Remote<T> {
    /// Construct a Remote with the given value
    #[inline(always)]
    pub fn new(value: T) -> Self {
        Self { tick: Tick(0), value }
    }

    /// Get the tick the latest remote value was from
    #[inline(always)]
    pub fn tick(&self) -> Tick {
        self.tick
    }

    /// Update the value and tick for this remote value
    #[inline(always)]
    pub fn update(&mut self, value: T, tick: Tick) {
        self.value = value;
        self.tick = tick;
    }
}

/// The tick a bundle was last updated at. Additionally, LastUpdate<()> is used to track the last
/// change to the entity.
#[derive(Component, Deref, DerefMut)]
pub struct LastUpdate<T> {
    #[deref]
    tick: Tick,
    _phantom: PhantomData<T>,
}

impl<T> PartialEq for LastUpdate<T> {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.tick == other.tick
    }
}

impl<T> std::fmt::Debug for LastUpdate<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.tick.fmt(f)
    }
}

impl<T> LastUpdate<T> {
    /// Construct a LastUpdate
    #[inline(always)]
    pub fn new(tick: Tick) -> Self {
        Self { tick, _phantom: PhantomData::<T> }
    }
}

/// An event fired when a client connects. When it is fired packets for all entities and bundles
/// that are relevant to this client are sent
#[derive(Event, Deref)]
pub struct NewConnection(pub Identity);

impl NewConnection {
    /// Get the send rule to send to only this new connection
    #[inline(always)]
    pub fn only(&self) -> SendRule {
        match **self {
            Identity::Server => SendRule::All,
            Identity::Client(c) => SendRule::Only(c),
        }
    }
}

/// The last tick the value was sent
#[derive(Component, Deref, DerefMut)]
pub struct LastSent<Bundle: NetworkedBundle> {
    #[deref]
    tick: Tick,
    _phantom: PhantomData<Bundle>,
}

impl<Bundle: NetworkedBundle> PartialEq for LastSent<Bundle> {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.tick == other.tick
    }
}

impl<Bundle: NetworkedBundle> std::fmt::Debug for LastSent<Bundle> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.tick.fmt(f)
    }
}

impl<Bundle: NetworkedBundle> LastSent<Bundle> {
    /// Construct a LastSent
    #[inline(always)]
    pub fn new(tick: Tick) -> Self {
        Self {
            tick,
            _phantom: PhantomData::<Bundle>,
        }
    }
}

/// The current Tick of the networked simulation
#[derive(Resource, Clone, Copy, Deref, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tick(pub u32);

use std::ops::{Add, Sub};

impl Add<u32> for Tick {
    type Output = Self;

    fn add(self, rhs: u32) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl Sub for Tick {
    type Output = u32;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0 - rhs.0
    }
}

impl Sub<u32> for Tick {
    type Output = Self;

    fn sub(self, rhs: u32) -> Self::Output {
        Self(self.0 - rhs)
    }
}

/// The system set in which the Tick is incremented, you should schedule your logic after this
#[derive(SystemSet, Clone, PartialEq, Eq, Debug, Hash)]
pub struct TickSet;

/// A component to override the owner of an entity. When this is present the provided client_id is
/// used instead of the Identifier
#[derive(Component, Clone, Copy, Debug, Deref, PartialEq, Eq)]
pub struct Owner(pub u32);

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

/// This component keeps track of what this entity is, the values get synced across all
/// clients/servers. For example you could have entity type 2 for enemies, and it is the 8th enemy to be spawned so it gets id 8.
/// entity_type 0 is special and reserved for players, the id needs to match with the client
/// ids from renet
#[derive(
    Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Identifier {
    /// The type of the entity
    pub entity_type: u8,
    /// The ID within this type
    pub id: u32,
}

impl Identifier {
    /// Check if the [Identifier] is a client
    #[inline(always)]
    pub fn is_client(&self) -> bool {
        self.entity_type == 0
    }

    fn new(entity_type: impl Into<u8>, id: u32) -> Self {
        Self { entity_type: entity_type.into(), id }
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
    from_ident: bevy::utils::HashMap<Identifier, EntityStatus>,
    to_ident: bevy::utils::HashMap<Entity, Identifier>,
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
pub enum EntityStatus {
    /// There is an [Entity] alive for the [Identifier]
    Alive(Entity),
    /// The [Identifier] has been despawned
    Despawned(Tick),
}

impl IdentifierMap {
    /// The number of tracked entities that are alive
    pub fn n_alive(&self) -> usize {
        self.to_ident.len()
    }

    /// The total number of identifiers being tracked, including despawned ones
    pub fn n_total(&self) -> usize {
        self.from_ident.len()
    }

    /// Insert the mapping from [Identifier] to [Entity]
    #[inline(always)]
    pub fn insert(&mut self, ident: Identifier, entity: Entity) {
        self.from_ident.insert(ident, EntityStatus::Alive(entity));
        self.to_ident.insert(entity, ident);
    }

    /// Get the [EntityStatus] for an [Identifier], using [Tick] when checking for despawns
    #[inline(always)]
    pub fn get(&self, ident: &Identifier, tick: Tick) -> IdentifierResult<&EntityStatus> {
        let status = self.from_ident.get(ident);
        if let Some(EntityStatus::Despawned(despawned_at)) = status {
            if *despawned_at < tick {
                return Err(IdentifierError::Despawned);
            }
        }
        match status {
            Some(v) => Ok(v),
            None => Err(IdentifierError::NonExistent),
        }
    }

    /// Get the [Entity] for an [Identifier], returning an error if it was despawned.
    /// This function has little use outside of tests
    #[inline(always)]
    pub fn get_alive(&self, ident: &Identifier) -> IdentifierResult<Entity> {
        let status = self.from_ident.get(ident);
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
    pub fn is_alive(&self, ident: &Identifier, tick: Tick) -> bool {
        let status = self.from_ident.get(ident);
        if let Some(EntityStatus::Despawned(despawned_at)) = status {
            if *despawned_at < tick {
                return false;
            }
        }
        status.is_some()
    }

    /// Get the [Identifier] for a [Entity]
    pub fn from_entity(&self, entity: &Entity) -> IdentifierResult<Identifier> {
        match self.to_ident.get(entity) {
            Some(ident) => Ok(*ident),
            None => Err(IdentifierError::NonExistent),
        }
    }

    /// Mark an [Identifier] as despawned at [Tick]
    pub fn despawn(&mut self, ident: &Identifier, entity: &Entity, tick: Tick) {
        self.from_ident.insert(*ident, EntityStatus::Despawned(tick));
        self.to_ident.remove(entity);
    }

    // TODO: Is this still necessary?
    /// Remove the mapping for an [Entity], returning the [Identifier] if it existed
    pub fn remove_entity(&mut self, entity: &Entity) -> Option<Identifier> {
        let ident = self.to_ident.remove(entity);
        if let Some(ident) = ident {
            self.from_ident.remove(&ident);
        }
        ident
    }
}

/// A [Command] to insert an [Identifier]-[Entity] binding into the [IdentifierMap]
pub struct SpawnIdentifierCommand {
    id: Identifier,
    entity: Entity,
}

impl Command for SpawnIdentifierCommand {
    fn apply(self, world: &mut World) {
        world.resource_mut::<IdentifierMap>().insert(self.id, self.entity);
    }
}

/// The channel on which despawn messages are sent
#[derive(Resource, Deref)]
pub struct DespawnChannel(u8);

fn send_despawns(
    mut removed: RemovedComponents<Identifier>,
    mut map: ResMut<IdentifierMap>,
    mut buffers: ResMut<Buffers>,
    held: Res<HeldAuthority>,
    our_ident: Res<Identity>,
    tick: Res<Tick>,
    despawn_channel: Res<DespawnChannel>,
) {
    for entity in removed.read() {
        let Some(ident) = map.remove_entity(&entity) else {
            continue;
        };
        if *our_ident != Identity::Server && !held.contains(&entity) {
            continue;
        }
        let mut buf =
            buffers.reserve_mut(BufferKey::new(**despawn_channel, SendRule::All), 6, *tick);
        buf.push(0);
        bincode::serialize_into(&mut buf, &ident).unwrap();
    }
}

fn handle_despawns(
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

/// A [HashSet] keeping track of which entities we hold [Authority] for. Only updated for clients,
/// as the server always has authority to modify things
#[derive(Resource, Deref, DerefMut, Default)]
pub struct HeldAuthority(bevy::utils::HashSet<Entity>);

fn track_authority(
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

/// A trait needed to network components, provided by a blanket impl if the component has
/// Serialize+Deserialize
pub trait NetworkedComponent: Sized {
    /// The type to serialize the component as
    type As: Serialize + for<'a> Deserialize<'a>;

    /// Convert the component value to a networked variant, using the current [Tick] and
    /// [IdentifierMap] to convert any necessary values
    fn to_networked(&self, tick: Tick, map: &IdentifierMap) -> IdentifierResult<Self::As>;

    /// Convert the networked value back to a component, using the [Tick] of the packet it was
    /// contained in and the [IdentifierMap] to convert any necessary values
    fn from_networked(
        tick: Tick,
        map: &IdentifierMap,
        networked: Self::As,
    ) -> IdentifierResult<Self>;
}

impl<T: Component + Clone + Serialize + for<'a> Deserialize<'a>> NetworkedComponent for T {
    type As = Self;

    fn to_networked(&self, _: Tick, _: &IdentifierMap) -> IdentifierResult<Self> {
        Ok(self.clone())
    }

    fn from_networked(_: Tick, _: &IdentifierMap, networked: Self) -> IdentifierResult<Self> {
        Ok(networked)
    }
}

/// A trait that allows wrapping a component as another type for bevy_bundlication. Useful when working
/// with components from bevy itself or 3rd party plugins
pub trait NetworkedWrapper<From: Component>: Serialize + for<'a> Deserialize<'a> {
    /// Construct this network representation from the component. Use the current [Tick] and
    /// [IdentifierMap] to convert any necessary values
    fn from_component(tick: Tick, map: &IdentifierMap, from: &From) -> IdentifierResult<Self>;

    /// Reconstruct the component from this network representation. Use the [Tick] of the packet it
    /// was contained in and the [IdentifierMap] to convert any necessary values
    fn to_component(self, tick: Tick, map: &IdentifierMap) -> IdentifierResult<From>;
}

/// An event received over the network, contains the [Identity] of the sender as well as the [Tick]
/// it was timestamped with
#[derive(Event)]
pub struct NetworkEvent<Event> {
    /// The [Identity] of the sender
    pub sender: Identity,
    /// The [Tick] the packet that contained this event was timestamped with
    pub tick: Tick,
    /// The actual event
    pub event: Event,
}

impl<E: Event + PartialEq> PartialEq for NetworkEvent<E> {
    fn eq(&self, other: &Self) -> bool {
        self.sender == other.sender && self.tick == other.tick && self.event == other.event
    }
}

impl<E: Event + std::fmt::Debug> std::fmt::Debug for NetworkEvent<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkEvent")
            .field("sender", &self.sender)
            .field("tick", &self.tick)
            .field("event", &self.event)
            .finish()
    }
}
/// A function that sends changes over the network
pub type SendChangeFn = fn(
    Identifier,
    Option<&Owner>,
    Option<&Authority>,
    &EntityRef,
    u8,
    &mut Buffers,
    &IdentifierMap,
    Tick,
    Identity,
    &[u32],
) -> ();

/// A function that applies changes from packets received over the network
pub type ApplyChangeFn = fn(&mut World, Identity, Tick, &mut std::io::Cursor<&[u8]>) -> ();

/// A trait that allows bundles to be networked, it expects a bundle to register systems that send
/// messages and return a handle to apply changes for its packets. This should not be impl'd
/// directly and should instead be derived using the macro
pub trait NetworkedBundle: Bundle + TypePath + Any + Sized {
    /// Fetch the component ids contained in this bundle
    fn get_component_ids(world: &mut World) -> Vec<bevy::ecs::component::ComponentId>;

    /// Get the serialize function for this bundle
    fn serializer<const CHANNEL: u8, Method: SendMethod>() -> SendChangeFn;

    /// Get the handler for packets of this bundle
    fn handler() -> ApplyChangeFn;
}

/// A trait that allows events to be networked
pub trait NetworkedEvent: Sync + Send + TypePath + Any + Sized {
    /// The type the event is networked as
    type As: Serialize + for<'a> Deserialize<'a>;

    /// Convert the event to a networked format, along with the rule for who receives it.
    /// Use [Tick] and [IdentifierMap] map to convert any necessary values.
    fn to_networked(&self, tick: Tick, map: &IdentifierMap) -> IdentifierResult<Self::As>;

    /// Reconstruct the event from the networked representation. Use [Tick] and [IdentifierMap] to
    /// convert any necessary values
    fn from_networked(
        tick: Tick,
        map: &IdentifierMap,
        networked: Self::As,
    ) -> IdentifierResult<Self>;
}

impl<T: Sync + Send + Clone + TypePath + Serialize + for<'a> Deserialize<'a>> NetworkedEvent for T {
    type As = Self;

    #[inline(always)]
    fn to_networked(&self, _: Tick, _: &IdentifierMap) -> IdentifierResult<Self> {
        Ok(self.clone())
    }

    #[inline(always)]
    fn from_networked(_: Tick, _: &IdentifierMap, networked: Self) -> IdentifierResult<Self> {
        Ok(networked)
    }
}

/// A bundle that was registered to the app
#[derive(Clone, Debug)]
pub struct RegisteredBundle {
    packet_id: u8,
    component_ids: Vec<bevy::ecs::component::ComponentId>,
    serialize: SendChangeFn,
    handler: ApplyChangeFn,
    path: &'static str,
}

/// An event that was registered to the app
#[derive(Clone, Debug)]
pub struct RegisteredEvent {
    packet_id: u8,
    handler: ApplyChangeFn,
    path: &'static str,
}

/// A trait with information about the [Direction] and [SendRule]s for a bundle
pub trait SendMethod: 'static + Sized + Sync + Send {
    /// The [Direction] of this send method
    type Direction: Direction;

    /// Return who needs to receive the packet, if the [Identifier] of an entity is a client, the
    /// client id is provided. If None is returned the packet is not sent
    fn rule(client: Option<u32>) -> Option<SendRule>;
}

/// The Direction for a bundle or event, either [ClientToServer] or [ServerToClient]
pub trait Direction: Resource + 'static + Sized + Sync + Send + std::fmt::Debug + Default {
    /// The opposite direction
    type Reverse: Direction;
    /// The resource used for this send direction, used to send and receive messsages
    type NetRes: Resource + Sync + Send + Sized;

    /// Receive all messages and handle them by calling process on [Handlers]
    fn receive_messages(
        net: &mut Self::NetRes,
        world: &mut World,
        handlers: &Handlers<Self::Reverse>,
        channels: &[u8],
    );

    /// Send the provided messages
    fn send_messages(net: &mut Self::NetRes, msgs: std::vec::Drain<(BufferKey, Vec<u8>)>);
}

/// The client to server [Direction]
#[derive(Resource, Debug, Default)]
pub struct ClientToServer;

impl SendMethod for ClientToServer {
    type Direction = Self;

    #[inline(always)]
    fn rule(_: Option<u32>) -> Option<SendRule> {
        Some(SendRule::All)
    }
}

/// The server to client [Direction]
#[derive(Resource, Debug, Default)]
pub struct ServerToClient;

/// A send method that broadcasts data to all clients
pub struct ServerToAll;
impl SendMethod for ServerToAll {
    type Direction = ServerToClient;

    #[inline(always)]
    fn rule(_: Option<u32>) -> Option<SendRule> {
        Some(SendRule::All)
    }
}

/// A send method that sends packets only to clients that own the entity
pub struct ServerToOwner;
impl SendMethod for ServerToOwner {
    type Direction = ServerToClient;

    #[inline(always)]
    fn rule(client: Option<u32>) -> Option<SendRule> {
        client.map(SendRule::Only)
    }
}

/// A send method that sends packets only to clients who DON'T own the entity
pub struct ServerToObserver;
impl SendMethod for ServerToObserver {
    type Direction = ServerToClient;

    #[inline(always)]
    fn rule(client: Option<u32>) -> Option<SendRule> {
        client.map(SendRule::Except).or(Some(SendRule::All))
    }
}

#[cfg(any(test, feature = "test"))]
mod test_impl;
#[cfg(any(test, feature = "test"))]
pub use test_impl::*;

#[cfg(not(any(test, feature = "test")))]
mod renet_impl;
#[cfg(not(any(test, feature = "test")))]
pub use renet_impl::*;

#[derive(Resource, Deref, DerefMut)]
struct RegistryDir<T> {
    #[deref]
    registry: Registry,
    _phantom: PhantomData<T>,
}
impl<T> Default for RegistryDir<T> {
    fn default() -> Self {
        Self {
            registry: default(),
            _phantom: PhantomData,
        }
    }
}

#[derive(Resource, Deref, DerefMut, Default)]
struct Channels(std::collections::BTreeSet<u8>);

#[derive(Debug, Default)]
struct Registry {
    bundles: HashMap<TypeId, RegisteredBundle>,
    events: HashMap<TypeId, RegisteredEvent>,
}

/// An extention trait for [App] that adds methods to register networked bundles and events
pub trait AppNetworkingExt {
    /// Register a bundle
    fn register_bundle<Method: SendMethod, Bundle: NetworkedBundle, const CHANNEL: u8>(
        &mut self,
    ) -> &mut App;
    /// Register an event
    fn register_event<Dir: Direction, Bundle: NetworkedEvent, const CHANNEL: u8>(
        &mut self,
    ) -> &mut App;
}

impl AppNetworkingExt for App {
    fn register_bundle<Method: SendMethod, Bundle: NetworkedBundle, const CHANNEL: u8>(
        &mut self,
    ) -> &mut App {
        let component_ids = Bundle::get_component_ids(&mut self.world);
        let mut registry = self.world.resource_mut::<RegistryDir<Method::Direction>>();
        registry.bundles.insert(
            TypeId::of::<Bundle>(),
            RegisteredBundle {
                packet_id: 0,
                component_ids,
                serialize: Bundle::serializer::<CHANNEL, Method>(),
                handler: Bundle::handler(),
                path: Bundle::type_path(),
            },
        );

        self.world.resource_mut::<Channels>().insert(CHANNEL);

        self
    }

    fn register_event<Dir: Direction, Event: NetworkedEvent, const CHANNEL: u8>(
        &mut self,
    ) -> &mut App {
        let mut registry = self.world.resource_mut::<RegistryDir<Dir>>();
        registry.events.insert(
            TypeId::of::<Event>(),
            RegisteredEvent {
                packet_id: 0,
                handler: handle_event::<Event>,
                path: Event::type_path(),
            },
        );

        self.add_systems(Startup, load_event_id::<Event, Dir>.after(GenerateSet));
        self.world.resource_mut::<Channels>().insert(CHANNEL);

        if self.world.get_resource::<Dir>().is_none()
            && self.world.get_resource::<Events<NetworkEvent<Event>>>().is_none()
        {
            self.init_resource::<Events<NetworkEvent<Event>>>();
            self.add_systems(Last, clear_events::<NetworkEvent<Event>>);
        }

        self
    }
}

fn clear_events<E: Event>(mut events: ResMut<Events<E>>) {
    events.clear();
}

// TODO: We can probably store the channel when registering it, so we don't need to search it here
// TODO: Add methods to more cleanly construct this type without using SendRule directly
/// A Command to send an event
pub struct SendEvent<Event: NetworkedEvent> {
    /// The event to send
    pub event: Event,
    /// The channel in which the event gets sent
    pub channel: u8,
    /// The rule for who receives the event
    pub rule: SendRule,
}

impl<Event: NetworkedEvent> bevy::ecs::system::Command for SendEvent<Event> {
    fn apply(self, world: &mut World) {
        let tick = *world.resource::<Tick>();
        let packet_id = **world.resource::<Id<Event>>();
        let map = world.resource::<IdentifierMap>();
        let Ok(networked) = self.event.to_networked(tick, map) else {
            warn!(
                "Tried to send event {:?} but failed to find a necessary Identifier",
                Event::type_path()
            );
            return;
        };

        let mut buffers = world.resource_mut::<Buffers>();

        let packet_size = 1 + bincode::serialized_size(&networked).unwrap() as usize;
        let mut buf =
            buffers.reserve_mut(BufferKey::new(self.channel, self.rule), packet_size, tick);
        buf.push(packet_id);
        bincode::serialize_into(&mut buf, &networked).unwrap();
    }
}

fn handle_event<Event: NetworkedEvent>(
    world: &mut World,
    ident: Identity,
    tick: Tick,
    cursor: &mut std::io::Cursor<&[u8]>,
) {
    let Ok(networked): Result<Event::As, _> = bincode::deserialize_from(cursor) else {
        return;
    };

    let map = world.resource::<IdentifierMap>();
    let Ok(event) = Event::from_networked(tick, map, networked) else {
        warn!("Got event {:?} with unresolvable Identifier", Event::type_path());
        return;
    };

    let network_event = NetworkEvent { sender: ident, tick, event };
    world.send_event(network_event);
}

/// A resource holding handlers for each known packet id
#[derive(Resource, Deref, DerefMut)]
pub struct Handlers<Dir: Direction> {
    #[deref]
    handlers: bevy::utils::HashMap<u8, ApplyChangeFn>,
    _phantom: PhantomData<Dir>,
}

impl<Dir: Direction> Handlers<Dir> {
    /// Construct a [Handlers] with the specified capacity
    pub fn with_capacity(cap: usize) -> Self {
        let mut handlers = bevy::utils::HashMap::<u8, ApplyChangeFn>::with_capacity(1 + cap);
        handlers.insert(0, handle_despawns);
        Self {
            handlers,
            _phantom: PhantomData::<Dir>,
        }
    }

    /// Process a packet, should be called immediately on every packet received in [Direction]
    #[inline(always)]
    pub(crate) fn process(&self, world: &mut World, ident: Identity, b: &[u8]) {
        use std::io::Read;
        let mut cursor = std::io::Cursor::new(b);
        let mut buf = [0u8; 4];
        let Ok(_) = cursor.read_exact(&mut buf) else {
            return;
        };
        let tick = Tick(u32::from_le_bytes(buf));

        loop {
            let mut buf = [0u8; 1];
            let Ok(_) = cursor.read_exact(&mut buf) else {
                break;
            };

            let Some(handler) = self.get(&buf[0]) else {
                break;
            };
            (handler)(world, ident, tick, &mut cursor);
        }
    }
}

/// A function to check if a packet should be sent based on our [Identity] and the entity's [Authority],
/// returns the [SendRule] if it should be sent
#[inline(always)]
pub fn should_send<Method: SendMethod>(
    our_identity: Identity,
    auth: Option<&Authority>,
    owner: Option<&Owner>,
    ident: Identifier,
) -> Option<SendRule> {
    if !our_identity.can_send(auth) {
        return None;
    }

    let client_id = match (owner, ident.is_client()) {
        (Some(client_id), _) => Some(**client_id),
        (_, true) => Some(ident.id),
        (_, false) => None,
    };
    Method::rule(client_id)
}

fn receive_messages<Dir: Direction>(world: &mut World) {
    let handlers = world
        .remove_resource::<Handlers<Dir::Reverse>>()
        .expect("Missing Handlers resource");

    let mut net = world
        .remove_resource::<Dir::NetRes>()
        .expect("Missing NetRes resource");

    let channels: Vec<u8> = world.resource::<Channels>().iter().cloned().collect();

    Dir::receive_messages(&mut net, world, &handlers, &channels);

    world.insert_resource(handlers);
    world.insert_resource(net);
}

fn send_buffers<Dir: Direction>(world: &mut World) {
    let mut buf = world.remove_resource::<Buffers>().expect("Missing Buffers resource");

    let mut net = world
        .remove_resource::<Dir::NetRes>()
        .expect("Missing NetRes resource");

    Dir::send_messages(&mut net, buf.buffers.drain(..));

    buf.clear();

    world.insert_resource(net);
    world.insert_resource(buf);
}

/// The packet ID for a bundle/event
#[derive(Resource, Deref)]
pub struct Id<T> {
    #[deref]
    id: u8,
    _phantom: PhantomData<T>,
}

impl<T> Id<T> {
    fn new(id: u8) -> Self {
        Self { id, _phantom: PhantomData::<T> }
    }
}

fn generate_ids<Dir: Direction>(mut commands: Commands, mut registry: ResMut<RegistryDir<Dir>>) {
    let mut i = 0u8;

    let mut bundles = registry
        .bundles
        .iter()
        .map(|(t, r)| (*t, r.clone()))
        .collect::<Vec<_>>();
    bundles.sort_by(|(_, a), (_, b)| a.path.cmp(b.path));

    for (type_id, _) in bundles.iter() {
        i += 1;
        registry.bundles.get_mut(type_id).unwrap().packet_id = i;
    }

    let mut events = registry
        .events
        .iter()
        .map(|(t, r)| (*t, r.clone()))
        .collect::<Vec<_>>();
    events.sort_by(|(_, a), (_, b)| a.path.cmp(b.path));
    for (type_id, _) in events.iter() {
        i += 1;
        registry.events.get_mut(type_id).unwrap().packet_id = i;
    }

    let mut handlers = Handlers::<Dir>::with_capacity(bundles.len() + events.len());

    for bundle in registry.bundles.values() {
        handlers.insert(bundle.packet_id, bundle.handler);
    }

    for event in registry.events.values() {
        handlers.insert(event.packet_id, event.handler);
    }

    commands.insert_resource(handlers);
}

fn load_event_id<Event: NetworkedEvent, Dir: Direction>(
    mut commands: Commands,
    registry: Res<RegistryDir<Dir>>,
) {
    let Some(reg) = registry.events.get(&TypeId::of::<Event>()) else {
        warn!("Event {} got no ID", Event::type_path());
        return;
    };
    commands.insert_resource(Id::<Event>::new(reg.packet_id));
}

/// A plugin that adds a client's network replication capabilities to the app
pub struct ClientNetworkingPlugin {
    /// The channel that should be used for despawn messages. Should be a reliable channel
    pub despawn_channel: u8,
    /// The schedule in which the Tick gets advanced
    pub tick_schedule: Interned<dyn ScheduleLabel>,
}

#[cfg(feature = "test")]
impl ClientNetworkingPlugin {
    /// Create a client bevy_bundlication plugin for a test
    pub fn new(channel: u8) -> Self {
        Self {
            despawn_channel: channel,
            tick_schedule: Last.intern(),
        }
    }
}

impl Plugin for ClientNetworkingPlugin {
    fn build(&self, app: &mut App) {
        app
            .insert_resource(Identity::Client(0)) // TODO: Figure out our own client id
            .add_plugins(NetworkingPlugin {
                despawn_channel: self.despawn_channel,
                tick_schedule: self.tick_schedule,
                _phantom: PhantomData::<ClientToServer>,
            });
    }
}

/// A plugin that adds a server's network replication capabilities to the app
pub struct ServerNetworkingPlugin {
    /// The channel that should be used for despawn messages. Should be a reliable channel
    pub despawn_channel: u8,
    /// The schedule in which the Tick gets advanced
    pub tick_schedule: Interned<dyn ScheduleLabel>,
}

#[cfg(feature = "test")]
impl ServerNetworkingPlugin {
    /// Create a server bevy_bundlication plugin for a test
    pub fn new(channel: u8) -> Self {
        Self {
            despawn_channel: channel,
            tick_schedule: Last.intern(),
        }
    }
}

impl Plugin for ServerNetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Identity::Server).add_plugins(NetworkingPlugin {
            despawn_channel: self.despawn_channel,
            tick_schedule: self.tick_schedule,
            _phantom: PhantomData::<ServerToClient>,
        });
    }
}

#[derive(SystemSet, Clone, PartialEq, Eq, Debug, Hash)]
struct GenerateSet;

// TODO: Add better SystemSet for actual end users to schedule before/after bevy_bundlication stuff

/// A [SystemSet] containing all systems to replicate data between apps
#[derive(SystemSet, Clone, PartialEq, Eq, Debug, Hash)]
pub enum NetworkingSet {
    /// The set that contains all systems to receive data
    Receive,
    /// The set that contains all data to send data
    Send,
}

/// A [SystemSet] to group and order different internal stages for replication logic
#[derive(SystemSet, Clone, PartialEq, Eq, Debug, Hash)]
pub enum InternalSet {
    /// Read packets from the network
    ReadPackets,
    /// Receive and process messages in received packets
    ReceiveMessages,
    /// Send changes
    SendChanges,
    /// Send packets over the network
    SendPackets,
}

struct NetworkingPlugin<Direction> {
    despawn_channel: u8,
    tick_schedule: Interned<dyn ScheduleLabel>,
    _phantom: PhantomData<Direction>,
}

impl<Dir: Direction> Plugin for NetworkingPlugin<Dir> {
    fn build(&self, app: &mut App) {
        app.init_resource::<Tick>()
            .init_resource::<Dir>()
            .init_resource::<Buffers>()
            .init_resource::<Channels>()
            .init_resource::<IdentifierMap>()
            .init_resource::<HeldAuthority>()
            .init_resource::<RegistryDir<ServerToClient>>()
            .init_resource::<RegistryDir<ClientToServer>>()
            .init_resource::<Events<NewConnection>>()
            .init_resource::<Tick>()
            .insert_resource(DespawnChannel(self.despawn_channel))
            .add_systems(self.tick_schedule, increment_tick.in_set(TickSet))
            .add_systems(Last, |mut events: ResMut<Events<NewConnection>>| events.clear())
            // TODO: Also configure renet's sets to be in Receive/Send, if it is enabled
            .configure_sets(PreUpdate, (
                    InternalSet::ReadPackets,
                    InternalSet::ReceiveMessages,
            ).chain().in_set(NetworkingSet::Receive))
            .add_systems(PreUpdate, (receive_messages::<Dir>.in_set(InternalSet::ReceiveMessages),).chain())
            .configure_sets(
                PostUpdate,
                (InternalSet::SendChanges, InternalSet::SendPackets)
                    .chain()
                    .in_set(NetworkingSet::Send),
            )
            .add_systems(
                PostUpdate,
                (
                    (send_despawns, track_authority, iter::iterate_world::<Dir>)
                        .chain()
                        .in_set(InternalSet::SendChanges),
                    send_buffers::<Dir>.after(InternalSet::SendChanges).before(InternalSet::SendPackets).in_set(NetworkingSet::Send),
                ),
            )
            .add_systems(Startup, generate_ids::<ServerToClient>.in_set(GenerateSet))
            .add_systems(Startup, generate_ids::<ClientToServer>.in_set(GenerateSet));
    }
}

fn increment_tick(mut tick: ResMut<Tick>) {
    *tick = *tick + 1;
}
