use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use jetty_core::{
    connectors::{nodes as jetty_nodes, AssetType},
    cual::Cual,
};
use serde::{Deserialize, Serialize};

use crate::rest::{self, get_tableau_cual, FetchJson, TableauAssetType};

use super::{Permissionable, ProjectId, TableauAsset, LENS};

/// Representation of a Tableau Lens
#[derive(Clone, Default, Debug, Deserialize, Serialize)]
pub(crate) struct Lens {
    pub(crate) cual: Cual,
    pub id: String,
    pub name: String,
    pub datasource_id: String,
    pub project_id: ProjectId,
    pub owner_id: String,
    pub permissions: Vec<super::Permission>,
}

impl Lens {
    pub(crate) fn new(
        cual: Cual,
        id: String,
        name: String,
        datasource_id: String,
        project_id: ProjectId,
        owner_id: String,
        permissions: Vec<super::Permission>,
    ) -> Self {
        Self {
            cual,
            id,
            name,
            datasource_id,
            project_id,
            owner_id,
            permissions,
        }
    }
}

/// Convert JSON to a Lens struct
fn to_node(val: &serde_json::Value) -> Result<Lens> {
    #[derive(Deserialize)]
    struct AssetInfo {
        name: String,
        id: String,
        datasource_id: String,
        owner_id: String,
        project_id: String,
    }

    let asset_info: AssetInfo =
        serde_json::from_value(val.to_owned()).context("parsing lens information")?;

    Ok(Lens {
        cual: get_tableau_cual(TableauAssetType::Lens, &asset_info.id)?,
        id: asset_info.id,
        name: asset_info.name,
        owner_id: asset_info.owner_id,
        project_id: ProjectId(asset_info.project_id),
        datasource_id: asset_info.datasource_id,
        permissions: Default::default(),
    })
}

/// Get basic lense information. Excludes permissions.
pub(crate) async fn get_basic_lenses(
    tc: &rest::TableauRestClient,
) -> Result<HashMap<String, Lens>> {
    let node = tc
        .build_lens_request("askdata/lenses".to_owned(), None, reqwest::Method::GET)
        .context("fetching lenses")?;

    let node = node
        .fetch_json_response(None)
        .await
        .context("fetching and parsing response")?;
    let node = rest::get_json_from_path(&node, &vec!["lenses".to_owned()])?;
    super::to_asset_map(tc, node, &to_node)
}

impl From<Lens> for jetty_nodes::Asset {
    fn from(val: Lens) -> Self {
        jetty_nodes::Asset::new(
            val.cual,
            val.name,
            AssetType(LENS.to_owned()),
            // We will add metadata as it's useful.
            HashMap::new(),
            // Governing policies will be assigned in the policy.
            HashSet::new(),
            // Lenses are children of their datasources?
            HashSet::from([
                get_tableau_cual(TableauAssetType::Datasource, &val.datasource_id)
                    .expect("Getting parent datasource CUAL")
                    .uri(),
            ]),
            // Children objects will be handled in their respective nodes.
            HashSet::new(),
            // Lenses are derived from their source data.
            HashSet::from([
                get_tableau_cual(TableauAssetType::Datasource, &val.datasource_id)
                    .expect("Getting parent datasource CUAL")
                    .uri(),
            ]),
            HashSet::new(),
            // No tags at this point.
            HashSet::new(),
        )
    }
}

impl TableauAsset for Lens {
    fn get_asset_type(&self) -> TableauAssetType {
        TableauAssetType::Lens
    }
}

impl Permissionable for Lens {
    fn get_endpoint(&self) -> String {
        format!("lenses/{}/permissions", self.id)
    }
    fn set_permissions(&mut self, permissions: Vec<super::Permission>) {
        self.permissions = permissions;
    }

    fn get_permissions(&self) -> &Vec<super::Permission> {
        &self.permissions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rest::set_cual_prefix;
    use anyhow::{Context, Result};
    use jetty_core::logging::debug;

    #[tokio::test]
    async fn test_fetching_lenses_works() -> Result<()> {
        let tc = crate::connector_setup()
            .await
            .context("running tableau connector setup")?;
        let nodes = get_basic_lenses(&tc.coordinator.rest_client).await?;
        for (_k, v) in nodes {
            debug!("{:#?}", v);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_fetching_lens_permissions_works() -> Result<()> {
        let tc = crate::connector_setup()
            .await
            .context("running tableau connector setup")?;
        let mut nodes = get_basic_lenses(&tc.coordinator.rest_client).await?;
        for (_k, v) in &mut nodes {
            v.update_permissions(&tc.coordinator.rest_client, &tc.coordinator.env)
                .await?;
        }
        for (_k, v) in nodes {
            debug!("{:#?}", v);
        }
        Ok(())
    }

    #[test]
    #[allow(unused_must_use)]
    fn test_asset_from_lens_works() {
        set_cual_prefix("", "");
        let l = Lens::new(
            Cual::new("".to_owned()),
            "id".to_owned(),
            "name".to_owned(),
            "datasource_id".to_owned(),
            ProjectId("project_id".to_owned()),
            "owner_id".to_owned(),
            vec![],
        );
        jetty_nodes::Asset::from(l);
    }

    #[test]
    #[allow(unused_must_use)]
    fn test_lens_into_asset_works() {
        set_cual_prefix("", "");
        let l = Lens::new(
            Cual::new("".to_owned()),
            "id".to_owned(),
            "name".to_owned(),
            "datasource_id".to_owned(),
            ProjectId("project_id".to_owned()),
            "owner_id".to_owned(),
            vec![],
        );
        Into::<jetty_nodes::Asset>::into(l);
    }
}
