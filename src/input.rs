#[derive(Debug, Copy, Clone)]
pub enum InputEvent {
    Enter,
    Back,
    Left(u32),
    Right(u32),
    Send,
}
