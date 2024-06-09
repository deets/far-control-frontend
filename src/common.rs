use std::time::Duration;

use crate::rqprotocol::Node;

pub trait NRFStatusReporter {
    fn registered_nodes(&self) -> &Vec<Node>;
    fn heard_from_since(&self, node: &Node) -> Duration;
}

pub struct FakeNRFStatusReporter {
    nodes: Vec<Node>,
}

impl NRFStatusReporter for FakeNRFStatusReporter {
    fn registered_nodes(&self) -> &Vec<Node> {
        &self.nodes
    }

    fn heard_from_since(&self, node: &Node) -> Duration {
        match node {
            Node::RedQueen(b'B') => Duration::from_secs(5),
            Node::RedQueen(b'T') => Duration::from_secs(1),
            Node::Farduino(_) => Duration::from_secs(20),
            _ => Duration::from_secs(1000),
        }
    }
}

impl Default for FakeNRFStatusReporter {
    fn default() -> Self {
        Self {
            nodes: vec![
                Node::RedQueen(b'B'),
                Node::Farduino(b'B'),
                Node::RedQueen(b'T'),
                Node::Farduino(b'B'),
            ],
        }
    }
}
