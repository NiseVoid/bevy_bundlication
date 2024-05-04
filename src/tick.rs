use bevy::prelude::*;
use bevy_replicon::server::replicon_tick::RepliconTick;
use serde::{Deserialize, Serialize};

/// The current Tick of the networked simulation
#[rustfmt::skip]
#[derive(Resource,
    Clone, Copy,
    Deref, DerefMut,
    Default, Debug,
    PartialEq, Eq,
    PartialOrd, Ord,
    Hash,
    Serialize, Deserialize
)]
pub struct Tick(pub u32);

impl From<RepliconTick> for Tick {
    fn from(value: RepliconTick) -> Self {
        Tick(value.get())
    }
}

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
