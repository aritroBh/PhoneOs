//! Errors of the simulator (never of the agent core).

use thiserror::Error;

/// Anything that can go wrong setting up or driving the simulator.
#[derive(Debug, Error)]
pub enum SimError {
    /// An app file is not valid JSON for the schema.
    #[error("app definition `{name}`: {source}")]
    AppJson {
        /// File or app name.
        name: String,
        /// The JSON error.
        #[source]
        source: serde_json::Error,
    },
    /// An app definition points at something that does not exist.
    #[error("app `{app}`: {reason}")]
    AppInvalid {
        /// App id.
        app: String,
        /// What is wrong.
        reason: String,
    },
    /// No such app.
    #[error("unknown app `{0}` (see `jarvis-sim apps`)")]
    UnknownApp(String),
    /// No such screen.
    #[error("app `{app}` has no screen `{screen}`")]
    UnknownScreen {
        /// App id.
        app: String,
        /// Screen name.
        screen: String,
    },
    /// An expression could not be parsed.
    #[error(transparent)]
    Expr(#[from] crate::expr::ExprError),
    /// The runner-text round trip failed.
    #[error("runner text could not be parsed: {0}")]
    RunnerText(String),
    /// A task or script file could not be used.
    #[error("{0}")]
    Input(String),
    /// Reading or writing a file failed.
    #[error("{path}: {source}")]
    Io {
        /// The path.
        path: String,
        /// The IO error.
        #[source]
        source: std::io::Error,
    },
}
