use std::fmt;
use std::path::PathBuf;

/// Error that could happen during connecting to LND
///
/// This error may be returned by the `connect()` function if connecting failed.
/// It is currently opaque because it's unclear how the variants will look long-term.
/// Thus you probably only want to display it.
#[derive(Debug)]
pub struct LndConnectError {
    internal: LndInternalConnectError,
}

impl From<LndInternalConnectError> for LndConnectError {
    fn from(value: LndInternalConnectError) -> Self {
        LndConnectError {
            internal: value,
        }
    }
}

#[derive(Debug)]
pub(crate) enum LndInternalConnectError {
    ReadFile { file: PathBuf, error: std::io::Error, },
    ParseCert { file: PathBuf, error: std::io::Error, },
    InvalidAddress { address: String, error: Box<dyn std::error::Error + Send + Sync + 'static>, },
    TlsConfig(tonic::transport::Error),
    Connect { address: String, error: tonic::transport::Error, }
}

impl fmt::Display for LndConnectError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use LndInternalConnectError::*;

        match &self.internal {
            ReadFile { file, .. } => write!(f, "failed to read file {}", file.display()),
            ParseCert { file, .. } => write!(f, "failed to parse certificate {}", file.display()),
            InvalidAddress { address, .. } => write!(f, "invalid address {}", address),
            TlsConfig(_) => write!(f, "failed to configure TLS"),
            Connect { address, .. } => write!(f, "failed to connect to {}", address),
        }
    }
}

impl std::error::Error for LndConnectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        use LndInternalConnectError::*;

        match &self.internal {
            ReadFile { error, .. } => Some(error),
            ParseCert { error, .. } => Some(error),
            InvalidAddress { error, .. } => Some(&**error),
            TlsConfig(error) => Some(error),
            Connect { error, .. } => Some(error),
        }
    }
}
