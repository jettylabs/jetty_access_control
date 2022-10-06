use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use async_trait::async_trait;
use jetty_core::{
    connectors::{nodes as jetty_nodes, AssetType},
    cual::Cual,
};
use serde::{Deserialize, Serialize};

use crate::{
    coordinator::{Coordinator, HasSources},
    file_parse::xml_docs,
    rest::{self, get_tableau_cual, Downloadable, FetchJson, TableauAssetType},
};

use super::{Permissionable, ProjectId, TableauAsset};

/// Representation of a Tableau Datasource
#[derive(Clone, Default, Debug, Deserialize, Serialize)]
pub(crate) struct Datasource {
    pub(crate) cual: Cual,
    pub id: String,
    pub name: String,
    pub datasource_type: String,
    pub updated_at: String,
    pub project_id: ProjectId,
    pub owner_id: String,
    pub sources: HashSet<String>,
    pub permissions: Vec<super::Permission>,
    /// Vec of origin cuals
    pub derived_from: Vec<String>,
}

impl Datasource {
    pub(crate) fn new(
        cual: Cual,
        id: String,
        name: String,
        datasource_type: String,
        updated_at: String,
        project_id: ProjectId,
        owner_id: String,
        sources: HashSet<String>,
        permissions: Vec<super::Permission>,
        derived_from: Vec<String>,
    ) -> Self {
        Self {
            cual,
            id,
            name,
            datasource_type,
            updated_at,
            project_id,
            owner_id,
            sources,
            permissions,
            derived_from,
        }
    }
}

impl Downloadable for Datasource {
    /// URI Path for asset download
    fn get_path(&self) -> String {
        format!("/datasources/{}/content", &self.id)
    }

    /// Function to match the right filenames to extract from downloaded zip
    fn match_file(name: &str) -> bool {
        name.ends_with(".tds")
    }
}

impl From<Datasource> for jetty_nodes::Asset {
    fn from(val: Datasource) -> Self {
        let ProjectId(project_id) = val.project_id;
        jetty_nodes::Asset::new(
            val.cual,
            val.name,
            AssetType::Other,
            // We will add metadata as it's useful.
            HashMap::new(),
            // Governing policies will be assigned in the policy.
            HashSet::new(),
            // Datasources are children of their projects.
            HashSet::from([get_tableau_cual(TableauAssetType::Project, &project_id)
                .expect("Getting parent project for datasource")
                .uri()]),
            // Children objects will be handled in their respective nodes.
            HashSet::new(),
            // Datasources can be derived from other datasources.
            val.sources,
            // Handled in any child datasources.
            HashSet::new(),
            // No tags at this point.
            HashSet::new(),
        )
    }
}

#[async_trait]
impl HasSources for Datasource {
    fn id(&self) -> &String {
        &self.id
    }

    fn name(&self) -> &String {
        &self.name
    }

    fn updated_at(&self) -> &String {
        &self.updated_at
    }

    fn sources(&self) -> (HashSet<String>, HashSet<String>) {
        (self.sources.to_owned(), HashSet::new())
    }

    async fn fetch_sources(
        &self,
        coord: &Coordinator,
    ) -> Result<(HashSet<String>, HashSet<String>)> {
        // download the source
        let archive = coord.rest_client.download(self, true).await?;
        // get the file
        let file = rest::unzip_text_file(archive, Self::match_file)?;
        // parse the file
        let input_sources = xml_docs::parse(&file)?;
        // datasources don't have output sources (derive_to), so just return an empty set
        let output_sources = HashSet::new();

        Ok((input_sources, output_sources))
    }

    fn set_sources(&mut self, sources: (HashSet<String>, HashSet<String>)) {
        self.sources = sources.0;
    }
}

impl TableauAsset for Datasource {
    fn get_asset_type(&self) -> TableauAssetType {
        TableauAssetType::Datasource
    }
}

/// Convert a JSON value to a Datasource node
fn to_node(val: &serde_json::Value) -> Result<super::Datasource> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct AssetInfo {
        name: String,
        id: String,
        updated_at: String,
        #[serde(rename = "type")]
        datasource_type: String,
        owner: super::IdField,
        project: super::IdField,
    }

    let asset_info: AssetInfo =
        serde_json::from_value(val.to_owned()).context("parsing datasource information")?;

    Ok(super::Datasource {
        cual: get_tableau_cual(TableauAssetType::Datasource, &asset_info.id)?,
        id: asset_info.id,
        name: asset_info.name,
        owner_id: asset_info.owner.id,
        project_id: ProjectId(asset_info.project.id),
        updated_at: asset_info.updated_at,
        datasource_type: asset_info.datasource_type,
        permissions: Default::default(),
        sources: Default::default(),
        derived_from: Default::default(),
    })
}

/// Fetch basic datasource information. Doesn't include permissions or sources. Those need
/// to be fetched separately
pub(crate) async fn get_basic_datasources(
    tc: &rest::TableauRestClient,
) -> Result<HashMap<String, Datasource>> {
    let node = tc
        .build_request("datasources".to_owned(), None, reqwest::Method::GET)
        .context("fetching datasources")?
        .fetch_json_response(Some(vec![
            "datasources".to_owned(),
            "datasource".to_owned(),
        ]))
        .await?;
    super::to_asset_map(tc, node, &to_node)
}

impl Permissionable for Datasource {
    /// URI path to fetch datasource permissions
    fn get_endpoint(&self) -> String {
        format!("datasources/{}/permissions", self.id)
    }

    /// function to set permissions
    fn set_permissions(&mut self, permissions: Vec<super::Permission>) {
        self.permissions = permissions;
    }

    fn get_permissions(&self) -> &Vec<super::Permission> {
        &self.permissions
    }
}

#[cfg(test)]
mod tests {

    use crate::rest::set_cual_prefix;

    use super::*;
    use anyhow::{Context, Result};

    #[tokio::test]
    async fn test_fetching_flows_works() -> Result<()> {
        let tc = crate::connector_setup()
            .await
            .context("running tableau connector setup")?;
        let nodes = get_basic_datasources(&tc.coordinator.rest_client).await?;
        for (_, v) in nodes {
            println!("{:#?}", v);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_fetching_datasource_permissions_works() -> Result<()> {
        let tc = crate::connector_setup()
            .await
            .context("running tableau connector setup")?;
        let mut nodes = get_basic_datasources(&tc.coordinator.rest_client).await?;
        for (_, v) in &mut nodes {
            v.update_permissions(&tc.coordinator.rest_client, &tc.coordinator.env)
                .await;
        }
        for (_, v) in nodes {
            println!("{:#?}", v);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_downloading_datasource_works() -> Result<()> {
        let tc = crate::connector_setup()
            .await
            .context("running tableau connector setup")?;
        let datasources = get_basic_datasources(&tc.coordinator.rest_client).await?;

        let test_datasource = datasources.values().next().unwrap();
        let x = tc
            .coordinator
            .rest_client
            .download(test_datasource, true)
            .await?;
        println!("Downloaded {} bytes", x.len());
        Ok(())
    }

    #[tokio::test]
    async fn test_fetching_sources_for_datasource_works() -> Result<()> {
        let tc = crate::connector_setup()
            .await
            .context("running tableau connector setup")?;
        let datasources = get_basic_datasources(&tc.coordinator.rest_client).await?;

        for test_datasource in datasources.values() {
            test_datasource.fetch_sources(&tc.coordinator).await?;
        }
        Ok(())
    }

    #[test]
    fn test_asset_from_datasource_works() {
        set_cual_prefix("", "");
        let ds = Datasource::new(
            Cual::new("".to_owned()),
            "id".to_owned(),
            "name".to_owned(),
            "datasource_type".to_owned(),
            "updated".to_owned(),
            ProjectId("project_id".to_owned()),
            "owner_id".to_owned(),
            HashSet::new(),
            vec![],
            vec![],
        );
        jetty_nodes::Asset::from(ds);
    }

    #[test]
    fn test_datasource_into_asset_works() {
        set_cual_prefix("", "");
        let ds = Datasource::new(
            Cual::new("".to_owned()),
            "id".to_owned(),
            "name".to_owned(),
            "datasource_type".to_owned(),
            "updated".to_owned(),
            ProjectId("project_id".to_owned()),
            "owner_id".to_owned(),
            HashSet::new(),
            vec![],
            vec![],
        );
        Into::<jetty_nodes::Asset>::into(ds);
    }
}
