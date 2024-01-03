#[derive(Debug, Copy, Clone)]
pub enum Event {
    Enter,
    Back,
    Left(u32),
    Right(u32),
}
