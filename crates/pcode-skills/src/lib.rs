//! Skills: registro, contrato y exportación a cada agente.
pub mod contract;
pub mod export;
pub mod registry;

pub use contract::{Contract, Field, lint, parse_contract};
pub use export::{Target, check, export};
pub use registry::{Registry, Skill, SkillError, parse_skill_md};
