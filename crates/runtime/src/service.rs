use crate::Library;
use anyhow::{Result, anyhow};
use futures::channel::oneshot;
use std::sync::mpsc;

type Job = Box<dyn FnOnce(&mut Library) + Send>;
#[derive(Clone)]
pub struct Service {
    sender: mpsc::Sender<Job>,
}
impl Service {
    pub fn new(mut library: Library) -> Self {
        let (sender, receiver) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("reader-library".into())
            .spawn(move || {
                for job in receiver {
                    job(&mut library);
                }
            })
            .expect("无法启动书库线程");
        Self { sender }
    }
    pub fn submit<T: Send + 'static, F: FnOnce(&mut Library) -> Result<T> + Send + 'static>(
        &self,
        job: F,
    ) -> impl Future<Output = Result<T>> + Send + 'static + use<T, F> {
        let (sender, receiver) = oneshot::channel();
        let submitted = self.sender.send(Box::new(move |library| {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job(library)))
                .unwrap_or_else(|panic| {
                    let message = panic
                        .downcast_ref::<&str>()
                        .copied()
                        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
                        .unwrap_or("未知错误");
                    Err(anyhow!("书库任务异常：{message}"))
                });
            let _ = sender.send(result);
        }));
        async move {
            submitted.map_err(|_| anyhow!("书库服务已关闭"))?;
            receiver.await.map_err(|_| anyhow!("书库服务未返回结果"))?
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panic_returns_error_and_worker_accepts_next_job() {
        let root = std::env::temp_dir().join(format!("reader-panic-{}", std::process::id()));
        let paths = crate::AppPaths::isolated(&root).unwrap();
        let service = Service::new(Library::open(paths).unwrap());
        let failed = futures::executor::block_on(service.submit::<(), _>(|_| panic!("坏任务")));
        assert!(failed.unwrap_err().to_string().contains("坏任务"));
        assert!(futures::executor::block_on(service.submit(|library| library.books())).is_ok());
        drop(service);
        let _ = std::fs::remove_dir_all(root);
    }
}
