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

#![warn(missing_docs)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

mod tick;
use tick::Tick;

mod identifier;
use identifier::{EntityStatus, Identifier, IdentifierMap, IdentifierResult, Owner};

mod component_info;
pub use component_info::Remote;

mod bundle_info;
pub use bundle_info::LastUpdate;

mod iter;

mod despawn;

mod client_authority;
pub use client_authority::{Authority, Identity};

mod buffer;
pub use buffer::*;

pub use bincode::{deserialize_from as deserialize, serialize_into as serialize};

pub mod prelude {
    //! The prelude of the crate, contains everything necessary to get started with this crate

    pub mod exts {
        //! A sub-prelude containing all extension traits to use this crate
        pub use crate::{
            identifier::{
                CommandsSpawnIdentifierExt, EntityCommandsInsertIdentifierExt,
                SpawnIdentifierCommand, WorldSpawnIdentifierExt,
            },
            AppNetworkingExt,
        };
    }
    pub use exts::*;

    pub use crate::{
        buffer::SendRule,
        client_authority::{Authority, Identity},
        component_info::Remote,
        deserialize,
        identifier::{
            EntityStatus, Identifier, IdentifierError, IdentifierMap, IdentifierResult, Owner,
        },
        serialize,
        tick::{Tick, TickSet},
        BundlicationSet, ClientNetworkingPlugin, ClientToServer, NetworkEvent, NetworkedBundle,
        NetworkedComponent, NetworkedEvent, NetworkedWrapper, SendEvent, ServerNetworkingPlugin,
        ServerToAll, ServerToClient, ServerToObserver, ServerToOwner,
    };
    pub use bevy_bundlication_macros::NetworkedBundle;

    #[cfg(any(test, feature = "test"))]
    pub use crate::{
        test_impl::{ClientMessages, ServerMessages},
        Connected, StartReplication,
    };
}

pub mod macro_export {
    //! A module with exports used by the macro

    pub use crate::{
        buffer::{TakenBuffers, WriteBuffer, WriteFilters},
        bundle_info::LastUpdate,
        client_authority::Identity,
        deserialize,
        prelude::*,
        serialize, ApplyEntityChangeFn, ConsumeFn, SendChangeFn, SendMethod,
    };
}

use std::any::{Any, TypeId};
use std::io::{Read, Write};
use std::marker::PhantomData;

use bevy::{
    ecs::schedule::ScheduleLabel,
    prelude::*,
    reflect::TypePath,
    utils::{intern::Interned, HashMap},
};

use serde::{Deserialize, Serialize};

/// An event fired when a client is ready for replication. When it is fired packets for all
/// entities and bundles that are relevant to this client are sent
#[derive(Event, Deref)]
pub struct StartReplication(pub Identity);

/// An event fired when a client connects
#[derive(Event, Deref)]
pub struct Connected(pub Identity);

/// An event fired when a client disconnects.
#[derive(Event, Deref)]
pub struct Disconnected(pub Identity);

// TODO: Change error handling. Reads should not be forced to resort to panics
/// A trait needed to network components, provided by a blanket impl if the component has
/// Serialize+Deserialize
pub trait NetworkedComponent: Sized {
    /// Write the component to the network, using the current [Tick] and [IdentifierMap] to
    /// convert any necessary values
    fn write_data(
        &self,
        writer: impl Write,
        tick: Tick,
        map: &IdentifierMap,
    ) -> IdentifierResult<()>;

    /// Read the component from the network, using the [Tick] of the packet it was contained
    /// in and the [IdentifierMap] to convert any necessary values
    fn read_new(reader: impl Read, tick: Tick, map: &mut IdentifierMap) -> IdentifierResult<Self>;

    /// Read the component in-place from the network, this can be used to write directly to
    fn read_in_place(
        &mut self,
        reader: impl Read,
        tick: Tick,
        map: &mut IdentifierMap,
    ) -> IdentifierResult<()> {
        *self = Self::read_new(reader, tick, map)?;
        Ok(())
    }
}

impl<T: Component + Serialize + for<'a> Deserialize<'a>> NetworkedComponent for T {
    fn write_data(&self, w: impl Write, _: Tick, _: &IdentifierMap) -> IdentifierResult<()> {
        serialize(w, self).unwrap();
        Ok(())
    }

    fn read_new(r: impl Read, _: Tick, _: &mut IdentifierMap) -> IdentifierResult<Self> {
        Ok(deserialize(r).unwrap())
    }
}

/// A trait that allows wrapping a component as another type for bevy_bundlication. Useful when working
/// with components from bevy itself or 3rd party plugins
pub trait NetworkedWrapper<From: Component> {
    /// Write the component to the network, using the current [Tick] and [IdentifierMap] to
    /// convert any necessary values
    fn write_data(
        from: &From,
        writer: impl Write,
        tick: Tick,
        map: &IdentifierMap,
    ) -> IdentifierResult<()>;

    /// Read the component from the network, using the [Tick] of the packet it was contained
    /// in and the [IdentifierMap] to convert any necessary values
    fn read_new(reader: impl Read, tick: Tick, map: &mut IdentifierMap) -> IdentifierResult<From>;

    /// Read the component in-place from the network, this can be used to write directly to
    fn read_in_place(
        from: &mut From,
        reader: impl Read,
        tick: Tick,
        map: &mut IdentifierMap,
    ) -> IdentifierResult<()> {
        *from = Self::read_new(reader, tick, map)?;
        Ok(())
    }
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
    &[bevy::ecs::component::ComponentId],
    Option<u32>,
    EntityRef,
    u8,
    &mut TakenBuffers,
    &mut WriteBuffer,
    &IdentifierMap,
    Tick,
    bevy::ecs::component::Tick,
) -> ();

/// A function that applies changes from packets received over the network
pub type ApplyChangeFn = fn(&mut World, Identity, Tick, &mut std::io::Cursor<&[u8]>) -> ();

/// A function that applies changes to a specific entity
pub type ApplyEntityChangeFn =
    fn(&mut EntityWorldMut, &mut IdentifierMap, Identity, Tick, &mut std::io::Cursor<&[u8]>) -> ();

// TODO: ConsumeFn can still apply stuff via IdentifierMap, maybe we need some disable toggle for
// it?
/// A function that consumes the packet bytes without applying them
pub type ConsumeFn = fn(&mut IdentifierMap, Tick, &mut std::io::Cursor<&[u8]>) -> ();

/// A trait that allows groups of components to be networked, this trait should not be impl'd
/// directly and instead be implemented by the derive macro of the same name.
pub trait NetworkedBundle: TypePath + Any + Sized {
    /// Fetch the component ids contained in this bundle
    fn get_component_ids(world: &mut World) -> Vec<bevy::ecs::component::ComponentId>;

    /// Get the serialize function for this bundle
    fn serializer<const CHANNEL: u8, Method: SendMethod>() -> SendChangeFn;

    /// Get the updater for packets of this bundle
    fn updater() -> ApplyEntityChangeFn;

    /// Get the spawner for packets of this bundle
    fn spawner() -> ApplyEntityChangeFn;

    /// Get the consumer of invalid packets for this bundle
    fn consumer() -> ConsumeFn;
}

// TODO: Sync this trait's api with NetworkedComponent/Wrapper
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
        map: &mut IdentifierMap,
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
    fn from_networked(_: Tick, _: &mut IdentifierMap, networked: Self) -> IdentifierResult<Self> {
        Ok(networked)
    }
}

/// A bundle that was registered to the app
#[derive(Clone, Debug)]
pub struct RegisteredBundle {
    packet_id: u8,
    component_ids: Vec<bevy::ecs::component::ComponentId>,
    serialize: SendChangeFn,
    updater: ApplyEntityChangeFn,
    spawner: ApplyEntityChangeFn,
    consumer: ConsumeFn,
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
}

impl Direction for ClientToServer {
    type Reverse = ServerToClient;
}

impl Direction for ServerToClient {
    type Reverse = ClientToServer;
}

/// A trait for a networking implementation, implementing this trait and creating a plugin to add
/// the necessary systems, ordering, and conditions allows you to use this crate with any
/// low-level networking crate
pub trait NetImpl<Dir: Direction>: Resource + Sync + Send + Sized {
    /// Receive all messages and handle them by calling process on [Handlers]
    fn receive_messages(
        &mut self,
        world: &mut World,
        handlers: &Handlers<Dir::Reverse>,
        channels: &[u8],
    );

    /// Send the provided messages
    fn send_messages(&mut self, msgs: impl Iterator<Item = (BufferKey, Vec<u8>)>);
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

#[cfg(feature = "renet")]
mod renet_impl;
#[cfg(feature = "renet")]
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
                updater: Bundle::updater(),
                spawner: Bundle::spawner(),
                consumer: Bundle::consumer(),
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
            && self
                .world
                .get_resource::<Events<NetworkEvent<Event>>>()
                .is_none()
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

        let mut buf = world.remove_resource::<WriteBuffer>().unwrap();
        let connections = world.remove_resource::<Connections>().unwrap();

        let mut buffers = world.resource_mut::<Buffers>();
        let mut buffer = buffers.take(
            tick,
            self.channel,
            bevy::ecs::component::Tick::new(0),
            connections
                .iter()
                .map(|i| (i.ident, RecipientData::default())),
        );

        buf.push(Packet::EVENT);
        buf.push(packet_id);
        serialize(&mut buf, &networked).unwrap();

        buffer.send(self.rule, &mut buf);
        buffer.fragment();
        drop(buffer);

        world.insert_resource(buf);
        world.insert_resource(connections);
    }
}

fn handle_event<Event: NetworkedEvent>(
    world: &mut World,
    ident: Identity,
    tick: Tick,
    cursor: &mut std::io::Cursor<&[u8]>,
) {
    let Ok(networked): Result<Event::As, _> = deserialize(cursor) else {
        return;
    };

    let mut id_map = world.resource_mut::<IdentifierMap>();
    let Ok(event) = Event::from_networked(tick, &mut id_map, networked) else {
        warn!(
            "Got event {:?} with unresolvable Identifier",
            Event::type_path()
        );
        return;
    };

    let network_event = NetworkEvent {
        sender: ident,
        tick,
        event,
    };
    world.send_event(network_event);
}

/// A resource holding handlers for each known packet id
#[derive(Resource)]
pub struct Handlers<Dir: Direction> {
    events: Vec<ApplyChangeFn>,
    bundles: Vec<([ApplyEntityChangeFn; 2], ConsumeFn)>,
    _phantom: PhantomData<Dir>,
}

struct Packet;

impl Packet {
    const DESPAWN: u8 = 0;
    const ENTITY: u8 = 1;
    const EVENT: u8 = 2;
}

impl<Dir: Direction> Handlers<Dir> {
    /// Construct a [Handlers] with the specified capacity
    pub fn with_capacity(bundles: usize, events: usize) -> Self {
        Self {
            bundles: Vec::with_capacity(bundles),
            events: Vec::with_capacity(events),
            _phantom: PhantomData::<Dir>,
        }
    }

    /// Process a packet, should be called immediately on every packet received in [Direction]
    #[inline(always)]
    pub(crate) fn process(&self, world: &mut World, ident: Identity, b: &[u8]) {
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

            match buf[0] {
                Packet::DESPAWN => {
                    despawn::handle_despawns(world, ident, tick, &mut cursor);
                }
                Packet::ENTITY => {
                    if !handle_entity(&self.bundles, world, ident, tick, &mut cursor) {
                        break;
                    }
                }
                Packet::EVENT => {
                    let Ok(_) = cursor.read_exact(&mut buf) else {
                        break;
                    };
                    if buf[0] == 0 {
                        break;
                    }

                    let Some(handler) = self.events.get((buf[0] - 1) as usize) else {
                        break;
                    };
                    (handler)(world, ident, tick, &mut cursor);
                }
                _ => break,
            }
        }
    }
}

fn handle_entity(
    handlers: &[([ApplyEntityChangeFn; 2], ConsumeFn)],
    world: &mut World,
    ident: Identity,
    tick: Tick,
    mut cursor: &mut std::io::Cursor<&[u8]>,
) -> bool {
    let Ok(identifier) = deserialize(&mut cursor) else {
        return false;
    };

    world.resource_scope(|world, mut id_map: Mut<'_, IdentifierMap>| {
        let entity = match id_map.get(&identifier, tick) {
            Ok(EntityStatus::Alive(entity)) => Some(*entity),
            Ok(EntityStatus::Despawned(_)) => {
                return consume_bundles(handlers, &mut id_map, tick, cursor);
            }
            Err(_) => None,
        };
        let (mut entity, handler_index) = if let Some(entity) = entity {
            (world.entity_mut(entity), 0)
        } else {
            if ident != Identity::Server {
                warn!("Client sent data for unknown Identifier: {:?}", identifier);
                return false;
            }
            let entity = world.spawn((identifier, LastUpdate::<()>::new(tick)));
            id_map.insert(identifier, entity.id());
            (entity, 1)
        };

        let mut buf = [0u8; 1];
        loop {
            let Ok(_) = cursor.read_exact(&mut buf) else {
                return false;
            };
            // Bundle id 0 is used to mark the end of an entity
            if buf[0] == 0 {
                return true;
            }

            let Some(handler) = handlers.get((buf[0] - 1) as usize) else {
                return false;
            };
            (handler.0[handler_index])(&mut entity, &mut id_map, ident, tick, cursor);
        }
    })
}

fn consume_bundles(
    handlers: &[([ApplyEntityChangeFn; 2], ConsumeFn)],
    id_map: &mut IdentifierMap,
    tick: Tick,
    cursor: &mut std::io::Cursor<&[u8]>,
) -> bool {
    let mut buf = [0u8; 1];
    loop {
        let Ok(_) = cursor.read_exact(&mut buf) else {
            return false;
        };
        // Bundle id 0 is used to mark the end of an entity
        if buf[0] == 0 {
            return true;
        }

        let Some(handler) = handlers.get((buf[0] - 1) as usize) else {
            return false;
        };
        (handler.1)(id_map, tick, cursor);
    }
}

/// The system that receives and processes messages, should be added by the plugin that enables
/// support for your used networking crate
pub fn receive_messages<Dir: Direction, NetRes: NetImpl<Dir>>(world: &mut World) {
    let handlers = world
        .remove_resource::<Handlers<Dir::Reverse>>()
        .expect("Missing Handlers resource");

    let mut net = world
        .remove_resource::<NetRes>()
        .expect("Missing NetRes resource");

    let channels: Vec<u8> = world.resource::<Channels>().iter().cloned().collect();

    NetRes::receive_messages(&mut net, world, &handlers, &channels);

    world.insert_resource(handlers);
    world.insert_resource(net);
}

/// The system that sends messages, should be added by the plugin that enables support for your
/// used networking crate
pub fn send_buffers<Dir: Direction, NetRes: NetImpl<Dir>>(
    mut buf: ResMut<Buffers>,
    mut net: ResMut<NetRes>,
    tick: Res<Tick>,
) {
    net.send_messages(buf.drain(*tick));
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
        Self {
            id,
            _phantom: PhantomData::<T>,
        }
    }
}

fn generate_ids<Dir: Direction>(mut commands: Commands, mut registry: ResMut<RegistryDir<Dir>>) {
    // Sort bundles by TypePath
    let mut bundles = registry
        .bundles
        .iter()
        .map(|(t, r)| (*t, r.clone()))
        .collect::<Vec<_>>();
    bundles.sort_by(|(_, a), (_, b)| a.path.cmp(b.path));

    // Assign IDs
    let mut i = 0u8;
    for (type_id, _) in bundles.iter() {
        i += 1;
        registry.bundles.get_mut(type_id).unwrap().packet_id = i;
    }

    // Sort event by TypePath
    let mut events = registry
        .events
        .iter()
        .map(|(t, r)| (*t, r.clone()))
        .collect::<Vec<_>>();
    events.sort_by(|(_, a), (_, b)| a.path.cmp(b.path));

    // Assign IDs
    let mut i = 0u8;
    for (type_id, _) in events.iter() {
        i += 1;
        registry.events.get_mut(type_id).unwrap().packet_id = i;
    }

    // Register handlers
    let mut handlers = Handlers::<Dir>::with_capacity(bundles.len(), events.len());
    for (type_id, _) in bundles.iter() {
        let bundle = registry.bundles.get(type_id).unwrap();
        handlers
            .bundles
            .push(([bundle.updater, bundle.spawner], bundle.consumer));
    }
    for (type_id, _) in events.iter() {
        handlers
            .events
            .push(registry.events.get(type_id).unwrap().handler);
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

/// A single connection
pub struct Connection {
    ident: Identity,
    new: bool,
    replicate: bool,
}

/// A list of connections
#[derive(Resource, Deref, DerefMut)]
pub struct Connections(Vec<Connection>);

fn update_connections(
    mut connected: ResMut<Connections>,
    mut buffers: ResMut<Buffers>,
    mut new: ResMut<Events<Connected>>,
    mut replicate: ResMut<Events<StartReplication>>,
    mut remove: ResMut<Events<Disconnected>>,
) {
    for Disconnected(ident) in remove.drain() {
        buffers.remove(ident);
        connected.retain(|c| c.ident != ident);
    }

    for con in connected.iter_mut() {
        if con.replicate {
            con.new = false;
        }
    }
    connected.extend(new.drain().map(|Connected(n)| Connection {
        ident: n,
        new: true,
        replicate: false,
    }));
    for StartReplication(n) in replicate.drain() {
        for con in connected.iter_mut() {
            if con.ident != n {
                continue;
            }
            con.replicate = true;
        }
    }
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
        #[cfg(feature = "renet")]
        app.add_plugins(BundlicationRenetClientPlugin);
        #[cfg(any(test, feature = "test"))]
        app.add_plugins(TestClientPlugin);

        app.insert_resource(Identity::Client(0)) // TODO: Figure out our own client id
            .add_plugins((
                tick::TickPlugin {
                    schedule: self.tick_schedule,
                },
                NetworkingPlugin {
                    despawn_channel: self.despawn_channel,
                    _phantom: PhantomData::<ClientToServer>,
                },
            ))
            .insert_resource(Connections(vec![Connection {
                ident: Identity::Server,
                new: false,
                replicate: true,
            }]));
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
        #[cfg(feature = "renet")]
        app.add_plugins(BundlicationRenetServerPlugin);
        #[cfg(any(test, feature = "test"))]
        app.add_plugins(TestServerPlugin);

        app.insert_resource(Identity::Server)
            .add_plugins((
                tick::TickPlugin {
                    schedule: self.tick_schedule,
                },
                NetworkingPlugin {
                    despawn_channel: self.despawn_channel,

                    _phantom: PhantomData::<ServerToClient>,
                },
            ))
            .insert_resource(Connections(Vec::with_capacity(20)))
            .add_systems(
                PostUpdate,
                update_connections
                    .in_set(BundlicationSet::Send)
                    .before(InternalSet::SendChanges),
            );
    }
}

#[derive(SystemSet, Clone, PartialEq, Eq, Debug, Hash)]
struct GenerateSet;

// TODO: Add better SystemSet for actual end users to schedule before/after bevy_bundlication stuff

/// A [SystemSet] containing all systems to replicate data between apps
#[derive(SystemSet, Clone, PartialEq, Eq, Debug, Hash)]
pub enum BundlicationSet {
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
    /// Send buffers
    SendBuffers,
    /// Send packets over the network
    SendPackets,
}

struct NetworkingPlugin<Dir: Direction> {
    despawn_channel: u8,
    _phantom: PhantomData<Dir>,
}

impl<Dir: Direction> Plugin for NetworkingPlugin<Dir> {
    fn build(&self, app: &mut App) {
        app.world.init_component::<Identifier>();
        app.world.init_component::<Owner>();
        app.world.init_component::<Authority>();

        app.init_resource::<Dir>()
            .init_resource::<WriteBuffer>()
            .init_resource::<Buffers>()
            .init_resource::<Channels>()
            .init_resource::<IdentifierMap>()
            .init_resource::<client_authority::HeldAuthority>()
            .init_resource::<RegistryDir<ServerToClient>>()
            .init_resource::<RegistryDir<ClientToServer>>()
            .init_resource::<Events<Connected>>()
            .init_resource::<Events<Disconnected>>()
            .init_resource::<Events<StartReplication>>()
            .insert_resource(despawn::DespawnChannel(self.despawn_channel))
            .configure_sets(
                PreUpdate,
                (InternalSet::ReadPackets, InternalSet::ReceiveMessages)
                    .chain()
                    .in_set(BundlicationSet::Receive),
            )
            .configure_sets(
                PostUpdate,
                (
                    InternalSet::SendChanges,
                    InternalSet::SendBuffers,
                    InternalSet::SendPackets,
                )
                    .chain()
                    .in_set(BundlicationSet::Send),
            )
            .add_systems(
                PostUpdate,
                (
                    despawn::send_despawns,
                    client_authority::track_authority,
                    iter::iterate_world::<Dir>,
                )
                    .chain()
                    .in_set(InternalSet::SendChanges),
            )
            .add_systems(Startup, generate_ids::<ServerToClient>.in_set(GenerateSet))
            .add_systems(Startup, generate_ids::<ClientToServer>.in_set(GenerateSet));
    }
}
