//! Error types for the native-first converter.

use crate::ids::SigCode;

/// Errors that can occur when reading a record from a plugin handle.
#[derive(Debug)]
pub enum RecordReadError {
    /// The requested form_key string could not be parsed.
    InvalidFormKey(String),
    /// No record with the given form_key exists in the plugin.
    NotFound(String),
    /// The record's signature does not match any schema entry.
    UnknownSignature(SigCode),
    /// Schema / binary mismatch during subrecord decode.
    SchemaMismatch {
        sig: SigCode,
        subrec: String,
        reason: String,
    },
    /// A low-level decode error occurred.
    Decode(DecodeError),
}

impl std::fmt::Display for RecordReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFormKey(s) => write!(f, "invalid form key: {s}"),
            Self::NotFound(s) => write!(f, "record not found: {s}"),
            Self::UnknownSignature(sig) => write!(f, "unknown record signature: {}", sig.as_str()),
            Self::SchemaMismatch {
                sig,
                subrec,
                reason,
            } => {
                write!(f, "schema mismatch for {}/{subrec}: {reason}", sig.as_str())
            }
            Self::Decode(e) => write!(f, "decode error: {e}"),
        }
    }
}

impl std::error::Error for RecordReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(e) => Some(e),
            _ => None,
        }
    }
}

/// A low-level byte decode error.
#[derive(Debug)]
pub enum DecodeError {
    /// A codec name we don't handle yet.
    UnknownCodec(String),
    /// Binary data was too short for the expected layout.
    Truncated { expected: usize, got: usize },
    /// A null-terminated string was not null-terminated.
    MalformedString(String),
    /// A FormID in the binary data could not be resolved to a form key.
    UnresolvableFormId(u32),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownCodec(name) => write!(f, "unknown codec: {name:?}"),
            Self::Truncated { expected, got } => {
                write!(f, "truncated: expected {expected} bytes, got {got}")
            }
            Self::MalformedString(s) => write!(f, "malformed string: {s}"),
            Self::UnresolvableFormId(id) => write!(f, "unresolvable form_id: 0x{id:08X}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Errors that can occur when loading a translation map or transform registry.
#[derive(Debug)]
pub enum ConfigError {
    /// A translation map YAML file was expected but not found.
    MapFileMissing(std::path::PathBuf),
    /// A translation map YAML file could not be parsed.
    MapFileMalformed {
        path: std::path::PathBuf,
        source: String,
    },
    /// A transform name referenced in a map is not registered.
    UnknownTransform(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MapFileMissing(p) => write!(f, "translation map file missing: {}", p.display()),
            Self::MapFileMalformed { path, source } => {
                write!(
                    f,
                    "translation map malformed at {}: {source}",
                    path.display()
                )
            }
            Self::UnknownTransform(name) => write!(f, "unknown transform: {name:?}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Errors that can occur when writing a record to a plugin handle.
#[derive(Debug)]
pub enum WriteError {
    /// No plugin handle with the given ID exists.
    InvalidHandle(u64),
    /// The record's signature is not defined in the schema.
    UnknownSignature(SigCode),
    /// A subrecord signature is not defined for this record type in the schema.
    UnknownSubrecord {
        record_sig: SigCode,
        subrec_sig: String,
    },
    /// A field value type is incompatible with the schema codec.
    TypeMismatch {
        field: String,
        expected: String,
        got: String,
    },
    /// A subrecord byte-encoding step failed.
    EncodeFailure(String),
    /// The low-level insertion into the plugin handle failed.
    InsertFailure(String),
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidHandle(id) => write!(f, "invalid plugin handle: {id}"),
            Self::UnknownSignature(sig) => {
                write!(f, "unknown record signature: {}", sig.as_str())
            }
            Self::UnknownSubrecord {
                record_sig,
                subrec_sig,
            } => {
                write!(
                    f,
                    "unknown subrecord {subrec_sig:?} in record {}",
                    record_sig.as_str()
                )
            }
            Self::TypeMismatch {
                field,
                expected,
                got,
            } => {
                write!(
                    f,
                    "type mismatch for field {field:?}: expected {expected}, got {got}"
                )
            }
            Self::EncodeFailure(msg) => write!(f, "encode failure: {msg}"),
            Self::InsertFailure(msg) => write!(f, "insert failure: {msg}"),
        }
    }
}

impl std::error::Error for WriteError {}
