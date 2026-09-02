//! LLM reverse proxy: forwards `/v1/*` to the configured upstream and tees traffic into the store.

pub mod handler;
pub mod router;
pub mod server;

pub use server::ProxyHandle;
