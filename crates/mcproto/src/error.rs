use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// The peer closed the connection.
    Closed,
    /// A packet ended before all its fields were read.
    Truncated,
    VarIntTooLong,
    FrameTooLarge(usize),
    InvalidString,
    StringTooLong {
        len: usize,
        max: usize,
    },
    Zlib(String),
    /// Anything else malformed, with a description.
    Malformed(String),
    Crypto(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Closed => write!(f, "connection closed"),
            Error::Truncated => write!(f, "packet is shorter than its fields"),
            Error::VarIntTooLong => write!(f, "VarInt is too long"),
            Error::FrameTooLarge(n) => write!(f, "packet of {n} bytes is too large"),
            Error::InvalidString => write!(f, "string is not valid UTF-8"),
            Error::StringTooLong { len, max } => write!(f, "string of {len} chars exceeds {max}"),
            Error::Zlib(e) => write!(f, "compression error: {e}"),
            Error::Malformed(e) => write!(f, "malformed packet: {e}"),
            Error::Crypto(e) => write!(f, "encryption error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            Error::Closed
        } else {
            Error::Io(e)
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
