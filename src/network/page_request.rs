use tokio::sync::{oneshot, watch};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq)]
pub enum PageStatus {
    RequestingPath,
    WaitingForAnnounce,
    PathFound {
        hops: u8,
    },
    Connecting,
    LinkEstablished,
    SendingRequest,
    AwaitingResponse,
    Retrieving {
        parts_received: u32,
        total_parts: u32,
    },
    Complete,
    Cancelled,
    Failed(String),
}

pub struct FetchRequest {
    status_rx: watch::Receiver<PageStatus>,
    result_rx: Option<oneshot::Receiver<Result<Vec<u8>, String>>>,
    cancel: CancellationToken,
}

impl FetchRequest {
    pub(crate) fn new(
        status_rx: watch::Receiver<PageStatus>,
        result_rx: oneshot::Receiver<Result<Vec<u8>, String>>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            status_rx,
            result_rx: Some(result_rx),
            cancel,
        }
    }

    pub fn status_receiver(&self) -> watch::Receiver<PageStatus> {
        self.status_rx.clone()
    }

    pub async fn result(mut self) -> Result<Vec<u8>, String> {
        match self.result_rx.take() {
            Some(rx) => rx.await.unwrap_or_else(|_| Err("Request cancelled".into())),
            None => Err("Result already consumed".into()),
        }
    }
}

impl Drop for FetchRequest {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

pub(crate) struct FetchRequestHandle {
    pub status_tx: watch::Sender<PageStatus>,
    pub result_tx: Option<oneshot::Sender<Result<Vec<u8>, String>>>,
    pub cancel: CancellationToken,
}

impl FetchRequestHandle {
    pub fn new() -> (Self, FetchRequest) {
        let (status_tx, status_rx) = watch::channel(PageStatus::RequestingPath);
        let (result_tx, result_rx) = oneshot::channel();
        let cancel = CancellationToken::new();

        let handle = Self {
            status_tx,
            result_tx: Some(result_tx),
            cancel: cancel.clone(),
        };

        let request = FetchRequest::new(status_rx, result_rx, cancel);

        (handle, request)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    pub fn set_status(&self, status: PageStatus) {
        let _ = self.status_tx.send(status);
    }

    pub fn complete(mut self, data: Vec<u8>) {
        self.set_status(PageStatus::Complete);
        if let Some(tx) = self.result_tx.take() {
            let _ = tx.send(Ok(data));
        }
    }

    pub fn fail(mut self, reason: String) {
        self.set_status(PageStatus::Failed(reason.clone()));
        if let Some(tx) = self.result_tx.take() {
            let _ = tx.send(Err(reason));
        }
    }

    pub fn cancelled(mut self) {
        self.set_status(PageStatus::Cancelled);
        if let Some(tx) = self.result_tx.take() {
            let _ = tx.send(Err("Cancelled".into()));
        }
    }
}
