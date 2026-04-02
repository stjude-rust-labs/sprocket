//! Manager command types and responses.

use anyhow::Result;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::system::v1::exec::JsonObject;

/// Response for run submission.
#[derive(Debug)]
pub struct SubmitResponse {
    /// The run ID.
    pub id: Uuid,
    /// The generated run name.
    pub name: String,
    /// Events for this run execution.
    pub events: wdl::engine::Events,
    /// Join handle for the run execution task.
    pub handle: JoinHandle<()>,
}

/// Response for run cancellation.
#[derive(Debug)]
pub struct CancelRunResponse {
    /// The run ID.
    pub id: Uuid,
}

/// Commands sent to the run manager.
#[derive(Debug)]
pub enum RunManagerCmd {
    /// Ping the manager to check if it's ready.
    Ping {
        /// Channel to send the response back.
        rx: oneshot::Sender<Result<()>>,
    },

    /// Submit a new run for execution.
    Submit {
        /// WDL source path (local file path or HTTP/HTTPS URL).
        source: String,
        /// Run inputs.
        inputs: JsonObject,
        /// Optional target workflow or task name to execute.
        target: Option<String>,
        /// Optional output directory to index on.
        index_on: Option<String>,
        /// Channel to send the response back.
        rx: oneshot::Sender<Result<SubmitResponse, super::SubmitRunError>>,
    },

    /// Cancel a running run.
    Cancel {
        /// Run ID to cancel.
        id: Uuid,
        /// Channel to send the response back.
        rx: oneshot::Sender<Result<CancelRunResponse, super::CancelRunError>>,
    },

    /// Shutdown the manager gracefully.
    Shutdown {
        /// Channel to send the response back.
        rx: oneshot::Sender<Result<()>>,
    },
}
