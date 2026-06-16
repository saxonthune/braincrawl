use thiserror::Error;

pub struct WorkId(pub String);

pub enum Kind {
    Abstract,
    Fulltext,
}

#[derive(Debug)]
pub enum Rights {
    Open,
    LinkOnly,
    Restricted,
}

pub struct StoredBlob {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub content_hash: String,
}

pub struct PayloadDescriptor {
    pub key: String,
    pub kind: Kind,
    pub version: u32,
    pub rights: Rights,
    pub mime: String,
}

#[derive(Error, Debug)]
pub enum DomainError {
    #[error("rights violation: {0:?}")]
    RightsViolation(Rights),
    #[error("not found")]
    NotFound,
}
