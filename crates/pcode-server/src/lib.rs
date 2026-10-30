//! `pcoded`: demonio con JSON-RPC, eventos SSE y la página web.
pub mod approvals;
mod server;

pub use approvals::ApprovalBroker;
pub use server::{AppState, router, serve};
