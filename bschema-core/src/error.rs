use oxigraph::io::RdfParseError;
use oxigraph::store::{LoaderError, SerializerError, StorageError};

#[derive(thiserror::Error, Debug)]
pub enum BschemaError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("RDF parse error: {0}")]
    Parse(#[from] RdfParseError),
    #[error("RDF load error: {0}")]
    Load(#[from] LoaderError),
    #[error("RDF serialize error: {0}")]
    Serialize(#[from] SerializerError),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unrecognized file extension for {0:?}")]
    UnknownFormat(std::path::PathBuf),
}

pub type Result<T> = std::result::Result<T, BschemaError>;
