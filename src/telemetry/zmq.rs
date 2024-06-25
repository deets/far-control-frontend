use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use log::error;

use crate::rqprotocol::Node;

use super::{Message, NRFConnector, TelemetryData};

pub struct ZMQSubscriberNRFConnector {
    nodes: Vec<Node>,
    #[allow(dead_code)]
    context: ::zmq::Context,
    socket: ::zmq::Socket,
    last_comms: HashMap<Node, Instant>,
    start: Instant,
}

impl NRFConnector for ZMQSubscriberNRFConnector {
    fn registered_nodes(&self) -> &Vec<Node> {
        &self.nodes
    }

    fn heard_from_since(&self, node: &Node) -> Duration {
        Instant::now()
            - if self.last_comms.contains_key(node) {
                self.last_comms[node]
            } else {
                self.start
            }
    }

    fn drive(&mut self) -> Vec<super::TelemetryData> {
        let mut res = vec![];
        match self.socket.recv_bytes(::zmq::DONTWAIT) {
            Ok(bytes) => {
                let s = unsafe { std::str::from_utf8_unchecked(&bytes) };
                let message: Message = serde_json::from_str(&s).unwrap();
                self.last_comms.insert(message.node, Instant::now());
                res.push(TelemetryData::Frame(message.node, message.data.into()));
            }
            Err(err) => match err {
                zmq::Error::EAGAIN => {}
                _ => {
                    error!("ZMQ ERROR{:?}", err);
                }
            },
        }
        res
    }
}

impl ZMQSubscriberNRFConnector {
    pub fn new(uri: &str) -> anyhow::Result<Self> {
        let context = ::zmq::Context::new();
        let socket = context.socket(::zmq::SUB)?;
        socket.set_subscribe(b"")?;
        socket.connect(uri)?;
        Ok(Self {
            context,
            socket,
            nodes: vec![
                Node::RedQueen(b'B'),
                Node::Farduino(b'B'),
                Node::RedQueen(b'T'),
                Node::Farduino(b'B'),
            ],
            last_comms: HashMap::new(),
            start: Instant::now(),
        })
    }
}
