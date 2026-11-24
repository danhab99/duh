use std::error::Error;
use std::fmt;

/// Comprehensive error type for duh operations
#[derive(Debug)]
pub enum DuhError {
    // Object/Data structure errors
    InvalidObjectType { expected: String, found: String },
    ObjectNotFound { hash: String, object_type: String },
    FileNotInCommit { file: String, commit: String },

    // Reference/Branch errors
    RefNotFound(String),
    InvalidRefFormat(String),

    // Remote/Network errors
    UnsupportedRemoteScheme { scheme: String, reason: String },
    RemoteOperationNotImplemented(String),
    RemoteNotFound(String),

    // State errors
    UncommittedChanges,
    DetachedHead { operation: String },
    BareRepoNoWorktree,

    // Filesystem errors
    EditorExitedWithError(i32),
    FileOperationFailed(String),

    // Generic errors
    NoSpace(String),
    Generic(String),
}

impl Error for DuhError {}

impl fmt::Display for DuhError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DuhError::InvalidObjectType { expected, found } => {
                write!(
                    f,
                    "Invalid object type: expected {}, found {}",
                    expected, found
                )
            }
            DuhError::ObjectNotFound { hash, object_type } => {
                write!(f, "{} object not found: {}", object_type, hash)
            }
            DuhError::FileNotInCommit { file, commit } => {
                write!(f, "'{}' isn't in version {}", file, commit)
            }
            DuhError::RefNotFound(r) => {
                write!(f, "Reference '{}' does not exist", r)
            }
            DuhError::InvalidRefFormat(r) => {
                write!(f, "Invalid reference format: {}", r)
            }
            DuhError::UnsupportedRemoteScheme { scheme, reason } => {
                write!(f, "unsupported storage '{}': {}", scheme, reason)
            }
            DuhError::RemoteOperationNotImplemented(op) => {
                write!(f, "Remote operation not yet implemented: {}", op)
            }
            DuhError::UncommittedChanges => {
                write!(f, "you have unsaved changes; save them, un-pick them or bring back your files before opening another draft")
            }
            DuhError::DetachedHead { operation } => {
                write!(
                    f,
                    "can't {} while not on a draft; open one first",
                    operation
                )
            }
            DuhError::BareRepoNoWorktree => {
                write!(f, "this space has no working files (it's bare)")
            }
            DuhError::EditorExitedWithError(code) => {
                write!(
                    f,
                    "Editor exited with error code {}. Commit message not saved.",
                    code
                )
            }
            DuhError::FileOperationFailed(msg) => {
                write!(f, "File operation failed: {}", msg)
            }
            DuhError::NoSpace(msg) => {
                write!(f, "{}", msg)
            }
            DuhError::Generic(msg) => {
                write!(f, "{}", msg)
            }
            DuhError::RemoteNotFound(name) => {
                write!(f, "no place called '{}' (`duh places add` sets one up; `duh get <url>` copies a space)", name)
            }
        }
    }
}

// Keep backward compatibility with NoSpace
pub struct NoSpace {
    pub details: String,
}

impl Error for NoSpace {}

impl fmt::Debug for NoSpace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

impl fmt::Display for NoSpace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

impl NoSpace {
    pub fn new(msg: &str) -> Box<NoSpace> {
        Box::new(NoSpace {
            details: msg.to_string(),
        })
    }
}

// Helper functions for creating errors
impl DuhError {
    pub fn invalid_object(expected: &str, found: &str) -> Self {
        DuhError::InvalidObjectType {
            expected: expected.to_string(),
            found: found.to_string(),
        }
    }

    pub fn object_not_found(hash: &str, object_type: &str) -> Self {
        DuhError::ObjectNotFound {
            hash: hash.to_string(),
            object_type: object_type.to_string(),
        }
    }

    pub fn file_not_in_commit(file: &str, commit: &str) -> Self {
        DuhError::FileNotInCommit {
            file: file.to_string(),
            commit: commit.to_string(),
        }
    }

    pub fn ref_not_found(r: &str) -> Self {
        DuhError::RefNotFound(r.to_string())
    }

    pub fn unsupported_scheme(scheme: &str, reason: &str) -> Self {
        DuhError::UnsupportedRemoteScheme {
            scheme: scheme.to_string(),
            reason: reason.to_string(),
        }
    }

    pub fn detached_head(operation: &str) -> Self {
        DuhError::DetachedHead {
            operation: operation.to_string(),
        }
    }
}
