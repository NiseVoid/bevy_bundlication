#![doc = include_str!("../README.md")]
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

    /// Read the component from the network, using the [`DeserializeCtx`] to convert any necessary values
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
    /// Write the component to the network, using the [`SerializeCtx`] to convert any necessary values
    fn write_data(from: &From, w: impl Write, ctx: &SerializeCtx) -> BincodeResult<()>;

    /// Read the component from the network, using [`DeserializeCtx`] to convert any necessary values
    fn read_new(r: impl Read, ctx: &mut DeserializeCtx) -> BincodeResult<From>;

    /// Read the component in-place from the network, avoiding creation of a new value
    fn read_in_place(from: &mut From, r: impl Read, ctx: &mut DeserializeCtx) -> BincodeResult<()> {
        *from = Self::read_new(r, ctx)?;
        Ok(())
    }
}
