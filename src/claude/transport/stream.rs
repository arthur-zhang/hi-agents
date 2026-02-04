use crate::claude::Error;
use crate::claude::types::ProtocolMessage;
use futures::StreamExt;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::AsyncWriteExt;
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, LinesCodec};

// ============================================================================
// ReadHalf - 读取并转换 Claude 输出为 UniversalEvent
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