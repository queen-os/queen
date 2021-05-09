use alloc::sync::Arc;
use spin::Lazy;

mod devfs;
mod file;

pub use self::{devfs::*, file::*};
pub use queen_fs::{INode, FileSystem, FileType, FsInfo, FsError};

pub const FOLLOW_MAX_DEPTH: usize = 3;
pub static ROOT_INODE: Lazy<Arc<dyn INode>> = Lazy::new(|| todo!());
