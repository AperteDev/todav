//! One error type for everything the bindings return.

/// Why a call failed. `reason` is in English, for logs and error details; front-ends word the
/// common cases themselves.
#[derive(Debug, uniffi::Error)]
pub enum TodavError {
    /// The server refused the login name or app password.
    Auth,
    /// The account is here already.
    Exists,
    /// Changed on the server meanwhile.
    Conflict,
    NoAccount,
    NotFound {
        reason: String,
    },
    Http {
        code: u16,
        reason: String,
    },
    Net {
        reason: String,
    },
    Parse {
        reason: String,
    },
    Db {
        reason: String,
    },
    /// The account is gone (removed meanwhile).
    Gone,
    /// The keyring did not take the password.
    SavePassword {
        reason: String,
    },
    /// Edits not on the server yet could not go up first; they would be lost.
    Upload {
        reason: String,
    },
    /// The account has edits not on the server yet and is not connected to upload them.
    NotUploaded,
    /// The list or calendar is read-only.
    ReadOnly,
}

impl std::fmt::Display for TodavError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TodavError::Auth => write!(f, "authentication failed"),
            TodavError::ReadOnly => write!(f, "read-only"),
            TodavError::Exists => write!(f, "account already added"),
            TodavError::Conflict => write!(f, "precondition failed (changed on server)"),
            TodavError::NoAccount => write!(f, "no account configured"),
            TodavError::NotFound { reason } => write!(f, "not found: {reason}"),
            TodavError::Http { code, reason } => write!(f, "HTTP {code}: {reason}"),
            TodavError::Net { reason } => write!(f, "network: {reason}"),
            TodavError::Parse { reason } => write!(f, "parse: {reason}"),
            TodavError::Db { reason } => write!(f, "database: {reason}"),
            TodavError::Gone => write!(f, "the account is gone"),
            TodavError::SavePassword { reason } => write!(f, "cannot save password: {reason}"),
            TodavError::Upload { reason } => write!(f, "cannot upload pending changes: {reason}"),
            TodavError::NotUploaded => write!(f, "changes not uploaded yet"),
        }
    }
}

impl std::error::Error for TodavError {}

impl From<todav_core::Error> for TodavError {
    fn from(e: todav_core::Error) -> Self {
        use todav_core::Error as E;
        let reason = e.to_string();
        match e {
            E::Auth => TodavError::Auth,
            E::ReadOnly => TodavError::ReadOnly,
            E::Exists => TodavError::Exists,
            E::Conflict => TodavError::Conflict,
            E::NoAccount => TodavError::NoAccount,
            E::NotFound(reason) => TodavError::NotFound { reason },
            // The server's message as the core shows it, without "HTTP <code>: ".
            E::Http(code, _) => TodavError::Http {
                code,
                reason: reason
                    .split_once(": ")
                    .map_or(reason.clone(), |(_, m)| m.into()),
            },
            E::Net(reason) => TodavError::Net { reason },
            E::Parse(reason) => TodavError::Parse { reason },
            E::Db(reason) => TodavError::Db { reason },
        }
    }
}

impl From<todav_app::accounts::Error> for TodavError {
    fn from(e: todav_app::accounts::Error) -> Self {
        use todav_app::accounts::Error as E;
        match e {
            E::Exists => TodavError::Exists,
            E::Gone => TodavError::Gone,
            E::SavePassword(reason) => TodavError::SavePassword { reason },
            E::Upload(e) => TodavError::Upload {
                reason: e.to_string(),
            },
            E::NotUploaded => TodavError::NotUploaded,
            E::Core(e) => e.into(),
        }
    }
}

pub type Result<T> = std::result::Result<T, TodavError>;
