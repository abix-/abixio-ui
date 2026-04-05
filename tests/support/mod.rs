use std::future::Future;
use std::sync::LazyLock;

use tokio::runtime::Runtime;
use tokio::sync::oneshot;

pub static RUNTIME: LazyLock<Runtime> =
    LazyLock::new(|| Runtime::new().expect("failed to create tokio runtime"));

/// AsyncOp is available for testing. The app itself uses iced's Task::perform.
pub struct AsyncOp<T> {
    rx: Option<oneshot::Receiver<Result<T, String>>>,
    pub data: Option<Result<T, String>>,
    pub pending: bool,
}

impl<T: Send + 'static> AsyncOp<T> {
    pub fn new() -> Self {
        Self {
            rx: None,
            data: None,
            pending: false,
        }
    }

    pub fn request<F>(&mut self, fut: F)
    where
        F: Future<Output = Result<T, String>> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        RUNTIME.spawn(async move {
            let result = fut.await;
            let _ = tx.send(result);
        });
        self.rx = Some(rx);
        self.pending = true;
        self.data = None;
    }

    pub fn poll(&mut self) {
        if let Some(rx) = &mut self.rx {
            match rx.try_recv() {
                Ok(result) => {
                    self.data = Some(result);
                    self.pending = false;
                    self.rx = None;
                }
                Err(oneshot::error::TryRecvError::Empty) => {}
                Err(oneshot::error::TryRecvError::Closed) => {
                    self.pending = false;
                    self.rx = None;
                }
            }
        }
    }
}
