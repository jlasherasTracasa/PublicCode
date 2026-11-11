//! IDE nativo de PublicCode: un cliente más del demonio `pcoded` (JSON-RPC y SSE), sin navegador.
pub mod agent;
pub mod app;
pub mod events;
pub mod find;
pub mod git;
pub mod highlight;
pub mod lsp_pos;
pub mod problems;
pub mod rpc;
pub mod settings;
pub mod state;
pub mod terminal;
pub mod texts;
