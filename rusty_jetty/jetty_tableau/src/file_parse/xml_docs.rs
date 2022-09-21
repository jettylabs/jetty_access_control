use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, bail, Result};
use roxmltree::NodeId;

use super::{
    snowflake_common::{self, build_snowflake_connection_info, SnowflakeQueryInfo},
    NamedConnection, RelationType,
};

/// Represents the different types of relations that we can parse
#[derive(Hash, PartialEq, Eq, Debug)]
enum Relation {
    SnowflakeTable(snowflake_common::SnowflakeTableInfo),
    SnowflakeQuery(snowflake_common::SnowflakeQueryInfo),
}

/// This Macro implements to_cuals for Relation by matching on
/// the inner enum types
macro_rules! impl_to_cuals {
    ($($t:tt),+) => {
        impl Relation {
            fn to_cuals(&self) -> Result<Vec<String>> {
                match self {
                    $(Relation::$t(n) => n.to_cuals(),)*
                }
            }
        }
    }
}

impl_to_cuals!(SnowflakeTable, SnowflakeQuery);

/// Gets cuals from an xml file by parsing the file, pulling out the relevant relations,
/// and building an identifier from it.
#[allow(unused)]
pub(crate) fn parse(data: &str) -> Result<HashSet<String>> {
    let doc = roxmltree::Document::parse(data).unwrap();

    // filter the doc down to the connection info
    let connection_info = doc
        .descendants()
        .find(|n| n.has_tag_name("connection"))
        .ok_or_else(|| anyhow!("unable to find connection_info"))?;

    // pull out the named connections - we'll use these later to get info needed down the road.
    let named_connection_nodes = connection_info
        .descendants()
        .find(|n| n.has_tag_name("named-connections"))
        .ok_or_else(|| anyhow!("unable to find connection information"))?;

    let named_connections = get_named_connections(connection_info);

    // pull out the relations
    let relations = get_relations(connection_info, named_connections);

    let mut cuals = HashSet::new();

    for r in relations {
        let c = r.to_cuals().unwrap_or_else(|e| {
            println!("unable to create qual from {:#?}", r);
            vec![]
        });

        cuals.extend(c);
    }

    Ok(cuals)
}

/// Given a node, look at the children and pull out named connection information.
/// Currently only looks for Snowflake connections.
fn get_named_connections(node: roxmltree::Node) -> HashMap<String, NamedConnection> {
    let mut named_connections = HashMap::new();

    let named_connection_nodes = node
        .children()
        .filter(|n| n.is_element() && n.has_tag_name("named-connection"));
    for n in named_connection_nodes {
        match get_named_connection(&n) {
            Ok(c) => {
                named_connections.insert(c.0, c.1);
            }
            Err(e) => {
                println!("skipping named connection: {}", e);
            }
        };
    }

    named_connections
}

fn get_named_connection(node: &roxmltree::Node) -> Result<(String, NamedConnection)> {
    match get_named_connection_class(&node) {
        Ok(connection_class) => match connection_class.as_str() {
            "snowflake" => {
                let c = snowflake_common::build_snowflake_connection_info(&node)?;
                Ok((c.name.to_owned(), NamedConnection::Snowflake(c.to_owned())))
            }
            _ => bail!("unsupported connection type {}; skipping", connection_class),
        },
        Err(e) => bail!("unable to find connection class for connection node: {}", e),
    }
}

fn get_named_connection_class(node: &roxmltree::Node) -> Result<String> {
    // filter the doc down to the connection info
    let connection_info = node
        .children()
        .find(|n| n.has_tag_name("connection"))
        .ok_or_else(|| anyhow!("unable to find connection class information"))?;

    connection_info
        .attribute("class")
        .map(|class| class.to_owned())
        .ok_or_else(|| anyhow!("unable to find connection class information"))
}

/// Given an XML node, find the embedded relations. It takes multiple passes over the descendants
/// of `node`, but this is generally fast enough not to cause any major issues.
fn get_relations(
    node: roxmltree::Node,
    named_connections: HashMap<String, NamedConnection>,
) -> HashSet<Relation> {
    let mut relations = HashSet::new();

    // get relation nodes
    let relation_nodes = node.descendants().filter(|n| n.has_tag_name("relation"));

    for node in relation_nodes {
        match get_relation(&node, &named_connections) {
            Ok(rel) => {
                relations.insert(rel);
            }
            Err(e) => println!("{}", e),
        }
    }

    relations
}

fn get_relation(
    node: &roxmltree::Node,
    named_connections: &HashMap<String, NamedConnection>,
) -> Result<Relation> {
    let named_connection = node
        .attribute("connection")
        .and_then(|id| named_connections.get(id))
        .ok_or_else(|| anyhow!("unable to find matching connection"))?;

    match get_relation_type(node)? {
        RelationType::SqlQuery => {
            let query = node
                .text()
                .ok_or_else(|| anyhow!("unable to find query text"))?;
            match named_connection {
                NamedConnection::Snowflake(c) => Ok(Relation::SnowflakeQuery(
                    snowflake_common::SnowflakeQueryInfo {
                        query: query.to_string(),
                        db: c.db.to_owned(),
                        server: c.server.to_owned(),
                        schema: c.schema.to_owned(),
                    },
                )),
            }
        }
        RelationType::Table => {
            let table = node
                .attribute("table")
                .ok_or_else(|| anyhow!("unable to find table name"))?;
            match named_connection {
                NamedConnection::Snowflake(c) => Ok(Relation::SnowflakeTable(
                    snowflake_common::SnowflakeTableInfo {
                        table: table.to_string(),
                        db: c.db.to_owned(),
                        server: c.server.to_owned(),
                        schema: c.schema.to_owned(),
                    },
                )),
            }
        }
    }
}

fn get_relation_type(node: &roxmltree::Node) -> Result<super::RelationType> {
    let node_type = node
        .attribute("type")
        .ok_or_else(|| anyhow!("unable to find node type"))?;
    let node_name = node
        .attribute("name")
        .ok_or_else(|| anyhow!("unable to find node name"))?;

    match node_type {
        "table" => Ok(RelationType::Table),
        "text" => {
            if node_type == "Custom SQL Query" {
                Ok(RelationType::SqlQuery)
            } else {
                bail!(
                    "unknown relation type; type: {} name: {}",
                    node_type,
                    node_name
                )
            }
        }
        t => bail!("unknown relation type; type: {}", t),
    }
}
