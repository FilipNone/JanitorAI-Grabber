//! LLM reverse proxy that forwards `/v1/*` to the configured upstream and stores the traffic.

pub mod handler;
pub mod router;
pub mod server;

pub use server::ProxyHandle;
