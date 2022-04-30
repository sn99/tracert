#[cfg(not(target_os="windows"))]
mod unix;
#[cfg(not(target_os="windows"))]
use unix::trace_route;

#[cfg(target_os="windows")]
mod windows;
#[cfg(target_os="windows")]
use self::windows::trace_route;

mod tracer;
pub use tracer::*;

use std::time::Duration;
use crate::node::Node;

/// Exit status of traceroute
#[derive(Clone, Debug)]
pub enum TraceStatus {
    Done,
    Error,
    Timeout,
}

/// Result of traceroute
#[derive(Clone, Debug)]
pub struct TraceResult {
    pub nodes: Vec<Node>,
    pub status: TraceStatus,
    pub probe_time: Duration,
}
