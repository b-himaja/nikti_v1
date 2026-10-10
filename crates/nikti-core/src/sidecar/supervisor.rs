use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use arrow::ipc::reader::StreamReader;
use arrow::ipc::writer::StreamWriter;

use super::compute_backend::PythonSidecarStatus;
use super::ipc::{handshake_schema, make_message, msg_type, python_env_hash};

#[derive(Debug, thiserror::Error)]
pub enum SidecarError {
    #[error("failed to spawn sidecar process: {0}")]
    Spawn(std::io::Error),
    #[error("sidecar process I/O error: {0}")]
    Io(std::io::Error),
    #[error("sidecar Arrow IPC error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),
    #[error("sidecar sent unexpected message: {0}")]
    UnexpectedMessage(String),
    #[error("sidecar IPC stream ended unexpectedly")]
    StreamEnded,
}

/// Frozen sidecar contract: spawned and supervised by exactly this one
/// Rust-side type — no other module spawns or holds a handle to the
/// Python process. Transport is Arrow IPC over the child's stdio pipes
/// (INV-024: local child process; Part 5 item 14: never JSON).
///
/// This is the handshake/heartbeat skeleton only. Real Analysis procedure
/// dispatch, timeouts, and crash recovery (Rule 9) belong to
/// `PythonAnalysisClient`, which will use a `SidecarSupervisor` internally
/// in a later task — not implemented here.
pub struct SidecarSupervisor {
    child: Child,
    writer: StreamWriter<ChildStdin>,
    reader: StreamReader<ChildStdout>,
    status: PythonSidecarStatus,
}

impl SidecarSupervisor {
    /// Spawns `python_bin -m nikti_sidecar.main` in `sidecar_dir` and
    /// performs the Arrow IPC handshake. On success, `status()` reports
    /// `PythonSidecarStatus::Ready` carrying the environment hash the
    /// sidecar computed at startup (INV-030).
    pub fn spawn(python_bin: &str, sidecar_dir: &Path) -> Result<Self, SidecarError> {
        let mut child = Command::new(python_bin)
            .args(["-m", "nikti_sidecar.main"])
            .current_dir(sidecar_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(SidecarError::Spawn)?;

        let stdin = child.stdin.take().expect("stdin is piped above");
        let stdout = child.stdout.take().expect("stdout is piped above");

        let writer = StreamWriter::try_new(stdin, handshake_schema().as_ref())?;
        let mut reader = StreamReader::try_new(stdout, None)?;

        let handshake = reader.next().ok_or(SidecarError::StreamEnded)??;
        let kind = msg_type(&handshake);
        if kind != "handshake" {
            return Err(SidecarError::UnexpectedMessage(kind));
        }

        let status = PythonSidecarStatus::Ready {
            python_env_hash: python_env_hash(&handshake),
        };

        Ok(Self {
            child,
            writer,
            reader,
            status,
        })
    }

    pub fn status(&self) -> &PythonSidecarStatus {
        &self.status
    }

    /// Sends a ping and blocks for the matching pong.
    pub fn heartbeat(&mut self) -> Result<(), SidecarError> {
        self.writer.write(&make_message("ping", ""))?;

        let batch = self.reader.next().ok_or(SidecarError::StreamEnded)??;
        let kind = msg_type(&batch);
        if kind != "pong" {
            return Err(SidecarError::UnexpectedMessage(kind));
        }

        Ok(())
    }

    /// Sends a shutdown message, closes the write side, and waits for the
    /// child to exit cleanly.
    pub fn shutdown(mut self) -> Result<std::process::ExitStatus, SidecarError> {
        self.writer.write(&make_message("shutdown", ""))?;
        self.writer.finish()?;
        drop(self.writer);

        self.child.wait().map_err(SidecarError::Io)
    }
}
