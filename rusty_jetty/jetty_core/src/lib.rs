//!
//! Access to Jetty
//!
//! Provides all utilities for accessing Jetty connectors and the Jetty Access
//! Graph.
#![deny(missing_docs)]

pub use connectors::Connector;
pub use jetty::fetch_credentials;
pub use jetty::Jetty;

pub mod access_graph;
pub mod connectors;
pub mod cual;
pub mod jetty;
pub mod logging;
pub mod permissions;
pub mod project;
pub mod write;

#[macro_export]
/// Time stuff. For debugging.
macro_rules! time_it {
    ($context:literal, $($tt:tt)+) => {
        debug!("{}: starting", $context);
        let timer = std::time::Instant::now();
        $(
            $tt
        )+
        debug!("{}: {:?}", $context, timer.elapsed());
    }
}

macro_rules! time_it_print {
    ($context:literal, $($tt:tt)+) => {
        println!("{}: starting", $context);
        let timer = std::time::Instant::now();
        $(
            $tt
        )+
        println!("{}: {:?}", $context, timer.elapsed());
    }
}
