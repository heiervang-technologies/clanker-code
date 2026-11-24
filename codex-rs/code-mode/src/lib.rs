#[cfg(feature = "in-process-v8")]
mod cell_actor;
mod provider;
mod remote_session;
#[cfg(feature = "in-process-v8")]
mod runtime;
#[cfg(feature = "in-process-v8")]
mod service;
#[cfg(feature = "in-process-v8")]
mod session_runtime;
#[cfg(feature = "in-process-v8")]
mod v8_init;

#[cfg(feature = "in-process-v8")]
pub(crate) type TaskFailureHandler = std::sync::Arc<dyn Fn(String) + Send + Sync>;

pub use codex_code_mode_protocol::*;
pub use provider::InProcessCodeModeSessionProvider;
pub use provider::NoopCodeModeSessionDelegate;
pub use remote_session::ProcessOwnedCodeModeSession;
pub use remote_session::ProcessOwnedCodeModeSessionProvider;
#[cfg(feature = "in-process-v8")]
pub use service::InProcessCodeModeSession;
#[cfg(feature = "in-process-v8")]
pub use v8_init::V8JitMode;
#[cfg(feature = "in-process-v8")]
pub use v8_init::initialize_v8;
