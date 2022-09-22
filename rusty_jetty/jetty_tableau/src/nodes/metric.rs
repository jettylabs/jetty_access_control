use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::rest::{self, FetchJson};

use super::Permissionable;

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
pub(crate) struct Metric {
    pub id: String,
    pub name: String,
    pub updated_at: String,
    pub suspended: bool,
    pub project_id: String,
    pub owner_id: String,
    pub underlying_view_id: String,
    pub permissions: Vec<super::Permission>, // Not yet sure if this will be possible
}

fn to_node(val: &serde_json::Value) -> Result<Metric> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct AssetInfo {
        name: String,
        id: String,
        updated_at: String,
        owner: super::IdField,
        project: super::IdField,
        underlying_view: super::IdField,
        suspended: bool,
    }

    let asset_info: AssetInfo =
        serde_json::from_value(val.to_owned()).context("parsing metric information")?;

    Ok(Metric {
        id: asset_info.id,
        name: asset_info.name,
        owner_id: asset_info.owner.id,
        project_id: asset_info.project.id,
        updated_at: asset_info.updated_at,
        suspended: asset_info.suspended,
        underlying_view_id: asset_info.underlying_view.id,
        permissions: Default::default(),
    })
}

pub(crate) async fn get_basic_metrics(
    tc: &rest::TableauRestClient,
) -> Result<HashMap<String, Metric>> {
    let node = tc
        .build_request("metrics".to_owned(), None, reqwest::Method::GET)
        .context("fetching metrics")?
        .fetch_json_response(Some(vec!["metrics".to_owned(), "metric".to_owned()]))
        .await?;
    super::to_asset_map(node, &to_node)
}

impl Permissionable for Metric {
    fn get_endpoint(&self) -> String {
        format!("metrics/{}/permissions", self.id)
    }
    fn set_permissions(&mut self, permissions: Vec<super::Permission>) {
        self.permissions = permissions;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Context, Result};

    #[tokio::test]
    async fn test_fetching_metrics_works() -> Result<()> {
        let tc = crate::connector_setup()
            .await
            .context("running tableau connector setup")?;
        let nodes = get_basic_metrics(&tc.coordinator.rest_client).await?;
        for (_k, v) in nodes {
            println!("{:#?}", v);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_fetching_metric_permissions_works() -> Result<()> {
        let tc = crate::connector_setup()
            .await
            .context("running tableau connector setup")?;
        let mut nodes = get_basic_metrics(&tc.coordinator.rest_client).await?;
        for (_k, v) in &mut nodes {
            v.update_permissions(&tc.coordinator.rest_client).await;
        }
        for (_k, v) in nodes {
            println!("{:#?}", v);
        }
        Ok(())
    }
}
