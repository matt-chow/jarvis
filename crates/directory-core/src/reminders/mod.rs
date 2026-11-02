//! Reminders (D-049): records distinct from tasks, with optional dates,
//! times in the owner's zone, repeat rules, alerts, and per-occurrence
//! completion and snooze. A reminder may reference a task or project.

pub mod model;
pub mod recurrence;
pub mod schedule;
pub mod service;
pub mod zone;

#[cfg(test)]
pub(crate) mod memory;
#[cfg(test)]
mod tests;

pub use model::*;
pub use service::*;
pub use zone::ZoneClock;
