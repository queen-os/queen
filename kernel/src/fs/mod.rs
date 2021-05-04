use alloc::sync::Arc;
use queen_fs::INode;
use spin::Lazy;

mod file;
mod devfs;

pub use self::file::*;
pub use self::devfs::*;

pub const FOLLOW_MAX_DEPTH: usize = 3;
pub static ROOT_INODE: Lazy<Arc<dyn INode>> = Lazy::new(|| todo!());
