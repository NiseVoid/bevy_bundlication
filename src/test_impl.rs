use crate::*;

/// The input and output for a client
#[derive(Resource, Default)]
pub struct ClientMessages {
    /// The input, a list of packet bytes
    pub input: Vec<Vec<u8>>,
    /// The output, a list of channel and bytes
    pub output: Vec<(u8, Vec<u8>)>,
}

impl Direction for ClientToServer {
    type Reverse = ServerToClient;
    type NetRes = ClientMessages;

    fn receive_messages(
        net: &mut Self::NetRes,
        world: &mut World,
        handlers: &Handlers<Self::Reverse>,
        _: &[u8],
    ) {
        for b in net.input.drain(..) {
            handlers.process(world, Identity::Server, &b);
        }
    }

    fn send_messages(net: &mut Self::NetRes, msgs: std::vec::Drain<(BufferKey, Vec<u8>)>) {
        for (BufferKey { channel, rule: _ }, buf) in msgs {
            net.output.push((channel, buf));
        }
    }
}

/// The input and output for a server
#[derive(Resource, Default)]
pub struct ServerMessages {
    /// The input, a list of client id and bytes
    pub input: Vec<(u32, Vec<u8>)>,
    /// The output, a list of channel, send rules and bytes
    pub output: Vec<(u8, SendRule, Vec<u8>)>,
}

impl Direction for ServerToClient {
    type Reverse = ClientToServer;
    type NetRes = ServerMessages;

    fn receive_messages(
        net: &mut Self::NetRes,
        world: &mut World,
        handlers: &Handlers<Self::Reverse>,
        _: &[u8],
    ) {
        for (client_id, b) in net.input.drain(..) {
            handlers.process(world, Identity::Client(client_id), &b);
        }
    }

    fn send_messages(net: &mut Self::NetRes, msgs: std::vec::Drain<(BufferKey, Vec<u8>)>) {
        for (BufferKey { channel, rule }, buf) in msgs {
            net.output.push((channel, rule, buf));
        }
    }
}
