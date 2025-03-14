#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

pub mod prelude {
    //! The prelude of the crate, contains everything necessary to get started with this crate

    pub use super::PostcardResult;
    pub use crate::{deserialize, serialize, NetworkedComponent, NetworkedWrapper};
    pub use bevy_bundlication_macros::NetworkedBundle;
    pub use bevy_replicon::core::replication::replication_registry::ctx::{
        SerializeCtx, WriteCtx as DeserializeCtx,
    };
    pub use postcard::Error as PostcardError;
}

pub mod macro_export {
    //! A module with exports used by the macro

    pub use crate::{deserialize, serialize, NetworkedComponent, NetworkedWrapper};
    pub use bevy::ecs::world::World;
    pub use bevy_replicon::bytes::Bytes;
    pub use bevy_replicon::core::replication::{
        replication_registry::{
            ctx::{SerializeCtx, WriteCtx as DeserializeCtx},
            rule_fns::{DeserializeFn, RuleFns},
            ReplicationRegistry,
        },
        replication_rules::{GroupReplication, ReplicationRule},
    };
    pub use postcard;
}

use std::io::{Read, Write};

use bevy::prelude::*;
use prelude::{DeserializeCtx, SerializeCtx};

use serde::{Deserialize, Serialize};

/// An alias for postcard's Result type
pub type PostcardResult<T> = postcard::Result<T>;

/// Deserialize an instance of the specified type from the provided reader
pub fn deserialize<R, T>(r: R) -> PostcardResult<T>
where
    R: Read,
    T: serde::de::DeserializeOwned,
{
    postcard::from_io((r, &mut [0; 1500])).map(|(t, _)| t)
}

/// Serialize the provided value into the writer
pub fn serialize<T, W>(w: W, t: &T) -> PostcardResult<()>
where
    W: Write,
    T: Serialize + ?Sized,
{
    postcard::to_io(t, w).map(|_| ())
}

// TODO: Change error handling. Reads should not be forced to resort to panics
/// A trait needed to network components, provided by a blanket impl if the component has
/// Serialize+Deserialize
pub trait NetworkedComponent: Sized {
    /// Write the component to the network, using the [`SerializeCtx`] to convert any necessary values
    fn write_data(&self, w: impl Write, ctx: &SerializeCtx) -> PostcardResult<()>;

    /// Read the component from the network, using the [`DeserializeCtx`] to convert any necessary values
    fn read_new(r: impl Read, ctx: &mut DeserializeCtx) -> PostcardResult<Self>;

    /// Read the component in-place from the network, this can be used to write directly to
    fn read_in_place(&mut self, r: impl Read, ctx: &mut DeserializeCtx) -> PostcardResult<()> {
        *self = Self::read_new(r, ctx)?;
        Ok(())
    }
}

impl<T: Component + Serialize + for<'a> Deserialize<'a>> NetworkedComponent for T {
    fn write_data(&self, w: impl Write, _: &SerializeCtx) -> PostcardResult<()> {
        serialize(w, self).unwrap();
        Ok(())
    }

    fn read_new(r: impl Read, _: &mut DeserializeCtx) -> PostcardResult<Self> {
        deserialize(r)
    }
}

/// A trait that allows wrapping a component as another type for bevy_bundlication. Useful when working
/// with components from bevy itself or 3rd party plugins
pub trait NetworkedWrapper<From: Component> {
    /// Write the component to the network, using the [`SerializeCtx`] to convert any necessary values
    fn write_data(from: &From, w: impl Write, ctx: &SerializeCtx) -> PostcardResult<()>;

    /// Read the component from the network, using [`DeserializeCtx`] to convert any necessary values
    fn read_new(r: impl Read, ctx: &mut DeserializeCtx) -> PostcardResult<From>;

    /// Read the component in-place from the network, avoiding creation of a new value
    fn read_in_place(
        from: &mut From,
        r: impl Read,
        ctx: &mut DeserializeCtx,
    ) -> PostcardResult<()> {
        *from = Self::read_new(r, ctx)?;
        Ok(())
    }
}
