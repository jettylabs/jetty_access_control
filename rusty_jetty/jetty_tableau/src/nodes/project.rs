use std::collections::{HashMap, HashSet};

use super::{FetchPermissions, Permission};
use crate::rest::{self, FetchJson};

use anyhow::{Context, Result};
use jetty_core::{
    connectors::{nodes, AssetType},
    cual::Cual,
};
use serde::Deserialize;

#[derive(Clone, Default, Debug, Deserialize)]
pub(crate) struct Project {
    pub(crate) cual: Cual,
    pub id: String,
    pub name: String,
    pub owner_id: String,
    pub parent_project_id: Option<String>,
    pub controlling_permissions_project_id: Option<String>,
    pub permissions: Vec<Permission>,
}

impl Project {
    pub(crate) fn new(
        cual: Cual,
        id: String,
        name: String,
        owner_id: String,
        parent_project_id: Option<String>,
        controlling_permissions_project_id: Option<String>,
        permissions: Vec<Permission>,
    ) -> Self {
        Self {
            cual,
            id,
            name,
            owner_id,
            parent_project_id,
            controlling_permissions_project_id,
            permissions,
        }
    }
}

fn to_node(val: &serde_json::Value) -> Result<super::Project> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ProjectInfo {
        name: String,
        id: String,
        owner: super::IdField,
        parent_project_id: Option<String>,
        controlling_permissions_project_id: Option<String>,
        updated_at: String,
    }

    let project_info: ProjectInfo =
        serde_json::from_value(val.to_owned()).context("parsing asset information")?;

    Ok(super::Project {
        cual: Cual::new(format!(
            "{}/project/{}",
            tc.get_cual_prefix(),
            project_info.id
        )),
        id: project_info.id,
        name: project_info.name,
        owner_id: project_info.owner.id,
        parent_project_id: project_info.parent_project_id,
        controlling_permissions_project_id: project_info.controlling_permissions_project_id,
        permissions: Default::default(),
    })
}

pub(crate) async fn get_basic_projects(
    tc: &rest::TableauRestClient,
) -> Result<HashMap<String, Project>> {
    let node = tc
        .build_request("projects".to_owned(), None, reqwest::Method::GET)
        .context("fetching projects")?
        .fetch_json_response(Some(vec!["projects".to_owned(), "project".to_owned()]))
        .await?;
    super::to_asset_map(tc, node, &to_node)
}

impl FetchPermissions for Project {
    fn get_endpoint(&self) -> String {
        format!("projects/{}/permissions", self.id)
    }
}

impl From<Project> for nodes::Asset {
    fn from(val: Project) -> Self {
        let parents = val
            .parent_project_id
            .map(|i| HashSet::from([i]))
            .unwrap_or_default();
        nodes::Asset::new(
            val.cual,
            val.name,
            AssetType::Other,
            // We will add metadata as it's useful.
            HashMap::new(),
            // Governing policies will be assigned in the policy.
            HashSet::new(),
            // Projects can be the children of other projects.
            parents,
            // Children objects will be handled in their respective nodes.
            HashSet::new(),
            // Projects aren't derived from/to anything.
            HashSet::new(),
            HashSet::new(),
            // No tags at this point.
            HashSet::new(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Context, Result};
    use jetty_core::connectors::nodes;

    #[tokio::test]
    async fn test_fetching_projects_works() -> Result<()> {
        let tc = crate::connector_setup()
            .await
            .context("running tableau connector setup")?;
        let nodes = get_basic_projects(&tc.coordinator.rest_client).await?;
        for (_k, v) in nodes {
            println!("{:#?}", v);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_fetching_project_permissions_works() -> Result<()> {
        let tc = crate::connector_setup()
            .await
            .context("running tableau connector setup")?;
        let mut nodes = get_basic_projects(&tc.coordinator.rest_client).await?;
        for (_k, v) in &mut nodes {
            v.permissions = v.get_permissions(&tc.coordinator.rest_client).await?;
        }
        for (_k, v) in nodes {
            println!("{:#?}", v);
        }
        Ok(())
    }

    #[test]
    fn test_asset_from_project_works() {
        let wb = Project::new(
            Cual::new("".to_owned()),
            "id".to_owned(),
            "name".to_owned(),
            "owner_id".to_owned(),
            Some("parent_project_id".to_owned()),
            Some("cp_project_id".to_owned()),
            vec![],
        );
        nodes::Asset::from(wb);
    }

    #[test]
    fn test_project_into_asset_works() {
        let wb = Project::new(
            Cual::new("".to_owned()),
            "id".to_owned(),
            "name".to_owned(),
            "owner_id".to_owned(),
            Some("parent_project_id".to_owned()),
            Some("cp_project_id".to_owned()),
            vec![],
        );
        let a: nodes::Asset = wb.into();
    }
}
