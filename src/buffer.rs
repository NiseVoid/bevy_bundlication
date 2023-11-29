use crate::{Identity, Tick};

use bevy::{
    prelude::{Deref, DerefMut, Resource},
    utils::HashMap,
};

/// The rule for which client receives a message
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SendRule {
    /// Send the message to all connections
    All,
    /// Send the message to all connections, except the one for specified client id
    Except(u32),
    /// Send the message to only the one for specified client id
    Only(u32),
}

impl SendRule {
    /// Check if the [SendRule] includes the given [Identity]
    pub fn includes(&self, ident: Identity) -> bool {
        match self {
            Self::All => true,
            Self::Except(client_id) => ident != Identity::Client(*client_id),
            Self::Only(client_id) => ident == Identity::Client(*client_id),
        }
    }
}

const THRESHOLD: usize = 1100;
const CAP: usize = 1198;
const PREALLOC: usize = 1500;

/// The key of a buffer, contains information about where to send the data
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub struct BufferKey {
    /// The ID of the client the buffer is for
    pub destination: Identity,
    /// The channel the message needs to be sent on
    pub channel: u8,
}

/// A buffer used to write messages before adding them to the correct buffers
#[derive(Resource, Deref, DerefMut)]
pub struct WriteBuffer(Vec<u8>);

impl Default for WriteBuffer {
    fn default() -> Self {
        Self(Vec::with_capacity(4096))
    }
}

impl std::io::Write for WriteBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

/// A resource holding buffers with data that needs to be sent
#[derive(Resource, Default)]
pub struct Buffers {
    current: HashMap<BufferKey, Vec<u8>>,
    filled: Vec<(BufferKey, Vec<u8>)>,
    taken_cache: Option<Vec<TakenBuffer>>,
}

impl Buffers {
    /// Remove all registered buffers for the given [`Identity`]
    pub fn remove(&mut self, ident: Identity) {
        self.current.retain(|key, _| key.destination != ident);
    }

    /// Take or create buffers for the provided channel and clients and get a [`Write`]able type
    pub fn take(
        &'_ mut self,
        tick: Tick,
        channel: u8,
        this_run: bevy::ecs::component::Tick,
        targets: impl ExactSizeIterator<Item = (impl Into<Identity>, impl Into<RecipientData>)>,
    ) -> TakenBuffers<'_> {
        let mut taken = self.taken_cache.take().unwrap_or_default();
        taken.reserve_exact(targets.len());

        for (destination, info) in targets {
            let destination = destination.into();
            let buffer = self
                .current
                .remove(&BufferKey {
                    destination,
                    channel,
                })
                .unwrap_or(Vec::with_capacity(PREALLOC));
            taken.push(TakenBuffer {
                destination,
                info: info.into(),
                buffer,
                last_fragment: 0,
            });
        }

        TakenBuffers {
            this_run,
            tick: tick.to_le_bytes(),
            channel,
            buffers: self,
            taken,
            overhead: 0,
        }
    }

    /// Drain all available packets
    pub fn drain(&mut self, tick: Tick) -> impl Iterator<Item = (BufferKey, Vec<u8>)> + '_ {
        let tick = tick.to_le_bytes();
        self.current
            .iter_mut()
            .filter_map(move |(key, buf)| {
                if buf.is_empty() {
                    None
                } else {
                    let mut packet = Vec::with_capacity(buf.len() + 4);
                    packet.extend(tick);
                    packet.append(buf);
                    Some((*key, packet))
                }
            })
            .chain(self.filled.drain(..))
    }
}

/// Data about recipient
#[derive(Default)]
pub struct RecipientData {
    /// The last acknowledged Tick for this recipient
    pub last_ack: Option<bevy::ecs::component::Tick>,
}

/// Filters used when writing a message
pub struct WriteFilters {
    /// The rule for who receives the message
    pub rule: SendRule,
    /// The change tick for this message
    pub changed: bevy::ecs::component::Tick,
}

/// A buffer that was taken from [`Buffers`]
pub struct TakenBuffer {
    destination: Identity,
    info: RecipientData,
    buffer: Vec<u8>,
    last_fragment: usize,
}

/// A collection of buffers that was taken from [`Buffers`], can be used to write data to the
/// clients these buffers are for
pub struct TakenBuffers<'a> {
    this_run: bevy::ecs::component::Tick,
    tick: [u8; 4],
    channel: u8,
    buffers: &'a mut Buffers,
    taken: Vec<TakenBuffer>,
    overhead: u8,
}

impl<'a> Drop for TakenBuffers<'a> {
    fn drop(&mut self) {
        let mut taken = std::mem::take(&mut self.taken);
        for taken in taken.drain(..) {
            self.buffers.current.insert(
                BufferKey {
                    destination: taken.destination,
                    channel: self.channel,
                },
                taken.buffer,
            );
        }
        self.buffers.taken_cache = Some(taken);
    }
}

impl<'a> TakenBuffers<'a> {
    /// Increase the amount of registered overhead, if the number of bytes written since the last
    /// fragment call, the new bytes get discarded
    pub fn overhead(&mut self, overhead: u8) {
        self.overhead += overhead;
    }

    /// Send a message
    pub fn send(&mut self, rule: SendRule, buf: &mut WriteBuffer) {
        self.send_filtered(
            WriteFilters {
                rule,
                changed: self.this_run,
            },
            buf,
        );
    }

    /// Send a message with filters
    pub fn send_filtered(&mut self, filter: WriteFilters, buf: &mut WriteBuffer) {
        for taken in &mut self.taken {
            if filter.rule.includes(taken.destination)
                && (taken.info.last_ack.is_none()
                    || filter
                        .changed
                        .is_newer_than(taken.info.last_ack.unwrap(), self.this_run))
            {
                taken.buffer.extend(buf.iter());
            }
        }
        buf.clear();
    }

    /// Mark the position of the next fragment, moves filled buffers if necessary
    pub fn fragment(&mut self) {
        for taken in &mut self.taken {
            let mut len = taken.buffer.len();
            if len <= self.overhead as usize {
                taken.buffer.drain(taken.last_fragment..);
                continue;
            }
            if len < THRESHOLD {
                taken.last_fragment = len;
                continue;
            }

            while len > THRESHOLD {
                let end = if len < CAP || taken.last_fragment == 0 {
                    // If it's a single packet that's over the cap, or we aren't over the cap yet
                    // we can make the whole buffer a packet
                    taken.buffer.len()
                } else {
                    taken.last_fragment
                };

                let mut packet = Vec::with_capacity(end + 4);
                packet.extend(self.tick);
                packet.extend(taken.buffer.drain(..end));

                self.buffers.filled.push((
                    BufferKey {
                        destination: taken.destination,
                        channel: self.channel,
                    },
                    packet,
                ));

                len = taken.buffer.len();
                taken.last_fragment = 0;
            }
        }
        self.overhead = 0;
    }
}
