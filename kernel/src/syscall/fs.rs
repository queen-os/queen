use super::*;
use crate::{
    drivers::read_epoch,
    fs::{FileHandle, FileType, FsError, INode, SeekFrom, FOLLOW_MAX_DEPTH, ROOT_INODE},
    process::Process,
    utils::{from_cstr, write_cstr},
};
use alloc::{string::String, vec::Vec};
use core::ptr::NonNull;
use queen_syscall::flags::{AtFlags, OpenFlags, AT_FDCWD};

impl Syscall<'_> {
    pub async fn sys_read(&mut self, fd: usize, base: usize, len: usize) -> SysResult {
        let mut process = self.process();
        let buf = unsafe { self.vm().check_write_array(base as _, len)? };
        let len = process.get_file_mut(fd)?.read(buf).await?;

        Ok(len)
    }

    pub fn sys_write(&mut self, fd: usize, base: *const u8, len: usize) -> SysResult {
        let mut process = self.process();
        let buf = unsafe { self.vm().check_read_array(base, len)? };
        let len = process.get_file_mut(fd)?.write(buf)?;

        Ok(len)
    }

    pub async fn sys_pread(&mut self, fd: usize, base: usize, len: usize, pos: usize) -> SysResult {
        let mut process = self.process();
        let buf = unsafe { self.vm().check_write_array(base as _, len)? };
        let len = process.get_file_mut(fd)?.read_at(pos, buf).await?;

        Ok(len)
    }

    pub fn sys_pwrite(&mut self, fd: usize, base: *const u8, len: usize, pos: usize) -> SysResult {
        let mut process = self.process();
        let buf = unsafe { self.vm().check_read_array(base, len)? };
        let len = process.get_file_mut(fd)?.write_at(pos, buf)?;

        Ok(len)
    }

    #[inline]
    pub fn sys_open(&mut self, path: *const u8, flags: usize, mode: usize) -> SysResult {
        self.sys_open_at(AT_FDCWD, path, flags, mode)
    }

    pub fn sys_open_at(
        &mut self,
        dir_fd: usize,
        path: *const u8,
        flags: usize,
        mode: usize,
    ) -> SysResult {
        let mut process = self.process();
        let path = unsafe { from_cstr(path) };
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

    #[inline]
    pub fn sys_close(&mut self, fd: usize) -> SysResult {
        self.process().files.remove(&fd).ok_or(SysError::EBADF)?;

        Ok(0)
    }

    #[inline]
    pub fn sys_access(&mut self, path: *const u8, mode: usize) -> SysResult {
        self.sys_faccess_at(AT_FDCWD, path, mode, 0)
    }

    pub fn sys_faccess_at(
        &mut self,
        dir_fd: usize,
        path: *const u8,
        mode: usize,
        flags: usize,
    ) -> SysResult {
        // TODO: check permissions based on uid/gid
        let proc = self.process();
        let path = unsafe { from_cstr(path) };
        let flags = AtFlags::from_bits_truncate(flags);

        let _inode =
            proc.lookup_inode_at(dir_fd, &path, !flags.contains(AtFlags::SYMLINK_NOFOLLOW))?;

        Ok(0)
    }

    pub fn sys_get_cwd(&mut self, buf: *mut u8, len: usize) -> SysResult {
        let process = self.process();
        if process.cwd.len() + 1 > len {
            return Err(SysError::ERANGE);
        }
        unsafe { write_cstr(buf, process.cwd.as_str()) }

        Ok(buf as usize)
    }

    #[inline]
    pub fn sys_read_link(&mut self, path: *const u8, base: *mut u8, len: usize) -> SysResult {
        self.sys_read_link_at(AT_FDCWD, path, base, len)
    }

    pub fn sys_read_link_at(
        &mut self,
        dir_fd: usize,
        path: *const u8,
        base: *mut u8,
        len: usize,
    ) -> SysResult {
        let proc = self.process();
        let path = unsafe { from_cstr(path) };
        let slice = unsafe { self.vm().check_write_array(base, len)? };

        let inode = proc.lookup_inode_at(dir_fd, path, false)?;
        if inode.metadata()?.r#type == FileType::SymLink {
            // TODO: recursive link resolution and loop detection
            let len = inode.read_at(0, slice)?;
            Ok(len)
        } else {
            Err(SysError::EINVAL)
        }
    }

    pub fn sys_lseek(&mut self, fd: usize, offset: i64, whence: u8) -> SysResult {
        let pos = match whence {
            SEEK_SET => SeekFrom::Start(offset as u64),
            SEEK_END => SeekFrom::End(offset),
            SEEK_CUR => SeekFrom::Current(offset),
            _ => return Err(SysError::EINVAL),
        };
        let mut process = self.process();
        let file = process.get_file(fd)?;
        let offset = file.seek(pos)?;
        Ok(offset as usize)
    }

    #[inline]
    pub fn sys_fsync(&mut self, fd: usize) -> SysResult {
        self.process().get_file(fd)?.sync_all()?;
        Ok(0)
    }

    #[inline]
    pub fn sys_fdata_sync(&mut self, fd: usize) -> SysResult {
        self.process().get_file(fd)?.sync_data()?;
        Ok(0)
    }

    pub fn sys_truncate(&mut self, path: *const u8, len: usize) -> SysResult {
        let process = self.process();
        let path = unsafe { from_cstr(path) };
        process.lookup_inode(&path)?.resize(len)?;
        Ok(0)
    }

    pub fn sys_ftruncate(&mut self, fd: usize, len: usize) -> SysResult {
        self.process().get_file(fd)?.set_len(len as u64)?;
        Ok(0)
    }

    #[inline]
    pub fn sys_dup2(&mut self, fd1: usize, fd2: usize) -> SysResult {
        self.sys_dup3(fd1, fd2, 0)
    }

    pub fn sys_dup3(&mut self, fd1: usize, fd2: usize, flags: usize) -> SysResult {
        let mut process = self.process();
        // close fd2 first if it is opened
        process.files.remove(&fd2);
        let mut file = process.get_file(fd1)?.dup(flags != 0);
        process.files.insert(fd2, file);

        Ok(fd2)
    }

    pub fn sys_chdir(&mut self, path: *const u8) -> SysResult {
        let mut process = self.process();
        let path = unsafe { from_cstr(path) };

        let inode = process.lookup_inode(&path)?;
        let info = inode.metadata()?;
        if info.r#type != FileType::Dir {
            return Err(SysError::ENOTDIR);
        }

        // FIXME: '..' and '.'
        if path.len() > 0 {
            let cwd = match path.as_bytes()[0] {
                b'/' => String::from("/"),
                _ => process.cwd.clone(),
            };
            let mut cwd_vec: Vec<_> = cwd.split("/").filter(|&x| x != "").collect();
            let path_split = path.split("/").filter(|&x| x != "");
            for seg in path_split {
                if seg == ".." {
                    cwd_vec.pop();
                } else if seg == "." {
                    // nothing to do here.
                } else {
                    cwd_vec.push(seg);
                }
            }
            process.cwd = String::from("");
            for seg in cwd_vec {
                process.cwd.push_str("/");
                process.cwd.push_str(seg);
            }
            if process.cwd == "" {
                process.cwd = String::from("/");
            }
        }
        Ok(0)
    }

    pub fn sys_rename(&mut self, old_path: *const u8, new_path: *const u8) -> SysResult {
        self.sys_rename_at(AT_FDCWD, old_path, AT_FDCWD, new_path)
    }

    pub fn sys_rename_at(
        &mut self,
        old_dir_fd: usize,
        old_path: *const u8,
        new_dir_fd: usize,
        new_path: *const u8,
    ) -> SysResult {
        let process = self.process();
        let old_path = unsafe { from_cstr(old_path) };
        let new_path = unsafe { from_cstr(new_path) };

        let (old_dir_path, old_file_name) = split_path(&old_path);
        let (new_dir_path, new_file_name) = split_path(&new_path);
        let old_dir_inode = process.lookup_inode_at(old_dir_fd, old_dir_path, false)?;
        let new_dir_inode = process.lookup_inode_at(new_dir_fd, new_dir_path, false)?;
        old_dir_inode.r#move(old_file_name, &new_dir_inode, new_file_name)?;
        Ok(0)
    }

    pub fn sys_mkdir(&mut self, path: *const u8, mode: usize) -> SysResult {
        self.sys_mkdir_at(AT_FDCWD, path, mode)
    }

    pub fn sys_mkdir_at(&mut self, dir_fd: usize, path: *const u8, mode: usize) -> SysResult {
        let proc = self.process();
        let path = unsafe { from_cstr(path) };
        // TODO: check pathname

        let (dir_path, file_name) = split_path(&path);
        let dir_inode = proc.lookup_inode_at(dir_fd, dir_path, true)?;
        if dir_inode.find(file_name).is_ok() {
            return Err(SysError::EEXIST);
        }
        let inode = dir_inode.create(file_name, FileType::Dir, mode as u32)?;
        let now = read_epoch();
        inode.update_time(now);
        dir_inode.update_time(now);

        Ok(0)
    }

    pub fn sys_rmdir(&mut self, path: *const u8) -> SysResult {
        let proc = self.process();
        let path = unsafe { from_cstr(path) };
        info!("rmdir: path: {:?}", path);

        let (dir_path, file_name) = split_path(&path);
        let dir_inode = proc.lookup_inode(dir_path)?;
        let file_inode = dir_inode.find(file_name)?;
        if file_inode.metadata()?.r#type != FileType::Dir {
            return Err(SysError::ENOTDIR);
        }
        dir_inode.unlink(file_name)?;
        Ok(0)
    }

    pub fn sys_link(&mut self, old_path: *const u8, new_path: *const u8) -> SysResult {
        self.sys_link_at(AT_FDCWD, old_path, AT_FDCWD, new_path, 0)
    }

    pub fn sys_link_at(
        &mut self,
        old_dir_fd: usize,
        old_path: *const u8,
        new_dir_fd: usize,
        new_path: *const u8,
        flags: usize,
    ) -> SysResult {
        let proc = self.process();
        let old_path = unsafe { from_cstr(old_path) };
        let new_path = unsafe { from_cstr(new_path) };
        let flags = AtFlags::from_bits_truncate(flags);

        let (new_dir_path, new_file_name) = split_path(&new_path);
        let inode = proc.lookup_inode_at(old_dir_fd, &old_path, true)?;
        let new_dir_inode = proc.lookup_inode_at(new_dir_fd, new_dir_path, true)?;
        new_dir_inode.link(new_file_name, &inode)?;
        Ok(0)
    }

    pub fn sys_unlink(&mut self, path: *const u8) -> SysResult {
        self.sys_unlink_at(AT_FDCWD, path, 0)
    }

    pub fn sys_symlink(&mut self, target: *const u8, link_path: *const u8) -> SysResult {
        self.sys_symlink_at(target, AT_FDCWD, link_path)
    }

    pub fn sys_symlink_at(
        &mut self,
        target: *const u8,
        new_dir_fd: usize,
        link_path: *const u8,
    ) -> SysResult {
        let proc = self.process();
        let target = unsafe { from_cstr(target) };
        let link_path = unsafe { from_cstr(link_path) };
        let (dir_path, filename) = split_path(&link_path);
        let dir_inode = proc.lookup_inode_at(new_dir_fd, dir_path, true)?;

        // If link_path exists, it will not be overwritten.
        match dir_inode.find(filename) {
            Ok(_) => Err(SysError::EEXIST),
            Err(e) => match e {
                FsError::EntryNotFound => {
                    let symlink = dir_inode.create(filename, FileType::SymLink, 0o777)?;
                    symlink.write_at(0, target.as_bytes())?;
                    let now = read_epoch();
                    symlink.update_time(now);
                    dir_inode.update_time(now);
                    Ok(0)
                }
                _ => Err(e.into()),
            },
        }
    }

    pub fn sys_unlink_at(&mut self, dirfd: usize, path: *const u8, flags: usize) -> SysResult {
        let proc = self.process();
        let path = unsafe { from_cstr(path) };
        let flags = AtFlags::from_bits_truncate(flags);

        let (dir_path, file_name) = split_path(&path);
        let dir_inode = proc.lookup_inode_at(dirfd, dir_path, true)?;
        let file_inode = dir_inode.find(file_name)?;
        if file_inode.metadata()?.r#type == FileType::Dir {
            return Err(SysError::EISDIR);
        }
        dir_inode.unlink(file_name)?;
        Ok(0)
    }

    pub fn sys_sync(&mut self) -> SysResult {
        ROOT_INODE.fs().sync()?;
        Ok(0)
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
