//! Network replication based on bundles.
//!
//! Replication logic can be added to your app using [`AppNetworkingExt`].
//!
//! You can register bundles with [`AppNetworkingExt::register_bundle`], if the direction matches the
//! current app, any entity matching this bundle with an [`Identifier`] will then be sent over the network.
//! If the App is a client, it will only send packets if we have or can claim [`Authority`].
//! Direct updating of components can be avoided by adding the [`Remote`] on the entity, when this
//! component is around values will be stored there instead of the real field.
//!
//! You can register events with [`AppNetworkingExt::register_event`]. Events will be sent if the
//! direction matches, on the receiving side events are wrapped in [`NetworkEvent`]

#![warn(missing_docs)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

mod tick;

pub mod prelude {
    //! The prelude of the crate, contains everything necessary to get started with this crate

    pub use super::BincodeResult;
    pub use crate::{deserialize, serialize, tick::Tick, NetworkedComponent, NetworkedWrapper};
    pub use bevy_bundlication_macros::NetworkedBundle;
    pub use bevy_replicon::core::replication_fns::ctx::{SerializeCtx, WriteCtx as DeserializeCtx};
    pub use bincode::{Error as BincodeError, ErrorKind as BincodeErrorKind};
}

pub mod macro_export {
    //! A module with exports used by the macro

    pub use crate::{deserialize, serialize, tick::Tick, NetworkedComponent, NetworkedWrapper};
    pub use bevy::ecs::world::World;
    pub use bevy_replicon::core::{
        replication_fns::{
            ctx::{SerializeCtx, WriteCtx as DeserializeCtx},
            rule_fns::{DeserializeFn, RuleFns},
            ReplicationFns,
        },
        replication_rules::{GroupReplication, ReplicationRule},
    };
    pub use bincode;
    pub use std::io::Cursor;
}

use std::io::{Read, Write};

use bevy::prelude::*;
use prelude::{DeserializeCtx, SerializeCtx};

pub use bincode::{deserialize_from as deserialize, serialize_into as serialize};
use serde::{Deserialize, Serialize};

/// An alias for bincode's Result type
pub type BincodeResult<T> = bincode::Result<T>;

// TODO: Change error handling. Reads should not be forced to resort to panics
/// A trait needed to network components, provided by a blanket impl if the component has
/// Serialize+Deserialize
pub trait NetworkedComponent: Sized {
    /// Write the component to the network, using the [`SerializeCtx`] to convert any necessary values
    fn write_data(&self, w: impl Write, ctx: &SerializeCtx) -> BincodeResult<()>;

    /// Read the component from the network, using the [`Context`] to convert any necessary values
    fn read_new(r: impl Read, ctx: &mut DeserializeCtx) -> BincodeResult<Self>;

    /// Read the component in-place from the network, this can be used to write directly to
    fn read_in_place(&mut self, r: impl Read, ctx: &mut DeserializeCtx) -> BincodeResult<()> {
        *self = Self::read_new(r, ctx)?;
        Ok(())
    }
}

impl<T: Component + Serialize + for<'a> Deserialize<'a>> NetworkedComponent for T {
    fn write_data(&self, w: impl Write, _: &SerializeCtx) -> BincodeResult<()> {
        serialize(w, self).unwrap();
        Ok(())
    }

    fn read_new(r: impl Read, _: &mut DeserializeCtx) -> BincodeResult<Self> {
        Ok(deserialize(r)?)
    }
}

/// A trait that allows wrapping a component as another type for bevy_bundlication. Useful when working
/// with components from bevy itself or 3rd party plugins
pub trait NetworkedWrapper<From: Component> {
    /// Write the component to the network, using the current [`Tick`] and [`IdentifierMap`] to
    /// convert any necessary values
    fn write_data(from: &From, w: impl Write, ctx: &SerializeCtx) -> BincodeResult<()>;

    /// Read the component from the network, using the [`Tick`] of the packet it was contained
    /// in and the [`ClientMapper`] to convert any necessary values
    fn read_new(r: impl Read, ctx: &mut DeserializeCtx) -> BincodeResult<From>;

    /// Read the component in-place from the network, this can be used to write directly to
    fn read_in_place(from: &mut From, r: impl Read, ctx: &mut DeserializeCtx) -> BincodeResult<()> {
        *from = Self::read_new(r, ctx)?;
        Ok(())
    }
}
