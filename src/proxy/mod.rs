//! LLM endpoint: capture-only fake endpoint (default) or reverse proxy to an upstream.

pub mod capture;
pub mod handler;
pub mod router;
pub mod server;

pub use server::ProxyHandle;
