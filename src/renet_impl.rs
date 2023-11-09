use crate::*;
use renet::ClientId;

impl Direction for ClientToServer {
    type Reverse = ServerToClient;
    type NetRes = RenetClient;

    fn receive_messages(
        net: &mut Self::NetRes,
        world: &mut World,
        handlers: &Handlers<Self::Reverse>,
        channels: &[u8],
    ) {
        for channel in channels {
            while let Some(msg) = net.receive_message(*channel) {
                handlers.process(world, Identity::Server, &msg);
            }
        }
    }

    fn send_messages(net: &mut Self::NetRes, msgs: std::vec::Drain<(BufferKey, Vec<u8>)>) {
        for (BufferKey { channel, rule: _ }, buf) in msgs {
            net.send_message(channel, buf);
        }
    }
}

impl Direction for ServerToClient {
    type Reverse = ClientToServer;
    type NetRes = RenetServer;

    fn receive_messages(
        net: &mut Self::NetRes,
        world: &mut World,
        handlers: &Handlers<Self::Reverse>,
        channels: &[u8],
    ) {
        for client_id in net.clients_id() {
            for channel in channels {
                while let Some(msg) = net.receive_message(client_id, *channel) {
                    handlers.process(world, Identity::Client(client_id.raw() as u32), &msg);
                }
            }
        }
    }

    fn send_messages(net: &mut Self::NetRes, msgs: std::vec::Drain<(BufferKey, Vec<u8>)>) {
        for (BufferKey { channel, rule }, buf) in msgs {
            match rule {
                SendRule::All => {
                    net.broadcast_message(channel, buf);
                }
                SendRule::Except(client_id) => {
                    net.broadcast_message_except(
                        ClientId::from_raw(client_id as u64),
                        channel,
                        buf,
                    );
                }
                SendRule::Only(client_id) => {
                    net.send_message(ClientId::from_raw(client_id as u64), channel, buf);
                }
                SendRule::List(list) => {
                    let buf = renet::Bytes::from(buf);
                    for client_id in list {
                        net.send_message(
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
