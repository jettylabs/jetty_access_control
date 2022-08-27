use anyhow::{anyhow, Context, Result};
use graphviz_rust as graphviz;
use graphviz_rust::cmd::CommandArg;
use graphviz_rust::cmd::Format;
use graphviz_rust::printer::PrinterContext;
use petgraph::stable_graph::NodeIndex;
use petgraph::{dot, stable_graph::StableDiGraph};
use std::collections::HashMap;

use super::{EdgeType, JettyNode, NodeName};

pub struct Graph {
    graph: StableDiGraph<JettyNode, EdgeType>,
    /// A map of node identifiers to indecies
    nodes: HashMap<NodeName, NodeIndex>,
}

impl Graph {
    /// Save a svg of the access graph to the specified filename
    pub fn visualize(&self, path: String) -> Result<String> {
        let my_dot = dot::Dot::new(&self.graph);
        let g = graphviz::parse(&format!["{:?}", my_dot]).map_err(|s| anyhow!(s))?;
        let draw = graphviz::exec(
            g,
            &mut PrinterContext::default(),
            vec![CommandArg::Format(Format::Svg), CommandArg::Output(path)],
        )?;
        Ok(draw)
    }
    /// Check whether a given node already exists in the graph
    #[inline(always)]
    pub fn get_node(&self, node: &NodeName) -> Option<&NodeIndex> {
        self.nodes.get(node)
    }
    /// Adds a node to the graph and returns the index.
    pub fn add_node(&mut self, node: &JettyNode) {
        let node_name = node.get_name();
        let idx = self.graph.add_node(node.to_owned());
        self.nodes.insert(node_name, idx);
    }

    /// Updates a node. Should return the updated node. Returns an
    /// error if the nodes are incompatible (would require overwriting values).
    /// To be compatible, metadata from each
    pub fn merge_nodes(&mut self, idx: NodeIndex, new: &JettyNode) -> Result<()> {
        // Fetch node from graph
        let node = &mut self.graph[idx];

        *node = node
            .merge_nodes(new)
            .context(format!["merging: {:?}, {:?}", node, new])?;
        Ok(())
    }

    /// Add edges from cache. Return an error if to/from doesn't exist
    pub fn add_edge(&mut self, edge: super::JettyEdge) -> Result<()> {
        let to = self
            .get_node(&edge.to)
            .ok_or(anyhow!["Unable to find \"to\" node: {:?}", &edge.to])?;

        let from = self
            .get_node(&edge.from)
            .ok_or(anyhow!["Unable to find \"from\" node: {:?}", &edge.from])?;

        self.graph.add_edge(*from, *to, edge.edge_type);
        Ok(())
    }
}
