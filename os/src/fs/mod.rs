mod pipe;
mod stdio;

use crate::mm::UserBuffer;
pub trait File: Send + Sync {
    fn read(&self, buf: UserBuffer) -> usize;
    fn write(&self, buf: UserBuffer) -> usize;
}

pub use pipe::{make_pipe, Pipe};
pub use stdio::{Stdin, Stdout};
