use serde::{ser::Serializer, Serialize};
use thiserror::Error as ThisError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, ThisError)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    TauriError(tauri::Error),
    #[error("oneshot channel error")]
    OneshotRecvError(#[from] oneshot::RecvError),
    #[error("{0}")]
    StringError(String),
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error::StringError(s.to_string())
    }
}

impl From<tauri::Error> for Error {
    fn from(e: tauri::Error) -> Self {
        Error::StringError(e.to_string())
    }
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}

impl From<futures::channel::oneshot::Canceled> for Error {
    fn from(_: futures::channel::oneshot::Canceled) -> Self {
        Error::StringError("dialog was canceled".into())
    }
}
