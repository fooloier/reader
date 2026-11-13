use crate::{AppPaths, formats};
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use reader_core::{Book, BookId, Format, Location, Settings, Workspace};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize)]
pub struct ImportOutcome {
    pub path: PathBuf,
    pub status: String,
    pub id: Option<BookId>,
    pub error: Option<String>,
}

pub struct Library {
    pub paths: AppPaths,
    db: Connection,
    _lock: File,
    diagnostics: std::cell::RefCell<Vec<String>>,
}

impl Library {
    pub fn open(paths: AppPaths) -> Result<Self> {
        paths.create()?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(paths.data.join("library.lock"))?;
        lock.try_lock_exclusive()
            .context("书库正在被另一个 Reader 进程使用")?;
        let db = Connection::open(paths.data.join("library.sqlite3"))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS books(id INTEGER PRIMARY KEY AUTOINCREMENT, digest TEXT UNIQUE NOT NULL, format TEXT NOT NULL, metadata TEXT NOT NULL, title TEXT NOT NULL, filename TEXT NOT NULL UNIQUE, cover TEXT, location TEXT);")?;
        let mut library = Self {
            paths,
            db,
            _lock: lock,
            diagnostics: Default::default(),
        };
        library.recover()?;
        library.settings()?;
        library.workspace()?;
        Ok(library)
    }
    pub fn diagnostics(&self) -> Vec<String> {
        self.diagnostics.borrow().clone()
    }
    pub fn books(&self) -> Result<Vec<Book>> {
        let mut query = self.db.prepare("SELECT id,digest,format,metadata,title,filename,cover,location FROM books ORDER BY id DESC")?;
        let rows = query.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
            ))
        })?;
        rows.map(|row| {
            let (id, digest, format, metadata, display_title, filename, cover, location) = row?;
            Ok(Book {
                id: BookId(id),
                digest,
                format: Format::from_extension(&format).context("书库格式无效")?,
                metadata: serde_json::from_str(&metadata)?,
                display_title,
                filename,
                cover,
                location: location.map(|v| serde_json::from_str(&v)).transpose()?,
            })
        })
        .collect()
    }
    pub fn book(&self, id: BookId) -> Result<Book> {
        self.books()?
            .into_iter()
            .find(|b| b.id == id)
            .context("书籍已被删除")
    }
    pub fn path(&self, book: &Book) -> PathBuf {
        self.paths.books().join(&book.filename)
    }
    pub fn import(&mut self, roots: &[PathBuf]) -> Vec<ImportOutcome> {
        let mut files = Vec::new();
        let mut results = Vec::new();
        for root in roots {
            collect_files(root, true, 0, &mut 0, &mut files, &mut results);
        }
        files.sort();
        files.dedup();
        for path in files {
            let outcome = match self.import_file(&path) {
                Ok((id, duplicate)) => ImportOutcome {
                    path,
                    id: Some(id),
                    status: if duplicate { "duplicate" } else { "imported" }.into(),
                    error: None,
                },
                Err(e) => ImportOutcome {
                    path,
                    id: None,
                    status: "failed".into(),
                    error: Some(e.to_string()),
                },
            };
            results.push(outcome);
        }
        results
    }
    fn import_file(&mut self, source: &Path) -> Result<(BookId, bool)> {
        let format = source
            .extension()
            .and_then(|s| s.to_str())
            .and_then(Format::from_extension)
            .context("仅支持 PDF 和 EPUB")?;
        let mut input = File::open(source)?;
        ensure!(
            input.metadata()?.len() <= 512 * 1024 * 1024,
            "文件超过 512 MiB 导入上限"
        );
        let staging = self.paths.data.join("staging/import.tmp");
        let mut output = File::create(&staging)?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
            output.write_all(&buffer[..count])?;
        }
        output.sync_all()?;
        drop(output);
        let digest = format!("{:x}", hash.finalize());
        if let Some(id) = self
            .db
            .query_row("SELECT id FROM books WHERE digest=?1", [&digest], |r| {
                r.get(0)
            })
            .optional()?
        {
            fs::remove_file(&staging)?;
            return Ok((BookId(id), true));
        }
        let fallback = source.file_stem().unwrap_or_default().to_string_lossy();
        let (metadata, cover) = formats::inspect(&staging, format, &fallback)?;
        if format == Format::Epub {
            crate::epub::EpubBook::open(&staging)?;
        }
        let filename = managed_name(&digest, &metadata.title, format);
        let target = self.paths.books().join(&filename);
        fs::rename(&staging, &target)?;
        sync_directory(&self.paths.books())?;
        let cover_name = cover
            .map(|bytes| -> Result<String> {
                let name = format!("{digest}.png");
                atomic_write(&self.paths.cache.join(&name), &bytes)?;
                Ok(name)
            })
            .transpose()?;
        let inserted = self.db.execute("INSERT INTO books(digest,format,metadata,title,filename,cover) VALUES(?1,?2,?3,?4,?5,?6)", params![digest, format.extension(), serde_json::to_string(&metadata)?, metadata.title, filename, cover_name]);
        if let Err(e) = inserted {
            let _ = fs::remove_file(target);
            return Err(e.into());
        }
        Ok((BookId(self.db.last_insert_rowid()), false))
    }
    pub fn rename(&mut self, id: BookId, title: &str) -> Result<Book> {
        let title = title.trim();
        ensure!(
            !title.is_empty() && title.chars().count() <= 200,
            "书名须为 1 到 200 个字符"
        );
        let book = self.book(id)?;
        let filename = managed_name(&book.digest, title, book.format);
        let old = self.path(&book);
        let new = self.paths.books().join(&filename);
        if old != new {
            fs::copy(&old, &new)?;
            File::open(&new)?.sync_all()?;
            sync_directory(&self.paths.books())?;
        }
        self.db.execute(
            "UPDATE books SET title=?1,filename=?2 WHERE id=?3",
            params![title, filename, id.0],
        )?;
        if old != new {
            fs::remove_file(old)?;
        }
        self.book(id)
    }
    pub fn delete(&mut self, id: BookId) -> Result<()> {
        let book = self.book(id)?;
        self.db.execute("DELETE FROM books WHERE id=?1", [id.0])?;
        let mut workspace = self.workspace()?;
        workspace.close(id);
        self.save_workspace(&workspace)?;
        remove_if_exists(&self.path(&book))?;
        if let Some(cover) = book.cover {
            remove_if_exists(&self.paths.cache.join(cover))?;
        }
        Ok(())
    }
    pub fn save_location(&mut self, id: BookId, location: &Location) -> Result<()> {
        let book = self.book(id)?;
        ensure!(location.matches(book.format), "阅读位置与格式不匹配");
        self.db.execute(
            "UPDATE books SET location=?1 WHERE id=?2",
            params![serde_json::to_string(location)?, id.0],
        )?;
        Ok(())
    }
    pub fn settings(&self) -> Result<Settings> {
        self.read_json(&self.paths.config.join("settings.json"), Settings::is_valid)
    }
    pub fn save_settings(&mut self, value: &Settings) -> Result<()> {
        ensure!(value.is_valid(), "阅读设置超出范围");
        atomic_write(
            &self.paths.config.join("settings.json"),
            &serde_json::to_vec_pretty(value)?,
        )
    }
    pub fn workspace(&self) -> Result<Workspace> {
        let mut value: Workspace =
            self.read_json(&self.paths.state.join("workspace.json"), |_| true)?;
        let books = self.books()?;
        value.retain(|id| books.iter().any(|b| b.id == id));
        Ok(value)
    }
    pub fn save_workspace(&mut self, value: &Workspace) -> Result<()> {
        let mut value = value.clone();
        let books = self.books()?;
        value.retain(|id| books.iter().any(|b| b.id == id));
        atomic_write(
            &self.paths.state.join("workspace.json"),
            &serde_json::to_vec_pretty(&value)?,
        )
    }
    fn recover(&mut self) -> Result<()> {
        for entry in fs::read_dir(self.paths.data.join("staging"))? {
            let path = entry?.path();
            if path.is_file() {
                fs::remove_file(path)?;
            }
        }
        let books = self.books()?;
        for book in books {
            if !self.path(&book).is_file() {
                self.diagnostics.borrow_mut().push(format!(
                    "《{}》的托管文件缺失，请恢复 {}",
                    book.display_title,
                    self.path(&book).display()
                ));
            } else if book
                .cover
                .as_ref()
                .is_some_and(|name| !self.paths.cache.join(name).is_file())
                && let Ok((_, Some(bytes))) =
                    formats::inspect(&self.path(&book), book.format, &book.metadata.title)
            {
                let name = book.cover.as_ref().expect("已有封面记录");
                atomic_write(&self.paths.cache.join(name), &bytes)?;
            }
        }
        Ok(())
    }
    fn read_json<T: DeserializeOwned + Default>(
        &self,
        path: &Path,
        valid: impl Fn(&T) -> bool,
    ) -> Result<T> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
            Err(e) => return Err(e.into()),
        };
        match serde_json::from_slice::<T>(&bytes) {
            Ok(value) if valid(&value) => Ok(value),
            _ => {
                let mut sequence = 0;
                let backup = loop {
                    let backup = path.with_file_name(format!(
                        "{}.corrupt-{sequence}",
                        path.file_name().unwrap().to_string_lossy()
                    ));
                    match OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&backup)
                    {
                        Ok(mut file) => {
                            file.write_all(&bytes)?;
                            file.sync_all()?;
                            break backup;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                            if fs::read(&backup)? == bytes {
                                break backup;
                            }
                            sequence += 1;
                        }
                        Err(e) => return Err(e.into()),
                    }
                };
                sync_directory(path.parent().context("没有父目录")?)?;
                let message = format!(
                    "{} 已损坏，使用默认值。原文件保存在 {}",
                    path.display(),
                    backup.display()
                );
                if !self.diagnostics.borrow().contains(&message) {
                    self.diagnostics.borrow_mut().push(message);
                }
                Ok(T::default())
            }
        }
    }
}

fn collect_files(
    path: &Path,
    root: bool,
    depth: usize,
    visited: &mut usize,
    files: &mut Vec<PathBuf>,
    results: &mut Vec<ImportOutcome>,
) {
    let mut skipped = None;
    let result = (|| -> Result<()> {
        *visited += 1;
        if root && fs::symlink_metadata(path)?.file_type().is_symlink() {
            collect_files(
                &fs::canonicalize(path)?,
                true,
                depth,
                visited,
                files,
                results,
            );
            return Ok(());
        }
        ensure!(*visited <= 100_000 && depth <= 64, "文件夹遍历超过安全上限");
        let meta = if root {
            fs::metadata(path)?
        } else {
            fs::symlink_metadata(path)?
        };
        if meta.file_type().is_symlink() {
            skipped = Some("跳过嵌套符号链接，避免循环或越界".to_string());
        } else if meta.is_file() {
            let supported = path
                .extension()
                .and_then(|v| v.to_str())
                .and_then(Format::from_extension)
                .is_some();
            if root || supported {
                files.push(path.into());
            } else {
                skipped = Some("非 PDF/EPUB 文件".into());
            }
        } else if meta.is_dir() {
            for entry in fs::read_dir(path)? {
                if *visited >= 100_000 {
                    bail!("文件夹遍历超过 100000 项上限");
                }
                collect_files(&entry?.path(), false, depth + 1, visited, files, results);
            }
        } else {
            skipped = Some("非常规文件".into());
        }
        Ok(())
    })();
    if let Err(e) = result {
        results.push(ImportOutcome {
            path: path.into(),
            status: "failed".into(),
            id: None,
            error: Some(e.to_string()),
        });
    } else if let Some(reason) = skipped {
        results.push(ImportOutcome {
            path: path.into(),
            status: "skipped".into(),
            id: None,
            error: Some(reason),
        });
    }
}

fn managed_name(digest: &str, title: &str, format: Format) -> String {
    let mut clean = String::new();
    for c in title
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ' '))
    {
        if clean.len() + c.len_utf8() > 160 {
            break;
        }
        clean.push(c);
    }
    format!(
        "{digest}-{}.{}",
        if clean.is_empty() { "book" } else { &clean },
        format.extension()
    )
}
fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    Ok(())
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = path.with_extension("tmp");
    let mut file = File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, path)?;
    sync_directory(path.parent().context("没有父目录")?)
}
