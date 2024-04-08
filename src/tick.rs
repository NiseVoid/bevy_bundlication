use bevy::{ecs::schedule::ScheduleLabel, prelude::*, utils::intern::Interned};

/// The current Tick of the networked simulation
#[derive(Resource, Clone, Copy, Deref, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

pub struct TickPlugin {
    pub schedule: Interned<dyn ScheduleLabel>,
}

impl Plugin for TickPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Tick>()
            .add_systems(self.schedule, increment_tick.in_set(TickSet));
    }
}

fn increment_tick(mut tick: ResMut<Tick>) {
    *tick = *tick + 1;
}
