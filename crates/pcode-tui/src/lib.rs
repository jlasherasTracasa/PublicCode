//! `pcode-tui`: interfaz de terminal a pantalla completa.
//!
//! Diseño (AGENTS.md §6.3): todo el estado vive en [`App`]; [`App::update`] es puro y devuelve
//! [`Command`]s, y [`render`] dibuja sin efectos. La E/S (teclado, agente, aprobaciones) está en
//! [`run_tui`], que traduce lo que pasa a [`Event`]s y ejecuta los [`Command`]s.
pub mod app;
pub mod i18n;
pub mod markdown;
pub mod render;
pub mod run;
pub mod theme;

pub use app::{App, Command, Entry, Event, ToolState};
pub use i18n::{Lang, t};
pub use render::render;
pub use run::{OnTurn, TuiOptions, run_tui};
pub use theme::Theme;
