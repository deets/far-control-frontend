use crate::rqprotocol::Node;
use ::zmq::{Context, Socket};
use log::error;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use std::{cell::RefCell, rc::Rc};

use self::parser::rq2::{packet_parser, TelemetryPacket};

#[cfg(feature = "novaview")]
pub mod nrf;
#[cfg(not(feature = "novaview"))]
pub mod zmq;

pub mod parser;

#[derive(Serialize, Deserialize)]
pub struct Message {
    pub node: Node,
    pub data: [u8; 32],
}

#[derive(Clone)]
pub enum RawTelemetryPacket {
    Frame(Node, Vec<u8>),
    NoModule(Node),
}

pub trait NRFConnector {
    fn registered_nodes(&self) -> &Vec<Node>;
    fn heard_from_since(&self, node: &Node) -> Duration;
    fn drive(&mut self) -> Vec<RawTelemetryPacket>;
}

#[cfg(not(feature = "novaview"))]
pub fn create() -> Rc<RefCell<dyn NRFConnector>> {
    Rc::new(RefCell::new(
        zmq::ZMQSubscriberNRFConnector::new("tcp://novaview.local:2424").unwrap(),
    ))
}

#[cfg(feature = "novaview")]
pub fn create() -> Rc<RefCell<dyn NRFConnector>> {
    let telemetry = nrf::TelemetryFrontend::new(nrf::DEFAULT_CONFIGURATION.into_iter()).unwrap();
    Rc::new(RefCell::new(telemetry))
}

pub struct ZMQPublisher {
    #[allow(dead_code)]
    context: Context,
    socket: Socket,
    pub count: usize,
}

impl ZMQPublisher {
    pub fn new(uri: &str) -> anyhow::Result<Self> {
        let context = Context::new();
        let socket = context.socket(::zmq::PUB)?;
        socket.bind(uri)?;
        Ok(Self {
            context,
            socket,
            count: 0,
        })
    }

    pub fn publish_telemetry_data(&mut self, messages: &Vec<RawTelemetryPacket>) {
        for data in messages.into_iter() {
            match data {
                RawTelemetryPacket::Frame(node, data) => {
                    self.count += data.len();
                    let message = Message {
                        node: *node,
                        data: (*data).clone().try_into().unwrap(),
                    };

                    let j = serde_json::to_string(&message).unwrap();
                    let _ = self.socket.send(&j.as_bytes(), 0);
                }
                RawTelemetryPacket::NoModule(_) => {}
            }
        }
    }
}

pub fn process_raw_telemetry_data(raw: &Vec<RawTelemetryPacket>) -> Vec<TelemetryPacket> {
    let mut res = vec![];
    for packet in raw.into_iter() {
        match packet {
            RawTelemetryPacket::Frame(node, data) => match packet_parser(*node, data) {
                Ok((_, packet)) => {
                    res.push(packet);
                }
                Err(err) => {
                    error!("telemetry packet error: {:?}", err);
                }
            },
            RawTelemetryPacket::NoModule(_) => todo!(),
        }
    }
    res
}
