use super::*;
use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Clone)]
pub(crate) struct Datasource {
    pub id: String,
    pub name: String,
    pub datasource_type: String,
    pub updated_at: String,
    pub project_id: String,
    pub owner_id: String,
    pub datasource_connections: Vec<String>,
    pub permissions: Vec<Permission>,
}

fn to_node(val: &serde_json::Value) -> Result<super::Datasource> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct AssetInfo {
        name: String,
        id: String,
        updated_at: String,
        #[serde(rename = "type")]
        datasource_type: String,
        owner: IdField,
        project: IdField,
        suspended: bool,
    }

    let asset_info: AssetInfo =
        serde_json::from_value(val.to_owned()).context("parsing datasource information")?;

    Ok(super::Datasource {
        id: asset_info.id,
        name: asset_info.name,
        owner_id: asset_info.owner.id,
        project_id: asset_info.project.id,
        updated_at: asset_info.updated_at,
        datasource_type: asset_info.datasource_type,
        permissions: Default::default(),
        datasource_connections: Default::default(),
    })
}
