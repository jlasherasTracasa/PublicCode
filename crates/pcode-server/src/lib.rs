//! `pcoded`: demonio con JSON-RPC, eventos SSE y la página web.
pub mod approvals;
pub mod assist;
pub mod outline;
mod server;
pub mod workspace;

pub use approvals::ApprovalBroker;
pub use server::{AppState, router, serve, serve_listener};
