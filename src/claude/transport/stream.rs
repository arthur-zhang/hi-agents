use crate::claude::Error;
use crate::claude::types::ProtocolMessage;
use futures::{Stream, StreamExt};
use std::sync::atomic::{AtomicU64, Ordering};
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

pub fn into_event_stream(
    stdout: ChildStdout,
) -> impl Stream<Item = Result<ProtocolMessage, Error>> {
    let stream = FramedRead::new(stdout, LinesCodec::new());
    stream.map(|it| {
        it.map_err(Error::from).and_then(|line| {
            serde_json::from_str::<ProtocolMessage>(&line).map_err(Error::from)
        })
    })
}
