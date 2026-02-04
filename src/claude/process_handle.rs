//! Process handle for managing subprocess lifecycle.
//!
//! This module provides a safe wrapper around the subprocess that allows
//! controlling its lifecycle without direct access to the underlying process.

use crate::claude::types::{Error, Result};
use tokio::process::Child;

/// Handle for managing a subprocess.
///
/// Provides methods to control the subprocess lifecycle (kill, wait, etc.)
/// without exposing the underlying Child object. This allows safe concurrent
/// access to process management operations while other tasks handle I/O.
///
/// # Example
///
/// ```rust,no_run
/// use claude_agent_sdk::internal::transport::{SubprocessCLITransport, PromptInput};
/// use claude_agent_sdk::ClaudeAgentOptions;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let options = ClaudeAgentOptions::new();
///     let prompt = PromptInput::String("Hello!".to_string());
///
///     let mut transport = SubprocessCLITransport::new(prompt, options)?;
///     transport.connect().await?;
///
///     let (read_half, write_half, stderr_half, mut process_handle) = transport.split()?;
///
///     // Check process status
///     if let Some(pid) = process_handle.id() {
///         println!("Process ID: {}", pid);
///     }
///
///     // Wait for process to complete
///     let status = process_handle.wait().await?;
///     println!("Process exited with: {:?}", status);
///
///     Ok(())
/// }
/// ```
pub struct ProcessHandle {
    child: Child,
}

impl ProcessHandle {
    /// Create a new process handle from a Child process.
    ///
    /// # Arguments
    ///
    /// * `child` - The tokio Child process to wrap
    pub fn new(child: Child) -> Self {
        Self { child }
    }

    /// Terminate the process forcefully.
    ///
    /// Sends a SIGKILL signal to the process. This is a forceful termination
    /// and the process will not have a chance to clean up.
    ///
    /// # Errors
    ///
    /// Returns an error if the kill operation fails.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use claude_agent_sdk::internal::transport::ProcessHandle;
    /// # async fn example(mut handle: ProcessHandle) -> Result<(), Box<dyn std::error::Error>> {
    /// // Terminate the process
    /// handle.kill().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn kill(&mut self) -> Result<()> {
        self.child
            .kill()
            .await
            .map_err(|e| Error::Process(format!("Failed to kill process: {}", e)))
    }

    /// Wait for the process to exit and return its status.
    ///
    /// This is a blocking operation that waits until the process terminates.
    /// Use this to ensure the process has completed before proceeding.
    ///
    /// # Returns
    ///
    /// The exit status of the process.
    ///
    /// # Errors
    ///
    /// Returns an error if waiting fails.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use claude_agent_sdk::internal::transport::ProcessHandle;
    /// # async fn example(mut handle: ProcessHandle) -> Result<(), Box<dyn std::error::Error>> {
    /// let status = handle.wait().await?;
    /// if status.success() {
    ///     println!("Process completed successfully");
    /// } else {
    ///     println!("Process failed with code: {:?}", status.code());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn wait(&mut self) -> Result<std::process::ExitStatus> {
        self.child
            .wait()
            .await
            .map_err(|e| Error::Process(format!("Failed to wait for process: {}", e)))
    }

    /// Check if the process has exited without blocking.
    ///
    /// This is a non-blocking check that returns immediately. Use this to
    /// poll the process status without waiting.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(status))` if the process has exited
    /// - `Ok(None)` if the process is still running
    ///
    /// # Errors
    ///
    /// Returns an error if the status check fails.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use claude_agent_sdk::internal::transport::ProcessHandle;
    /// # fn example(mut handle: ProcessHandle) -> Result<(), Box<dyn std::error::Error>> {
    /// match handle.try_wait()? {
    ///     Some(status) => println!("Process exited with: {:?}", status),
    ///     None => println!("Process is still running"),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>> {
        self.child
            .try_wait()
            .map_err(|e| Error::Process(format!("Failed to check process status: {}", e)))
    }

    /// Get the process ID.
    ///
    /// Returns the OS-assigned process ID (PID) if available. This may return
    /// `None` if the process has already exited or on some platforms.
    ///
    /// # Returns
    ///
    /// The process ID, or `None` if not available.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use claude_agent_sdk::internal::transport::ProcessHandle;
    /// # fn example(handle: ProcessHandle) {
    /// if let Some(pid) = handle.id() {
    ///     println!("Process ID: {}", pid);
    /// }
    /// # }
    /// ```
    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }
}
