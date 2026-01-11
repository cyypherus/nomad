use tokio::sync::{oneshot, watch};

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
    Failed(String),
}

pub struct PageRequest {
    status_rx: watch::Receiver<PageStatus>,
    result_rx: Option<oneshot::Receiver<Result<String, String>>>,
}

impl PageRequest {
    pub(crate) fn new(
        status_rx: watch::Receiver<PageStatus>,
        result_rx: oneshot::Receiver<Result<String, String>>,
    ) -> Self {
        Self {
            status_rx,
            result_rx: Some(result_rx),
        }
    }

    pub fn status(&self) -> PageStatus {
        self.status_rx.borrow().clone()
    }

    pub fn status_receiver(&self) -> watch::Receiver<PageStatus> {
        self.status_rx.clone()
    }

    pub async fn result(mut self) -> Result<String, String> {
        match self.result_rx.take() {
            Some(rx) => rx.await.unwrap_or_else(|_| Err("Request cancelled".into())),
            None => Err("Result already consumed".into()),
        }
    }
}

pub(crate) struct PageRequestHandle {
    pub status_tx: watch::Sender<PageStatus>,
    pub result_tx: Option<oneshot::Sender<Result<String, String>>>,
}

impl PageRequestHandle {
    pub fn new() -> (Self, PageRequest) {
        let (status_tx, status_rx) = watch::channel(PageStatus::RequestingPath);
        let (result_tx, result_rx) = oneshot::channel();

        let handle = Self {
            status_tx,
            result_tx: Some(result_tx),
        };

        let request = PageRequest::new(status_rx, result_rx);

        (handle, request)
    }

    pub fn set_status(&self, status: PageStatus) {
        let _ = self.status_tx.send(status);
    }

    pub fn complete(mut self, content: String) {
        self.set_status(PageStatus::Complete);
        if let Some(tx) = self.result_tx.take() {
            let _ = tx.send(Ok(content));
        }
    }

    pub fn fail(mut self, reason: String) {
        self.set_status(PageStatus::Failed(reason.clone()));
        if let Some(tx) = self.result_tx.take() {
            let _ = tx.send(Err(reason));
        }
    }
}
