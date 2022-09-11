//! Graph stuff
//!

use anyhow::{anyhow, Context, Result};
use graphviz_rust as graphviz;
use graphviz_rust::cmd::CommandArg;
use graphviz_rust::cmd::Format;
use graphviz_rust::printer::PrinterContext;
use petgraph::stable_graph::NodeIndex;
use petgraph::{dot, stable_graph::StableDiGraph};
use std::collections::HashMap;

use super::{EdgeType, JettyNode, NodeName};

/// The main graph wrapper
pub struct Graph {
    pub(crate) graph: StableDiGraph<JettyNode, EdgeType>,
    /// A map of node identifiers to indecies
    pub(crate) nodes: HashMap<NodeName, NodeIndex>,
}

impl Graph {
    /// Save a svg of the access graph to the specified filename
    pub fn visualize(&self, path: String) -> Result<String> {
        let my_dot = dot::Dot::new(&self.graph);
        let g = graphviz::parse(&format!["{:?}", my_dot])
            .map_err(|s| anyhow!(s))
            .context("failed to parse")?;
        let draw = graphviz::exec(
            g,
            &mut PrinterContext::default(),
            vec![CommandArg::Format(Format::Svg), CommandArg::Output(path)],
        )
        .context("failed to exec graphviz. do you need to install it?")?;
        Ok(draw)
    }
    /// Check whether a given node already exists in the graph
    #[inline(always)]
    pub fn get_node(&self, node: &NodeName) -> Option<&NodeIndex> {
        self.nodes.get(node)
    }
    /// Adds a node to the graph and returns the index.
    pub(crate) fn add_node(&mut self, node: &JettyNode) -> Result<()> {
        let node_name = node.get_name();
        println!("adding node with name {:?}", node_name);
        // Check for duplicate
        if let Some(&idx) = self.get_node(&node_name) {
            println!(
                "found node potential match - {:?}:{:?}",
                node_name, self.graph[idx]
            );
            self.merge_nodes(idx, node)?;
        } else {
            let idx = self.graph.add_node(node.to_owned());
            self.nodes.insert(node_name, idx);
        };

        Ok(())
    }

    /// Updates a node. Should return the updated node. Returns an
    /// error if the nodes are incompatible (would require overwriting values).
    /// To be compatible, metadata from each
    #[allow(dead_code)]
    pub(crate) fn merge_nodes(&mut self, idx: NodeIndex, new: &JettyNode) -> Result<JettyNode> {
        // Fetch node from graph
        let node = &mut self.graph[idx];

        *node = node
            .merge_nodes(new)
            .context(format!["merging: {:?}, {:?}", node, new])?;
        Ok(node.to_owned())
    }

    /// Add edges from cache. Return an error if to/from doesn't exist
    pub(crate) fn add_edge(&mut self, edge: super::JettyEdge) -> Result<()> {
        let to = self.get_node(&edge.to).ok_or_else(|| {
            anyhow![
                "Unable to find \"to\" node: {:?} for \"from\" {:?}",
                &edge.to,
                &edge.from
            ]
        })?;

        let from = self.get_node(&edge.from).ok_or_else(|| {
            anyhow![
                "Unable to find \"from\" node: {:?} for \"to\" {:?}",
                &edge.from,
                &edge.to
            ]
        })?;

        self.graph.add_edge(*from, *to, edge.edge_type);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::{anyhow, Context, Result};

    use crate::access_graph::GroupAttributes;

    use super::*;
    use std::collections::{HashMap, HashSet};

    /// Test merge_nodes
    #[test]
    fn group_node_same_name_no_conflict() -> Result<()> {
        let mut g = new_graph();

        let original_node = JettyNode::Group(GroupAttributes {
            name: "Group 1".to_string(),
            metadata: HashMap::new(),
            connectors: HashSet::from(["test1".to_string()]),
        });

        // new_node introduces a new connector value
        let new_node = JettyNode::Group(GroupAttributes {
            name: "Group 1".to_string(),
            metadata: HashMap::new(),
            connectors: HashSet::from(["test2".to_string()]),
        });

        // desired output
        let combined_node = JettyNode::Group(GroupAttributes {
            name: "Group 1".to_string(),
            metadata: HashMap::new(),
            connectors: HashSet::from(["test2".to_string(), "test1".to_string()]),
        });

        g.add_node(&original_node)?;

        let &idx = g
            .get_node(&original_node.get_name())
            .ok_or(anyhow!["Unable to find \"to\" node: {:?}", &original_node])?;

        let merged_node = g
            .merge_nodes(idx, &new_node)
            .context(anyhow!["merging nodes"])?;

        assert_eq!(combined_node, merged_node);

        Ok(())
    }

    #[test]
    fn group_node_name_conflict() -> Result<()> {
        let mut g = new_graph();

        let original_node = JettyNode::Group(GroupAttributes {
            name: "Group 1".to_string(),
            metadata: HashMap::new(),
            connectors: HashSet::new(),
        });

        // new_node introduces a connector value
        let new_node = JettyNode::Group(GroupAttributes {
            name: "Group 2".to_string(),
            metadata: HashMap::new(),
            connectors: HashSet::new(),
        });

        g.add_node(&original_node)?;

        let &idx = g
            .get_node(&original_node.get_name())
            .ok_or(anyhow!["Unable to find \"to\" node: {:?}", &original_node])?;

        let merged_node = g
            .merge_nodes(idx, &new_node)
            .context(anyhow!["merging nodes"]);

        assert!(merged_node.is_err());

        Ok(())
    }

    #[test]
    fn group_node_hashmap_conflict() -> Result<()> {
        let mut g = new_graph();

        let original_node = JettyNode::Group(GroupAttributes {
            name: "Group 1".to_string(),
            metadata: HashMap::from([("test1".to_string(), "value2".to_string())]),
            connectors: HashSet::new(),
        });

        // new_node introduces a conflicting metadata value
        let new_node = JettyNode::Group(GroupAttributes {
            name: "Group 1".to_string(),
            metadata: HashMap::from([("test1".to_string(), "other_value".to_string())]),
            connectors: HashSet::new(),
        });

        g.add_node(&original_node)?;

        let &idx = g
            .get_node(&original_node.get_name())
            .ok_or(anyhow!["Unable to find \"to\" node: {:?}", &original_node])?;

        let merged_node = g
            .merge_nodes(idx, &new_node)
            .context(anyhow!["merging nodes"]);

        assert!(merged_node.is_err());

        Ok(())
    }

    #[test]
    fn group_node_hashmap_expand() -> Result<()> {
        let mut g = new_graph();

        let original_node = JettyNode::Group(GroupAttributes {
            name: "Group 1".to_string(),
            metadata: HashMap::from([("test1".to_string(), "value2".to_string())]),
            connectors: HashSet::new(),
        });

        // new_node introduces a new metadata key
        let new_node = JettyNode::Group(GroupAttributes {
            name: "Group 1".to_string(),
            metadata: HashMap::from([("test2".to_string(), "value 3".to_string())]),
            connectors: HashSet::new(),
        });

        // when merged, the result should be:
        let combined_node = JettyNode::Group(GroupAttributes {
            name: "Group 1".to_string(),
            metadata: HashMap::from([
                ("test2".to_string(), "value 3".to_string()),
                ("test1".to_string(), "value2".to_string()),
            ]),
            connectors: HashSet::new(),
        });

        g.add_node(&original_node)?;

        let &idx = g
            .get_node(&original_node.get_name())
            .ok_or(anyhow!["Unable to find \"to\" node: {:?}", &original_node])?;

        let merged_node = g
            .merge_nodes(idx, &new_node)
            .context(anyhow!["merging nodes"])?;

        assert_eq!(merged_node, combined_node);

        Ok(())
    }

    fn new_graph() -> super::Graph {
        super::Graph {
            graph: petgraph::stable_graph::StableDiGraph::new(),
            nodes: HashMap::new(),
        }
    }
}
