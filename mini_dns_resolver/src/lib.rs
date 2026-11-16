//! A small DNS resolver that walks the hierarchy from the root servers.

pub mod error;
pub mod header;
pub mod ids;
pub mod message;
pub mod name;
pub mod question;
pub mod record;
pub mod rtype;
pub mod transport;
pub mod wire;
