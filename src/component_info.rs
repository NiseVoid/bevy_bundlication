use crate::Tick;

use bevy::prelude::*;

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
        Self {
            tick: Tick(0),
            value,
        }
    }

    /// Get the tick the latest remote value was from
    #[inline(always)]
    pub fn tick(&self) -> Tick {
        self.tick
    }

    /// Update the value and tick for this remote value
    #[inline(always)]
    pub fn update(&mut self, tick: Tick) -> &mut T {
        self.tick = tick;
        &mut self.value
    }
}
