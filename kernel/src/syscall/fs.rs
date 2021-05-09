use super::*;
use crate::{
    fs::{FileHandle, FileType, FsError, INode, FOLLOW_MAX_DEPTH, ROOT_INODE},
    process::Process,
    TimeSpec,
};
use core::ptr::NonNull;
use queen_syscall::flags::OpenFlags;
use alloc::{string::{String, ToString}};

impl Syscall<'_> {
    pub async fn sys_read(&mut self, fd: usize, base: NonNull<u8>, len: usize) -> SysResult {
        let mut process = self.process();
        let buf = unsafe { self.vm().check_write_array(base.as_ptr(), len)? };
        let len = process.get_file_mut(fd)?.read(buf).await?;

        Ok(len)
    }

    pub fn sys_write(&mut self, fd: usize, base: NonNull<u8>, len: usize) -> SysResult {
        let mut process = self.process();
        let buf = unsafe { self.vm().check_read_array(base.as_ptr(), len)? };
        let len = process.get_file_mut(fd)?.write(buf)?;

        Ok(len)
    }

    pub async fn sys_pread(
        &mut self,
        fd: usize,
        base: NonNull<u8>,
        len: usize,
        pos: usize,
    ) -> SysResult {
        let mut process = self.process();
        let buf = unsafe { self.vm().check_write_array(base.as_ptr(), len)? };
        let len = process.get_file_mut(fd)?.read_at(pos, buf).await?;

        Ok(len)
    }

    pub fn sys_pwrite(
        &mut self,
        fd: usize,
        base: NonNull<u8>,
        len: usize,
        pos: usize,
    ) -> SysResult {
        let mut process = self.process();
        let buf = unsafe { self.vm().check_read_array(base.as_ptr(), len)? };
        let len = process.get_file_mut(fd)?.write_at(pos, buf)?;

        Ok(len)
    }

    #[inline]
    pub fn sys_open(&mut self, path: NonNull<u8>, flags: usize, mode: usize) -> SysResult {
        self.sys_open_at(AT_FDCWD, path, flags, mode)
    }

    pub fn sys_open_at(
        &mut self,
        dir_fd: usize,
        path: NonNull<u8>,
        flags: usize,
        mode: usize,
    ) -> SysResult {
        let mut process = self.process();
        let path = parse_cstr(path)?;
        let flags = OpenFlags::from_bits_truncate(flags);

        let inode = if flags.contains(OpenFlags::CREATE) {
            let (dir_path, file_name) = split_path(path);
            // relative to cwd
            let dir_inode = process.lookup_inode_at(dir_fd, dir_path, true)?;
            match dir_inode.find(file_name) {
                Ok(file_inode) => {
                    if flags.contains(OpenFlags::EXCLUSIVE) {
                        return Err(SysError::EEXIST);
                    }
                    if flags.contains(OpenFlags::TRUNCATE) {
                        file_inode.resize(0).ok();
                    }
                    file_inode
                }
                Err(FsError::EntryNotFound) => {
                    let inode = dir_inode.create(file_name, FileType::File, mode as u32)?;
                    let now = crate::drivers::read_epoch();
                    inode.update_time(now);
                    dir_inode.update_time(now);
                    inode
                }
                Err(e) => return Err(SysError::from(e)),
            }
        } else {
            process.lookup_inode_at(dir_fd, &path, true)?
        };

        let file = FileHandle::new(
            inode,
            flags.into(),
            path.into(),
            flags.contains(OpenFlags::CLOEXEC),
        );
        let fd = process.add_file(file);

        Ok(fd)
    }
}

impl Process {
    #[inline]
    pub fn get_file_mut(&mut self, fd: usize) -> Result<&mut FileHandle, SysError> {
        self.files.get_mut(&fd).ok_or(SysError::EBADF)
    }

    #[inline]
    pub fn get_file(&self, fd: usize) -> Result<&FileHandle, SysError> {
        self.files.get(&fd).ok_or(SysError::EBADF)
    }

    /// Lookup INode from the process.
    ///
    /// - If `path` is relative, then it is interpreted relative to the directory
    ///   referred to by the file descriptor `dirfd`.
    ///
    /// - If the `dirfd` is the special value `AT_FDCWD`, then the directory is
    ///   current working directory of the process.
    ///
    /// - If `path` is absolute, then `dirfd` is ignored.
    ///
    /// - If `follow` is true, then dereference `path` if it is a symbolic link.
    pub fn lookup_inode_at(
        &self,
        dir_fd: usize,
        path: &str,
        follow: bool,
    ) -> Result<Arc<dyn INode>, SysError> {
        debug!(
            "lookup_inode_at: dirfd: {:?}, cwd: {:?}, path: {:?}, follow: {:?}",
            dir_fd as isize, self.cwd, path, follow
        );

        let (fd_dir_path, fd_name) = split_path(&path);

        let follow_max_depth = if follow { FOLLOW_MAX_DEPTH } else { 0 };
        if dir_fd == AT_FDCWD {
            Ok(ROOT_INODE
                .lookup(&self.cwd)?
                .lookup_follow(path, follow_max_depth)?)
        } else {
            Ok(self
                .get_file(dir_fd)?
                .lookup_follow(path, follow_max_depth)?)
        }
    }

    #[inline]
    pub fn lookup_inode(&self, path: &str) -> Result<Arc<dyn INode>, SysError> {
        self.lookup_inode_at(AT_FDCWD, path, true)
    }
}

/// Split a `path` str to `(base_path, file_name)`
fn split_path(path: &str) -> (&str, &str) {
    let mut split = path.trim_end_matches('/').rsplitn(2, '/');
    let file_name = split.next().unwrap();
    let mut dir_path = split.next().unwrap_or(".");
    if dir_path == "" {
        dir_path = "/";
    }
    (dir_path, file_name)
}

/// Pathname is interpreted relative to the current working directory(CWD)
const AT_FDCWD: usize = -100isize as usize;
