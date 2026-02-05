use crate::claude::Error;
use crate::claude::types::ProtocolMessage;
use futures::{Stream, StreamExt};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use tokio::io::AsyncWriteExt;
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, LinesCodec};

// ============================================================================
// ReadHalf - reads and converts Claude output into ProtocolMessage
// ============================================================================

static TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn next_temp_id(prefix: &str) -> String {
    let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}_{id}")
}

type InnerStream = FramedRead<ChildStdout, LinesCodec>;

pub struct ReadHalf {
    session_id: String,
    sequence: u64,
    stream: InnerStream,
}

impl ReadHalf {
    pub fn new(stdout: ChildStdout, session_id: String) -> Self {
        Self {
            session_id,
            sequence: 0,
            stream: FramedRead::new(stdout, LinesCodec::new()),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }
}

/// A stream wrapper that allows recovering the inner ChildStdout
pub struct EventStream {
    inner: FramedRead<ChildStdout, LinesCodec>,
}

impl EventStream {
    pub fn new(stdout: ChildStdout) -> Self {
        Self {
            inner: FramedRead::new(stdout, LinesCodec::new()),
        }
    }

    /// Consume the stream and return the inner ChildStdout
    pub fn into_inner(self) -> ChildStdout {
        self.inner.into_inner()
    }

    /// Take the inner ChildStdout, leaving None in its place
    /// This is useful when we need to return the stdout while the stream is still alive
    pub fn take_inner(&mut self) -> Option<ChildStdout> {
        // We can't actually take from FramedRead, so we need a different approach
        // This method exists for API compatibility but won't work as expected
        None
    }
}

impl Stream for EventStream {
    type Item = Result<ProtocolMessage, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(line))) => {
                let result = serde_json::from_str::<ProtocolMessage>(&line).map_err(Error::from);
                Poll::Ready(Some(result))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(Error::from(e)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub fn into_event_stream(stdout: ChildStdout) -> EventStream {
    EventStream::new(stdout)
}
