//! Parsers for the Source engine file formats that gmod-rust reads from the
//! player's own Garry's Mod installation.
//!
//! All parsers are pure: they take bytes (or a memory map) and produce plain
//! Rust structures. No file format knowledge here comes from engine code; the
//! layouts follow the public Valve Developer Community documentation and are
//! confirmed by the `fmt_*` parity harnesses against the real game.

pub mod bsp;
pub mod keyvalues;
pub mod vpk;
pub mod vtf;

mod reader;

pub use reader::Reader;

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("unexpected end of data at offset {offset}: needed {needed} bytes")]
    Eof { offset: usize, needed: usize },
    #[error("bad magic: expected {expected}, found {found:02x?}")]
    BadMagic {
        expected: &'static str,
        found: Vec<u8>,
    },
    #[error("unsupported {what} version {version}")]
    UnsupportedVersion { what: &'static str, version: u32 },
    #[error("malformed {what}: {detail}")]
    Malformed { what: &'static str, detail: String },
    #[error("string is not valid UTF-8 at offset {offset}")]
    Utf8 { offset: usize },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, FormatError>;
