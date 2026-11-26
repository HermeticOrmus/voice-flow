//! voice-flow core: pure logic with no I/O. Every function takes its inputs as arguments and
//! returns a value, so the same code runs in the engine, the injector, the command and the tests.

pub mod conf;
pub mod dirs;
pub mod express;
pub mod filters;
pub mod history;
pub mod profile;
mod prompts;
pub mod subst;
pub mod text;
