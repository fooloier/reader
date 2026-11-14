use crossbeam_channel::{Receiver, Sender, bounded, select};
use futures::channel::oneshot;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

pub const ENDPOINT: &str = "https://index-translate.bilibili.com/v1/chat/completions";
const MODEL: &str = "Index-Translate-35B-A3B";
const MAX_SOURCE_BYTES: usize = 16 * 1024;
const MAX_RESULT_BYTES: usize = 128 * 1024;
const DISK_LIMIT: usize = 50 * 1024 * 1024;
const MEMORY_LIMIT: usize = 2 * 1024 * 1024;
type Reply = Result<Arc<str>, String>;
type Waiter = oneshot::Sender<Reply>;

#[derive(Clone)]
pub struct TranslationService {
    commands: Sender<Command>,
    epoch: Arc<AtomicU64>,
}

enum Command {
    Translate {
        source: String,
        epoch: u64,
        reply: Waiter,
    },
    Clear(oneshot::Sender<Result<(), String>>),
}
struct Job {
    key: String,
    source: String,
    epoch: u64,
}
struct Finished {
    job: Job,
    result: Reply,
}
struct MemoryEntry {
    text: Arc<str>,
    used: i64,
}
struct Cache {
    db: Option<Connection>,
    disk_error: Option<String>,
    memory: HashMap<String, MemoryEntry>,
    clock: i64,
    disk_limit: usize,
    memory_limit: usize,
}

impl TranslationService {
    pub fn new(cache_dir: &Path) -> Self {
        Self::start(
            cache_dir.join("translations.sqlite3"),
            ENDPOINT.into(),
            Duration::from_secs(60),
            DISK_LIMIT,
            MEMORY_LIMIT,
        )
    }
    fn start(
        path: PathBuf,
        endpoint: String,
        timeout: Duration,
        disk_limit: usize,
        memory_limit: usize,
    ) -> Self {
        let (commands, requests) = bounded(64);
        let (jobs, work) = bounded::<Job>(32);
        let (done, completions) = bounded(32);
        for ix in 0..3 {
            let work = work.clone();
            let done = done.clone();
            let endpoint = endpoint.clone();
            std::thread::Builder::new()
                .name(format!("translation-http-{ix}"))
                .spawn(move || {
                    let agent = ureq::Agent::config_builder()
                        .timeout_global(Some(timeout))
                        .build()
                        .new_agent();
                    for job in work {
                        let result = request(&agent, &endpoint, &job.source);
                        if done.send(Finished { job, result }).is_err() {
                            break;
                        }
                    }
                })
                .expect("无法创建翻译线程");
        }
        let epoch = Arc::new(AtomicU64::new(0));
        let worker_epoch = epoch.clone();
        std::thread::Builder::new()
            .name("translation-cache".into())
            .spawn(move || {
                let cache = Cache::open(&path, disk_limit, memory_limit);
                coordinate(requests, jobs, completions, worker_epoch, cache, endpoint);
            })
            .expect("无法创建翻译缓存线程");
        Self { commands, epoch }
    }
    pub fn translate(&self, source: &str) -> oneshot::Receiver<Reply> {
        let (reply, receive) = oneshot::channel();
        let source = source
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .trim()
            .to_owned();
        let error = if source.is_empty() {
            Some("请先选择文字。")
        } else if source.len() > MAX_SOURCE_BYTES {
            Some("选中文字过长，请缩小选区后重试（最多 16 KiB）。")
        } else {
            None
        };
        if let Some(error) = error {
            let _ = reply.send(Err(error.into()));
        } else if let Err(error) = self.commands.try_send(Command::Translate {
            source,
            epoch: self.epoch.load(Ordering::SeqCst),
            reply,
        }) && let Command::Translate { reply, .. } = error.into_inner()
        {
            let _ = reply.send(Err("翻译任务繁忙，请稍后重试。".into()));
        }
        receive
    }
    pub fn clear(&self) -> oneshot::Receiver<Result<(), String>> {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        let (reply, receive) = oneshot::channel();
        if let Err(error) = self.commands.try_send(Command::Clear(reply))
            && let Command::Clear(reply) = error.into_inner()
        {
            let _ = reply.send(Err("清除请求已标记，但尚未确认完成，请稍后重试。".into()));
        }
        receive
    }
    #[cfg(feature = "test-support")]
    pub fn local(cache_dir: &Path, endpoint: String) -> Self {
        Self::start(
            cache_dir.join("translations.sqlite3"),
            endpoint,
            Duration::from_secs(3),
            DISK_LIMIT,
            MEMORY_LIMIT,
        )
    }
}

fn cache_key(endpoint: &str, source: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(endpoint, MODEL, "zh-Hans", 1, source)).unwrap())
    )
}

enum Incoming {
    Command(Command),
    Finished(Finished),
}

fn coordinate(
    requests: Receiver<Command>,
    jobs: Sender<Job>,
    completions: Receiver<Finished>,
    epoch: Arc<AtomicU64>,
    mut cache: Cache,
    endpoint: String,
) {
    let mut current_epoch = epoch.load(Ordering::SeqCst);
    let mut inflight: HashMap<(String, u64), Vec<Waiter>> = HashMap::new();
    loop {
        let incoming = select! {
            recv(requests) -> command => match command { Ok(command) => Incoming::Command(command), Err(_) => break },
            recv(completions) -> finished => match finished { Ok(finished) => Incoming::Finished(finished), Err(_) => break },
        };
        match incoming {
            Incoming::Command(Command::Clear(reply)) => {
                current_epoch = epoch.load(Ordering::SeqCst);
                let _ = reply.send(cache.clear());
            }
            Incoming::Command(Command::Translate {
                source,
                epoch: requested_epoch,
                reply,
            }) => {
                let latest = epoch.load(Ordering::SeqCst);
                if latest != current_epoch {
                    let _ = cache.clear();
                    current_epoch = latest;
                }
                if requested_epoch != latest {
                    let _ = reply.send(Err("翻译缓存已清除，请重试。".into()));
                    continue;
                }
                let key = cache_key(&endpoint, &source);
                if let Some(text) = cache.get(&key) {
                    let _ = reply.send(Ok(text));
                    continue;
                }
                if let Some(waiters) = inflight.get_mut(&(key.clone(), latest)) {
                    if waiters.len() < 64 {
                        waiters.push(reply);
                    } else {
                        let _ = reply.send(Err("翻译任务繁忙，请稍后重试。".into()));
                    }
                    continue;
                }
                if inflight.len() >= 32 {
                    let _ = reply.send(Err("翻译任务繁忙，请稍后重试。".into()));
                    continue;
                }
                let job = Job {
                    key: key.clone(),
                    source,
                    epoch: latest,
                };
                match jobs.try_send(job) {
                    Ok(()) => {
                        inflight.insert((key, latest), vec![reply]);
                    }
                    Err(_) => {
                        let _ = reply.send(Err("翻译任务繁忙，请稍后重试。".into()));
                    }
                }
            }
            Incoming::Finished(Finished { job, result }) => {
                if job.epoch == epoch.load(Ordering::SeqCst)
                    && let Ok(text) = &result
                {
                    cache.put(&job.key, text.clone());
                }
                for reply in inflight.remove(&(job.key, job.epoch)).unwrap_or_default() {
                    let _ = reply.send(result.clone());
                }
            }
        }
    }
}

fn request(agent: &ureq::Agent, endpoint: &str, source: &str) -> Reply {
    let body = json!({
        "model": MODEL,
        "messages": [{"role": "user", "content": format!("请将以下文本翻译为简体中文，直接输出翻译结果，不要进行任何解释。\n\n{source}")}],
        "temperature": 0, "stream": false, "max_tokens": 16384,
        "chat_template_kwargs": {"enable_thinking": false}
    });
    let mut response = agent
        .post(endpoint)
        .header("User-Agent", "Index-Translate-Client/1.0")
        .send_json(body)
        .map_err(|error| match error {
            ureq::Error::StatusCode(429) => "翻译服务请求过多，请稍后重试。".into(),
            ureq::Error::Timeout(_) => "翻译请求超时，请重试。".into(),
            ureq::Error::StatusCode(code) => format!("翻译服务暂不可用（HTTP {code}），请重试。"),
            _ => format!("无法连接翻译服务：{error}"),
        })?;
    let body: serde_json::Value = response
        .body_mut()
        .with_config()
        .limit(MAX_RESULT_BYTES as u64)
        .read_json()
        .map_err(|_| "翻译服务返回了不完整或无效的数据，请重试。".to_owned())?;
    let choice = &body["choices"][0];
    if choice["finish_reason"].as_str() != Some("stop") {
        return Err("翻译结果不完整，请缩小选区后重试。".into());
    }
    let content = choice["message"]["content"]
        .as_str()
        .ok_or("翻译服务没有返回译文，请重试。")?
        .trim();
    if content.is_empty() || content.contains("<think>") || content.contains("</think>") {
        return Err("翻译服务没有返回有效译文，请重试。".into());
    }
    Ok(Arc::from(content))
}

impl Cache {
    fn open(path: &Path, disk_limit: usize, memory_limit: usize) -> Self {
        let db = (|| -> rusqlite::Result<Connection> {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let db = Connection::open(path)?;
            db.busy_timeout(Duration::from_secs(2))?;
            db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA auto_vacuum=FULL;
                CREATE TABLE IF NOT EXISTS translations (key TEXT PRIMARY KEY, text TEXT NOT NULL, used INTEGER NOT NULL);")?;
            Ok(db)
        })();
        let (db, disk_error) = match db {
            Ok(db) => (Some(db), None),
            Err(error) => (None, Some(format!("无法访问翻译缓存：{error}"))),
        };
        let clock = db
            .as_ref()
            .and_then(|db| {
                db.query_row("SELECT max(used) FROM translations", [], |r| {
                    r.get::<_, Option<i64>>(0)
                })
                .ok()
                .flatten()
            })
            .unwrap_or(0);
        Self {
            db,
            disk_error,
            memory: HashMap::new(),
            clock,
            disk_limit,
            memory_limit,
        }
    }
    fn get(&mut self, key: &str) -> Option<Arc<str>> {
        self.clock += 1;
        if let Some(entry) = self.memory.get_mut(key) {
            entry.used = self.clock;
            let text = entry.text.clone();
            if let Some(db) = &self.db {
                let _ = db.execute(
                    "UPDATE translations SET used=? WHERE key=?",
                    params![self.clock, key],
                );
            }
            return Some(text);
        }
        let text: String = self
            .db
            .as_ref()?
            .query_row("SELECT text FROM translations WHERE key=?", [key], |r| {
                r.get(0)
            })
            .optional()
            .ok()??;
        let text: Arc<str> = Arc::from(text);
        self.put(key, text.clone());
        Some(text)
    }
    fn put(&mut self, key: &str, text: Arc<str>) {
        self.clock += 1;
        self.memory.insert(
            key.into(),
            MemoryEntry {
                text: text.clone(),
                used: self.clock,
            },
        );
        while self
            .memory
            .iter()
            .map(|(k, e)| k.len() + e.text.len())
            .sum::<usize>()
            > self.memory_limit
        {
            let Some(key) = self
                .memory
                .iter()
                .min_by_key(|(_, e)| e.used)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            self.memory.remove(&key);
        }
        if let Some(db) = &self.db {
            let _ = db.execute(
                "INSERT OR REPLACE INTO translations VALUES (?, ?, ?)",
                params![key, text.as_ref(), self.clock],
            );
            loop {
                let size = db.query_row("SELECT coalesce(sum(length(CAST(text AS BLOB))+length(key)+64),0) FROM translations", [], |r| r.get::<_, i64>(0)).unwrap_or(0);
                let pages: i64 = db
                    .query_row("PRAGMA page_count", [], |r| r.get(0))
                    .unwrap_or(0);
                let page_size: i64 = db
                    .query_row("PRAGMA page_size", [], |r| r.get(0))
                    .unwrap_or(4096);
                if size <= self.disk_limit as i64
                    && pages * page_size <= self.disk_limit.max(16384) as i64
                {
                    break;
                }
                if db.execute("DELETE FROM translations WHERE key=(SELECT key FROM translations ORDER BY used LIMIT 1)", []).unwrap_or(0) == 0 { break; }
            }
        }
    }
    fn clear(&mut self) -> Result<(), String> {
        self.memory.clear();
        if let Some(error) = &self.disk_error {
            return Err(error.clone());
        }
        if let Some(db) = &self.db {
            db.execute_batch("DELETE FROM translations; VACUUM;")
                .map_err(|e| format!("无法清除翻译缓存：{e}"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{Mutex, atomic::AtomicBool},
        thread,
    };

    struct Server {
        url: String,
        stop: Arc<AtomicBool>,
        requests: Receiver<serde_json::Value>,
        thread: Option<thread::JoinHandle<()>>,
    }
    impl Server {
        fn new(handler: impl Fn(usize) -> (u16, String) + Send + Sync + 'static) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!(
                "http://{}/v1/chat/completions",
                listener.local_addr().unwrap()
            );
            listener.set_nonblocking(true).unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let stopped = stop.clone();
            let (sent, requests) = bounded(128);
            let handler = Arc::new(handler);
            let thread = thread::spawn(move || {
                let mut count = 0;
                let mut children = vec![];
                while !stopped.load(Ordering::SeqCst) {
                    let Ok((mut stream, _)) = listener.accept() else {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    };
                    count += 1;
                    let ix = count;
                    let handler = handler.clone();
                    let sent = sent.clone();
                    children.push(thread::spawn(move || {
                        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                        let mut data = Vec::new();
                        let mut buffer = [0u8; 4096];
                        let (header_end, length) = loop {
                            let read = stream.read(&mut buffer).unwrap();
                            assert!(read > 0);
                            data.extend_from_slice(&buffer[..read]);
                            if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                                let headers = String::from_utf8_lossy(&data[..end]).to_lowercase();
                                assert!(headers.contains("user-agent: index-translate-client/1.0"));
                                let length: usize = headers.lines().find_map(|l| l.strip_prefix("content-length:")).unwrap().trim().parse().unwrap();
                                break (end + 4, length);
                            }
                        };
                        while data.len() < header_end + length {
                            let read = stream.read(&mut buffer).unwrap();
                            assert!(read > 0);
                            data.extend_from_slice(&buffer[..read]);
                        }
                        sent.send(serde_json::from_slice(&data[header_end..header_end+length]).unwrap()).unwrap();
                        let (status, body) = handler(ix);
                        let _ = write!(stream, "HTTP/1.1 {status} Result\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    }));
                }
                for child in children {
                    child.join().unwrap();
                }
            });
            Self {
                url,
                stop,
                requests,
                thread: Some(thread),
            }
        }
    }
    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            self.thread.take().unwrap().join().unwrap();
        }
    }
    fn success(text: &str) -> String {
        json!({"choices":[{"message":{"content":text},"finish_reason":"stop"}]}).to_string()
    }
    fn temp_root(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "reader-translation-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
    fn receive(reply: oneshot::Receiver<Reply>) -> Reply {
        futures::executor::block_on(reply).unwrap()
    }
    fn service(root: &Path, server: &Server) -> TranslationService {
        TranslationService::start(
            root.join("cache.db"),
            server.url.clone(),
            Duration::from_secs(2),
            DISK_LIMIT,
            MEMORY_LIMIT,
        )
    }

    #[test]
    fn http_contract_dedup_and_restart_cache() {
        let server = Server::new(|_| {
            thread::sleep(Duration::from_millis(100));
            (200, success("你好\n世界"))
        });
        let root = temp_root("restart");
        let service = service(&root, &server);
        let replies = (0..24)
            .map(|_| service.translate(" \r\nHello\r\n  world\r\n "))
            .collect::<Vec<_>>();
        for reply in replies {
            assert_eq!(receive(reply).unwrap().as_ref(), "你好\n世界");
        }
        let body = server
            .requests
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        assert_eq!(body["model"], MODEL);
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][0]["role"], "user");
        assert!(
            body["messages"][0]["content"]
                .as_str()
                .unwrap()
                .ends_with("Hello\n  world")
        );
        assert_eq!(body["temperature"], 0);
        assert_eq!(body["stream"], false);
        assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);
        assert!(server.requests.is_empty(), "同键并发只能发送一次 HTTP");
        let endpoint = server.url.clone();
        drop(service);
        drop(server);
        let restarted = TranslationService::start(
            root.join("cache.db"),
            endpoint,
            Duration::from_millis(100),
            DISK_LIMIT,
            MEMORY_LIMIT,
        );
        assert_eq!(
            receive(restarted.translate("Hello\n  world"))
                .unwrap()
                .as_ref(),
            "你好\n世界"
        );
        drop(restarted);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failures_are_not_cached_and_length_is_explicit() {
        let server = Server::new(|ix| match ix {
            1 => (429, "{}".into()),
            2 => (200, "not json".into()),
            3 => (
                200,
                json!({"choices":[{"message":{"content":"半句"},"finish_reason":"length"}]})
                    .to_string(),
            ),
            4 => (200, success(" ")),
            _ => (200, success("完整译文")),
        });
        let root = temp_root("errors");
        let service = service(&root, &server);
        for _ in 0..4 {
            assert!(receive(service.translate("same source")).is_err());
        }
        assert_eq!(
            receive(service.translate("same source")).unwrap().as_ref(),
            "完整译文"
        );
        assert!(
            receive(service.translate(&"x".repeat(MAX_SOURCE_BYTES + 1)))
                .unwrap_err()
                .contains("过长")
        );
        assert!(receive(service.translate(" \n")).is_err());
        assert_eq!(server.requests.len(), 5);
        drop(service);
        drop(server);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn timeout_and_clear_cannot_repopulate_from_inflight() {
        let (release, gate) = bounded(1);
        let gate = Mutex::new(gate);
        let server = Server::new(move |ix| {
            if ix == 1 {
                gate.lock().unwrap().recv().unwrap();
            }
            (200, success("译文"))
        });
        let root = temp_root("clear");
        let service = service(&root, &server);
        let first = service.translate("old");
        server
            .requests
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        futures::executor::block_on(service.clear())
            .unwrap()
            .unwrap();
        release.send(()).unwrap();
        assert!(receive(first).is_ok());
        assert!(receive(service.translate("old")).is_ok());
        server
            .requests
            .recv_timeout(Duration::from_secs(1))
            .expect("清除之前的在途结果不得填回缓存");
        drop(service);
        drop(server);
        std::fs::remove_dir_all(root).unwrap();
        let server = Server::new(|_| {
            thread::sleep(Duration::from_millis(100));
            (200, success("late"))
        });
        let root = temp_root("timeout");
        let service = TranslationService::start(
            root.join("cache.db"),
            server.url.clone(),
            Duration::from_millis(20),
            DISK_LIMIT,
            MEMORY_LIMIT,
        );
        assert!(
            receive(service.translate("hello"))
                .unwrap_err()
                .contains("超时")
        );
        drop(service);
        drop(server);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn lru_bounds_memory_payload_and_database_file() {
        let root = temp_root("eviction");
        let path = root.join("cache.db");
        let mut cache = Cache::open(&path, 32768, 11000);
        let text: Arc<str> = Arc::from("译".repeat(3000));
        cache.put("a", text.clone());
        cache.put("b", text.clone());
        assert!(!cache.memory.contains_key("a"));
        assert!(cache.get("a").is_some());
        cache.put("c", text);
        assert!(cache.get("b").is_none(), "最久未使用的磁盘条目应淘汰");
        assert!(std::fs::metadata(&path).unwrap().len() <= 32768);
        assert!(
            cache
                .memory
                .iter()
                .map(|(k, e)| k.len() + e.text.len())
                .sum::<usize>()
                <= 11000
        );
        cache.clear().unwrap();
        assert!(cache.get("a").is_none());
        drop(cache);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn fixed_http_pool_and_cache_failure_keep_results_available() {
        let gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let held = gate.clone();
        let server = Server::new(move |_| {
            let (lock, wake) = &*held;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = wake.wait(released).unwrap();
            }
            (200, success("完整结果"))
        });
        let root = temp_root("pool");
        std::fs::write(root.join("blocked"), "file").unwrap();
        let service = TranslationService::start(
            root.join("blocked/cache.db"),
            server.url.clone(),
            Duration::from_secs(2),
            DISK_LIMIT,
            MEMORY_LIMIT,
        );
        let replies = (0..12)
            .map(|ix| service.translate(&format!("source {ix}")))
            .collect::<Vec<_>>();
        for _ in 0..3 {
            server
                .requests
                .recv_timeout(Duration::from_secs(1))
                .unwrap();
        }
        assert!(
            server
                .requests
                .recv_timeout(Duration::from_millis(50))
                .is_err(),
            "固定三个 worker 之外不得发送并行 HTTP"
        );
        {
            let (lock, wake) = &*gate;
            *lock.lock().unwrap() = true;
            wake.notify_all();
        }
        for reply in replies {
            assert_eq!(receive(reply).unwrap().as_ref(), "完整结果");
        }
        assert!(receive(service.translate("source 0")).is_ok());
        assert!(
            futures::executor::block_on(service.clear())
                .unwrap()
                .is_err(),
            "无法访问磁盘缓存时不能声称清除成功"
        );
        let mut cache = Cache::open(&root.join("readonly.db"), DISK_LIMIT, MEMORY_LIMIT);
        cache
            .db
            .as_ref()
            .unwrap()
            .execute_batch("PRAGMA query_only=ON")
            .unwrap();
        cache.put("key", Arc::from("成功译文"));
        assert_eq!(cache.get("key").unwrap().as_ref(), "成功译文");
        drop(cache);
        drop(service);
        drop(server);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn key_preserves_internal_structure_and_namespaces_endpoint() {
        assert_ne!(cache_key("a", "one\n two"), cache_key("a", "one two"));
        assert_ne!(cache_key("a", "same"), cache_key("b", "same"));
    }
    #[test]
    #[ignore = "显式运行公共接口连通性检查"]
    fn public_api_smoke() {
        let root = temp_root("public");
        let service = TranslationService::new(&root);
        let result = receive(service.translate("Hello, world."));
        println!("公共翻译接口结果：{result:?}");
        assert!(result.is_ok());
        drop(service);
        std::fs::remove_dir_all(root).unwrap();
    }
}
