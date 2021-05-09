use alloc::{string::String, sync::Arc};
use core::fmt;
use queen_fs::vfs::{ FsError, INode, Metadata, PollStatus, Result};
use spin::RwLock;

enum Flock {
    None = 0,
    Shared = 1,
    Exclusive = 2,
}

struct OpenFileDescription {
    offset: u64,
    options: OpenOptions,
    flock: Flock,
}

impl OpenFileDescription {
    fn create(options: OpenOptions) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(OpenFileDescription {
            offset: 0,
            options,
            flock: Flock::None,
        }))
    }
}

#[derive(Clone)]
pub struct FileHandle {
    inode: Arc<dyn INode>,
    description: Arc<RwLock<OpenFileDescription>>,
    pub path: String,
    pub fd_cloexec: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct OpenOptions {
    pub read: bool,
    pub write: bool,
    /// Before each write, the file offset is positioned at the end of the file.
    pub append: bool,
}

impl From<queen_syscall::flags::OpenFlags> for OpenOptions {
    fn from(flag: queen_syscall::flags::OpenFlags) -> Self {
        OpenOptions {
            read: flag.readable(),
            write: flag.writable(),
            append: flag.is_append(),
        }
    }
}

#[derive(Debug)]
pub enum SeekFrom {
    Start(u64),
    End(i64),
    Current(i64),
}

impl FileHandle {
    pub fn new(
        inode: Arc<dyn INode>,
        options: OpenOptions,
        path: String,
        fd_cloexec: bool,
    ) -> Self {
        return FileHandle {
            inode,
            description: OpenFileDescription::create(options),
            path,
            fd_cloexec,
        };
    }

    // do almost as default clone does, but with fd_cloexec specified
    pub fn dup(&self, fd_cloexec: bool) -> Self {
        FileHandle {
            inode: self.inode.clone(),
            description: self.description.clone(),
            path: self.path.clone(),
            fd_cloexec, // this field do not share
        }
    }

    pub fn set_options(&self, arg: usize) {}

    // pub fn get_options(&self) -> usize {
    // let options = self.description.read().options;
    // let mut ret = 0 as usize;
    // }

    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let offset = self.description.read().offset as usize;
        let len = self.read_at(offset, buf).await?;
        self.description.write().offset += len as u64;
        Ok(len)
    }

    pub async fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize> {
        // let options = &self.description.read().options;
        if !self.description.read().options.read {
            return Err(FsError::InvalidParam); // TODO: => EBADF
        }
        // block
        loop {
            match self.inode.read_at(offset, buf) {
                Ok(read_len) => {
                    return Ok(read_len);
                }
                Err(FsError::Again) => {
                    self.async_poll().await?;
                }
                Err(err) => {
                    return Err(err);
                }
            }
        }
    }

    pub fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let description = self.description.read();
        let offset = match description.options.append {
            true => self.inode.metadata()?.size as u64,
            false => description.offset,
        } as usize;
        drop(description);
        let len = self.write_at(offset, buf)?;
        self.description.write().offset += len as u64;
        Ok(len)
    }

    pub fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize> {
        if !self.description.read().options.write {
            return Err(FsError::InvalidParam);
        }
        let len = self.inode.write_at(offset, buf)?;
        // TimeSpec::update(&self.inode);
        Ok(len)
    }

    pub fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        let mut description = self.description.write();
        description.offset = match pos {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => (self.inode.metadata()?.size as i64 + offset) as u64,
            SeekFrom::Current(offset) => (description.offset as i64 + offset) as u64,
        };
        Ok(description.offset)
    }

    pub fn set_len(&mut self, len: u64) -> Result<()> {
        if !self.description.read().options.write {
            return Err(FsError::InvalidParam);
        }
        self.inode.resize(len as usize)?;
        Ok(())
    }

    pub fn sync_all(&mut self) -> Result<()> {
        self.inode.sync_all()
    }

    pub fn sync_data(&mut self) -> Result<()> {
        self.inode.sync_data()
    }

    pub fn metadata(&self) -> Result<Metadata> {
        self.inode.metadata()
    }

    pub fn lookup_follow(&self, path: &str, max_follow: usize) -> Result<Arc<dyn INode>> {
        self.inode.lookup_follow(path, max_follow)
    }

    pub fn read_entry(&mut self) -> Result<String> {
        let mut description = self.description.write();
        if !description.options.read {
            return Err(FsError::InvalidParam); // TODO: => EBADF
        }
        let offset = &mut description.offset;
        let name = self.inode.get_entry(*offset as usize)?;
        *offset += 1;
        Ok(name)
    }

    pub fn read_entry_with_metadata(&mut self) -> Result<(Metadata, String)> {
        let mut description = self.description.write();
        if !description.options.read {
            return Err(FsError::InvalidParam); // TODO: => EBADF
        }
        let offset = &mut description.offset;
        let ret = self.inode.get_entry_with_metadata(*offset as usize)?;
        *offset += 1;
        Ok(ret)
    }

    pub fn poll(&self) -> Result<PollStatus> {
        self.inode.poll()
    }

    pub async fn async_poll(&self) -> Result<PollStatus> {
        self.inode.async_poll().await
    }

    pub fn io_control(&self, cmd: u32, arg: usize) -> Result<usize> {
        self.inode.io_control(cmd, arg)
    }

    pub fn inode(&self) -> Arc<dyn INode> {
        self.inode.clone()
    }
}

impl fmt::Debug for FileHandle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let description = self.description.read();
        return f
            .debug_struct("FileHandle")
            .field("offset", &description.offset)
            .field("options", &description.options)
            .field("path", &self.path)
            .finish();
    }
}
