use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Result};
use serde::Deserialize;

use super::FlowDoc;
use crate::file_parse::snowflake_common;

#[derive(Deserialize)]
struct ConnectionAttributes {
    schema: String,
    dbname: String,
}

fn get_server_info(doc: &FlowDoc, connection_id: &String) -> Result<String> {
    Ok(doc.connections[connection_id]
        .get("connectionAttributes")
        .and_then(|v| v.get("server"))
        .and_then(|v| v.as_str())
        .unwrap()
        .to_owned())
}

pub(super) fn get_input_table_cuals(
    doc: &FlowDoc,
    node: &serde_json::Value,
) -> Result<HashSet<String>> {
    #[derive(Deserialize)]
    struct TableRelation {
        table: String,
    }

    #[derive(Deserialize)]
    struct TableInfo {
        #[serde(rename = "connectionAttributes")]
        connection_attributes: ConnectionAttributes,
        #[serde(rename = "connectionId")]
        connection_id: String,
        relation: TableRelation,
    }

    let table_info: TableInfo = serde_json::from_value(node.to_owned())?;
    let server = get_server_info(doc, &table_info.connection_id)?;

    let snowflake_table = crate::file_parse::snowflake_common::SnowflakeTableInfo {
        table: table_info.relation.table,
        connection: table_info.connection_id.to_owned(),
    };
    let connections = HashMap::from([(
        table_info.connection_id.to_owned(),
        crate::file_parse::NamedConnection::Snowflake(
            crate::file_parse::snowflake_common::SnowflakeConnectionInfo {
                name: table_info.connection_id,
                db: table_info.connection_attributes.dbname,
                server,
                schema: table_info.connection_attributes.schema,
            },
        ),
    )]);
    let mut cuals = HashSet::new();
    cuals.extend(snowflake_table.to_cuals(&connections)?);

    Ok(cuals)
}

pub(super) fn get_output_table_cuals(
    doc: &FlowDoc,
    node: &serde_json::Value,
) -> Result<HashSet<String>> {
    #[derive(Deserialize)]
    struct OutputDbAttributes {
        schema: String,
        dbname: String,
        warehouse: String,
        tablename: String,
    }

    #[derive(Deserialize)]
    struct TableInfo {
        attributes: OutputDbAttributes,
        #[serde(rename = "connectionId")]
        connection_id: String,
    }

    let mut relations = HashSet::new();

    let table_info: TableInfo = serde_json::from_value(node.to_owned())?;
    let server = get_server_info(doc, &table_info.connection_id)?;

    let mut table = table_info.attributes.tablename;
    // Fix up the table name:
    if table.starts_with('"') {
        table = table.trim_matches('"').to_owned();
    } else if table.starts_with("'") {
        table = table.trim_matches('\'').to_owned();
    } else if table.starts_with('[') {
        table = table.trim_matches('[').to_owned();
        table = table.trim_matches(']').to_owned();
    } else if table.starts_with('`') {
        table = table.trim_matches('`').to_owned();
    }

    let snowflake_table = crate::file_parse::snowflake_common::SnowflakeTableInfo {
        table,
        connection: table_info.connection_id.to_owned(),
    };
    let connections = HashMap::from([(
        table_info.connection_id.to_owned(),
        crate::file_parse::NamedConnection::Snowflake(
            crate::file_parse::snowflake_common::SnowflakeConnectionInfo {
                name: table_info.connection_id,
                db: table_info.attributes.dbname,
                server,
                schema: table_info.attributes.schema,
            },
        ),
    )]);

    relations.extend(snowflake_table.to_cuals(&connections)?);

    Ok(relations)
}

pub(super) fn get_input_query_cuals(
    doc: &FlowDoc,
    node: &serde_json::Value,
) -> Result<HashSet<String>> {
    #[derive(Deserialize)]
    struct QueryRelation {
        query: String,
    }

    #[derive(Deserialize)]
    struct QueryInfo {
        #[serde(rename = "connectionAttributes")]
        connection_attributes: ConnectionAttributes,
        #[serde(rename = "connectionId")]
        connection_id: String,
        relation: QueryRelation,
    }

    let mut relations = HashSet::new();

    let table_info: QueryInfo = serde_json::from_value(node.to_owned())?;
    let server = get_server_info(doc, &table_info.connection_id)?;

    let snowflake_table = crate::file_parse::snowflake_common::SnowflakeQueryInfo {
        query: table_info.relation.query,
        connection: table_info.connection_id.to_owned(),
    };
    let connections = HashMap::from([(
        table_info.connection_id.to_owned(),
        crate::file_parse::NamedConnection::Snowflake(
            crate::file_parse::snowflake_common::SnowflakeConnectionInfo {
                name: table_info.connection_id,
                db: table_info.connection_attributes.dbname,
                server,
                schema: table_info.connection_attributes.schema,
            },
        ),
    )]);

    relations.extend(snowflake_table.to_cuals(&connections)?);
    Ok(relations)
}
