//! # Access Graph
//!
//! `access_graph` is a library for modeling data access permissions and metadata as a graph.

use std::collections::HashMap;

use anyhow::{context, *};

use graphviz_rust as graphviz;
use graphviz_rust::cmd::CommandArg;
use graphviz_rust::cmd::Format;
use graphviz_rust::printer::PrinterContext;
use petgraph::dot;
use petgraph::stable_graph::StableDiGraph;

/// Attributes associated with a User node
#[derive(Debug)]
struct UserAttributes {
    name: String,
}

/// Attributes associated with a Group node
#[derive(Debug)]
struct GroupAttributes {
    name: String,
}

/// Enum of node types
#[derive(Debug)]
enum JettyNode {
    Group(GroupAttributes),
    User(UserAttributes),
}

/// Enum of edge types
#[derive(Debug)]
enum JettyEdge {
    MemberOf,
    Includes,
}

/// Mapping of node identifiers (like asset name) to their id in the graph
struct NodeMap {
    users: HashMap<String, usize>,
    groups: HashMap<String, usize>,
    assets: HashMap<String, usize>,
    policies: HashMap<String, usize>,
    tags: HashMap<String, usize>,
}

/// Representation of data access state
pub struct AccessGraph {
    graph: StableDiGraph<JettyNode, JettyEdge>,
    nodes: NodeMap,
}

impl AccessGraph {
    /// Save a svg of the access graph to the specified filename
    pub fn visualize(&self, path: String) -> anyhow::Result<String> {
        let my_dot = dot::Dot::new(&self.graph);
        let g = graphviz::parse(&format!["{:?}", my_dot])
            .context("Failed to parse dot object into Graphviz")?;
        let draw = graphviz::exec(
            g,
            &mut PrinterContext::default(),
            vec![CommandArg::Format(Format::Svg), CommandArg::Output(path)],
        )?;
        Ok(draw)
    }
}