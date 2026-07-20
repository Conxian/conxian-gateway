use std::{fmt, path::PathBuf};

#[derive(Debug)]
pub enum EvalError {
    Io(String),
    Manifest(String),
    Preflight(String),
    ExecutionRejected {
        reason: String,
        report_path: PathBuf,
    },
}

impl EvalError {
    pub fn execution_rejected(reason: impl Into<String>, report_path: impl Into<PathBuf>) -> Self {
        Self::ExecutionRejected {
            reason: reason.into(),
            report_path: report_path.into(),
        }
    }
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(f, "I/O error: {message}"),
            Self::Manifest(message) => write!(f, "manifest rejected: {message}"),
            Self::Preflight(message) => write!(f, "preflight rejected: {message}"),
            Self::ExecutionRejected {
                reason,
                report_path,
            } => write!(
                f,
                "evaluation rejected ({reason}); report: {}",
                report_path.display()
            ),
        }
    }
}

impl std::error::Error for EvalError {}
