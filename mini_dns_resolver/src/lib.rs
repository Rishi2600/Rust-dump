//! A small DNS resolver that walks the hierarchy from the root servers.

pub mod cache;
pub mod classify;
pub mod error;
pub mod header;
pub mod ids;
pub mod message;
pub mod name;
pub mod question;
pub mod record;
pub mod resolver;
pub mod roots;
pub mod rtype;
pub mod transport;
pub mod wire;
