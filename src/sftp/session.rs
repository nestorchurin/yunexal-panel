use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version,
};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

pub struct SftpSession {
    root: PathBuf,
    open_files: HashMap<String, fs::File>,
    open_dirs: HashMap<String, Vec<fs::DirEntry>>,
    next_handle: u64,
}

impl SftpSession {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            open_files: HashMap::new(),
            open_dirs: HashMap::new(),
            next_handle: 0,
        }
    }

    fn alloc_handle(&mut self) -> String {
        self.next_handle += 1;
        self.next_handle.to_string()
    }

    /// Resolve a client-supplied path safely within the SFTP root.
    fn resolve(&self, path: &str) -> Result<PathBuf, StatusCode> {
        let rel = path.trim_start_matches('/');
        let joined = self.root.join(rel);

        let mut out = PathBuf::new();
        for component in joined.components() {
            match component {
                Component::ParentDir => { out.pop(); }
                Component::CurDir => {}
                c => out.push(c),
            }
        }

        if !out.starts_with(&self.root) {
            return Err(StatusCode::PermissionDenied);
        }
        Ok(out)
    }

    fn strip_root(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .map(|p| format!("/{}", p.display()))
            .unwrap_or_else(|_| "/".to_string())
    }
}

fn ok_status(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".to_string(),
        language_tag: "en-US".to_string(),
    }
}

fn io_to_sftp(e: &std::io::Error) -> StatusCode {
    match e.kind() {
        std::io::ErrorKind::NotFound => StatusCode::NoSuchFile,
        std::io::ErrorKind::PermissionDenied => StatusCode::PermissionDenied,
        _ => StatusCode::Failure,
    }
}

impl russh_sftp::server::Handler for SftpSession {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<Version, Self::Error> {
        Ok(Version::new())
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        let resolved = self.resolve(&path)?;
        let display = self.strip_root(&resolved);
        Ok(Name {
            id,
            files: vec![File::dummy(&display)],
        })
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let resolved = self.resolve(&path)?;
        let meta = fs::metadata(&resolved).await.map_err(|e| io_to_sftp(&e))?;
        Ok(Attrs { id, attrs: FileAttributes::from(&meta) })
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let resolved = self.resolve(&path)?;
        let meta = fs::symlink_metadata(&resolved).await.map_err(|e| io_to_sftp(&e))?;
        Ok(Attrs { id, attrs: FileAttributes::from(&meta) })
    }

    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, Self::Error> {
        let file = self.open_files.get(&handle).ok_or(StatusCode::Failure)?;
        let meta = file.metadata().await.map_err(|e| io_to_sftp(&e))?;
        Ok(Attrs { id, attrs: FileAttributes::from(&meta) })
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        let resolved = self.resolve(&path)?;
        let mut read_dir = fs::read_dir(&resolved).await.map_err(|e| io_to_sftp(&e))?;
        let mut entries = Vec::new();
        while let Ok(Some(entry)) = read_dir.next_entry().await {
            entries.push(entry);
        }
        let handle = self.alloc_handle();
        self.open_dirs.insert(handle.clone(), entries);
        Ok(Handle { id, handle })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        let entries = self.open_dirs.get_mut(&handle).ok_or(StatusCode::Failure)?;
        if entries.is_empty() {
            return Err(StatusCode::Eof);
        }

        let batch: Vec<fs::DirEntry> = entries.drain(..entries.len().min(64)).collect();
        let mut files = Vec::with_capacity(batch.len());
        for entry in batch {
            let name = entry.file_name().to_string_lossy().to_string();
            let attrs = match entry.metadata().await {
                Ok(m) => FileAttributes::from(&m),
                Err(_) => FileAttributes::default(),
            };
            files.push(File::new(&name, attrs));
        }

        Ok(Name { id, files })
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
        self.open_files.remove(&handle);
        self.open_dirs.remove(&handle);
        Ok(ok_status(id))
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        let resolved = self.resolve(&filename)?;
        let std_opts: std::fs::OpenOptions = pflags.into();
        let file = tokio::fs::OpenOptions::from(std_opts)
            .open(&resolved)
            .await
            .map_err(|e| io_to_sftp(&e))?;
        let handle = self.alloc_handle();
        self.open_files.insert(handle.clone(), file);
        Ok(Handle { id, handle })
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        let file = self.open_files.get_mut(&handle).ok_or(StatusCode::Failure)?;
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|e| io_to_sftp(&e))?;
        let mut buf = vec![0u8; len as usize];
        let n = file.read(&mut buf).await.map_err(|e| io_to_sftp(&e))?;
        if n == 0 {
            return Err(StatusCode::Eof);
        }
        buf.truncate(n);
        Ok(Data { id, data: buf })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        let file = self.open_files.get_mut(&handle).ok_or(StatusCode::Failure)?;
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|e| io_to_sftp(&e))?;
        file.write_all(&data).await.map_err(|e| io_to_sftp(&e))?;
        Ok(ok_status(id))
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        let resolved = self.resolve(&filename)?;
        fs::remove_file(&resolved).await.map_err(|e| io_to_sftp(&e))?;
        Ok(ok_status(id))
    }

    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, Self::Error> {
        let old = self.resolve(&oldpath)?;
        let new = self.resolve(&newpath)?;
        fs::rename(&old, &new).await.map_err(|e| io_to_sftp(&e))?;
        Ok(ok_status(id))
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        let resolved = self.resolve(&path)?;
        fs::create_dir(&resolved).await.map_err(|e| io_to_sftp(&e))?;
        Ok(ok_status(id))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
        let resolved = self.resolve(&path)?;
        fs::remove_dir_all(&resolved).await.map_err(|e| io_to_sftp(&e))?;
        Ok(ok_status(id))
    }

    async fn setstat(
        &mut self,
        id: u32,
        _path: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        Ok(ok_status(id))
    }

    async fn fsetstat(
        &mut self,
        id: u32,
        _handle: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        Ok(ok_status(id))
    }
}
