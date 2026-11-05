//! The services a request may need: a resolved environment and a transport.
//!
//! The router takes these by injection rather than reaching for globals, so a
//! test can drive every method — including the networked ones — with
//! [`crate::transport::MockTransport`] and a hand-written [`Env`].

use std::sync::Arc;
use std::time::Duration;

use crate::env::Env;
use crate::transport::{HttpTransport, UreqTransport};

/// The environment variable naming an extra `.env` file to load on start-up.
pub const ENV_FILE_VAR: &str = "FORGE_ENV_FILE";

/// The default per-request network timeout, in seconds.
///
/// One client serves every adapter, so this is the longest call the forge
/// makes: a grounded completion on a large prompt.
pub const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// What every networked method needs.
pub trait Services: Send + Sync {
    /// The resolved environment.
    fn env(&self) -> &Env;

    /// The shared transport.
    fn transport(&self) -> Arc<dyn HttpTransport>;
}

/// The production wiring: the process environment plus one real client.
pub struct LiveServices {
    env: Env,
    transport: Arc<dyn HttpTransport>,
}

impl LiveServices {
    /// Wire an environment to a transport.
    pub fn new(env: Env, transport: Arc<dyn HttpTransport>) -> Self {
        Self { env, transport }
    }

    /// The process environment, layered over `FORGE_ENV_FILE` when it is set.
    ///
    /// The file path is taken from the environment rather than guessed, so no
    /// absolute user path is ever baked into the binary.
    pub fn from_process() -> Self {
        let file = std::env::var_os(ENV_FILE_VAR).map(std::path::PathBuf::from);
        let env = Env::from_process_with_file(file.as_deref());
        Self::new(
            env,
            Arc::new(UreqTransport::new(Duration::from_secs(DEFAULT_TIMEOUT_SECS))),
        )
    }
}

impl Services for LiveServices {
    fn env(&self) -> &Env {
        &self.env
    }

    fn transport(&self) -> Arc<dyn HttpTransport> {
        Arc::clone(&self.transport)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockTransport;

    #[test]
    fn services_expose_the_injected_environment_and_transport() {
        let mut env = Env::new();
        env.set("GROQ_API_KEY", "gsk_test");
        let services = LiveServices::new(env, Arc::new(MockTransport::new()));
        assert_eq!(services.env().get("GROQ_API_KEY"), Some("gsk_test"));
        // The transport is shared by handle, not rebuilt per call.
        assert!(Arc::ptr_eq(&services.transport(), &services.transport()));
    }
}
