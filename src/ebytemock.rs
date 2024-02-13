use crate::connection::Connection;

pub struct E32Connection {}

impl Connection for E32Connection {
    fn recv(&mut self, _callback: impl FnOnce(crate::connection::Answers)) {}
}

impl std::io::Write for E32Connection {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl E32Connection {
    pub fn new(_port: &str) -> anyhow::Result<E32Connection> {
        Ok(Self {})
    }
}
