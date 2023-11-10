use crate::{NetworkedBundle, Tick};

use std::marker::PhantomData;

use bevy::prelude::*;

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
        Self {
            tick,
            _phantom: PhantomData::<T>,
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
