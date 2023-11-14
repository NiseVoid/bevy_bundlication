use crate::*;

use bevy::prelude::resource_exists;
use bevy_renet::{
    renet::{Bytes, ClientId, RenetClient, RenetServer},
    {RenetReceive, RenetSend},
};

/// A plugin that adds renet support to a server
pub struct BundlicationRenetServerPlugin;

impl Plugin for BundlicationRenetServerPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            PreUpdate,
            (
                RenetReceive.in_set(InternalSet::ReadPackets),
                BundlicationSet::Receive.run_if(resource_exists::<RenetServer>()),
            ),
        )
        .configure_sets(
            PostUpdate,
            (
                RenetSend.in_set(InternalSet::SendPackets),
                BundlicationSet::Send.run_if(resource_exists::<RenetServer>()),
            ),
        )
        .add_systems(
            PreUpdate,
            receive_messages::<ServerToClient, RenetServer>.in_set(InternalSet::ReceiveMessages),
        )
        .add_systems(
            PostUpdate,
            send_buffers::<ServerToClient, RenetServer>.in_set(InternalSet::SendBuffers),
        );
    }
}

/// A plugin that adds renet support to a client
pub struct BundlicationRenetClientPlugin;

impl Plugin for BundlicationRenetClientPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            PreUpdate,
            (
                RenetReceive.in_set(InternalSet::ReadPackets),
                BundlicationSet::Receive.run_if(resource_exists::<RenetClient>()),
            ),
        )
        .configure_sets(
            PostUpdate,
            (
                RenetSend.in_set(InternalSet::SendPackets),
                BundlicationSet::Send.run_if(resource_exists::<RenetClient>()),
            ),
        )
        .add_systems(
            PreUpdate,
            receive_messages::<ClientToServer, RenetClient>.in_set(InternalSet::ReceiveMessages),
        )
        .add_systems(
            PostUpdate,
            send_buffers::<ClientToServer, RenetClient>.in_set(InternalSet::SendBuffers),
        );
    }
}

impl<Dir: Direction> NetImpl<Dir> for RenetClient {
    fn receive_messages(
        &mut self,
        world: &mut World,
        handlers: &Handlers<Dir::Reverse>,
        channels: &[u8],
    ) {
        for channel in channels {
            while let Some(msg) = self.receive_message(*channel) {
                handlers.process(world, Identity::Server, &msg);
            }
        }
    }

    fn send_messages(&mut self, msgs: std::vec::Drain<(BufferKey, Vec<u8>)>) {
        for (BufferKey { channel, rule: _ }, buf) in msgs {
            self.send_message(channel, buf);
        }
    }
}

impl<Dir: Direction> NetImpl<Dir> for RenetServer {
    fn receive_messages(
        &mut self,
        world: &mut World,
        handlers: &Handlers<Dir::Reverse>,
        channels: &[u8],
    ) {
        for client_id in self.clients_id() {
            for channel in channels {
                while let Some(msg) = self.receive_message(client_id, *channel) {
                    handlers.process(world, Identity::Client(client_id.raw() as u32), &msg);
                }
            }
        }
    }

    fn send_messages(&mut self, msgs: std::vec::Drain<(BufferKey, Vec<u8>)>) {
        for (BufferKey { channel, rule }, buf) in msgs {
            match rule {
                SendRule::All => {
                    self.broadcast_message(channel, buf);
                }
                SendRule::Except(client_id) => {
                    self.broadcast_message_except(
                        ClientId::from_raw(client_id as u64),
                        channel,
                        buf,
                    );
                }
                SendRule::Only(client_id) => {
                    self.send_message(ClientId::from_raw(client_id as u64), channel, buf);
                }
                SendRule::List(list) => {
                    let buf = Bytes::from(buf);
                    for client_id in list {
                        self.send_message(
                            ClientId::from_raw(client_id as u64),
                            channel,
                            buf.clone(),
                        );
                    }
                }
            }
        }
    }
}
