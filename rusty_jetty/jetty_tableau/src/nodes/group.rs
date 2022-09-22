use std::collections::{HashMap, HashSet};

use crate::rest::{self, FetchJson};
use anyhow::{Context, Result};
use jetty_core::connectors::nodes;
use serde::Deserialize;

#[derive(Clone, Default, Debug, Deserialize)]
pub(crate) struct Group {
    pub id: String,
    pub name: String,
    /// Vec of user uids
    pub includes: Vec<String>,
}

impl Group {
    pub(crate) fn new(id: String, name: String, includes: Vec<String>) -> Self {
        Self { id, name, includes }
    }
}

impl From<Group> for nodes::Group {
    fn from(val: Group) -> Self {
        nodes::Group::new(
            val.name,
            HashMap::from([("tableau::id".to_owned(), val.id)]),
            // TODO: No nested groups in tableau?
            HashSet::new(),
            HashSet::from_iter(val.includes),
            // TODO: No nested groups in tableau?
            HashSet::new(),
            // Handled in permissions/policies.
            HashSet::new(),
        )
    }
}

pub(crate) fn to_node(tc: &rest::TableauRestClient, val: &serde_json::Value) -> Result<Group> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct GroupInfo {
        name: String,
        id: String,
    }
    let group_info: GroupInfo =
        serde_json::from_value(val.to_owned()).context("parsing group information")?;

    Ok(Group {
        id: group_info.id,
        name: group_info.name,
        includes: Vec::new(),
    })
}

pub(crate) async fn get_basic_groups(
    tc: &rest::TableauRestClient,
) -> Result<HashMap<String, Group>> {
    let node = tc
        .build_request("groups".to_owned(), None, reqwest::Method::GET)
        .context("fetching groups")?
        .fetch_json_response(Some(vec!["groups".to_owned(), "group".to_owned()]))
        .await?;
    super::to_asset_map(tc, node, &to_node)
}

async fn get_group_users(
    tc: &rest::TableauRestClient,
    groups: &mut HashMap<String, Group>,
) -> Result<()> {
    for (id, group) in groups {
        let resp = tc
            .build_request(format!("groups/{}/users", id), None, reqwest::Method::GET)
            .context("fetching group membership")?
            .fetch_json_response(Some(vec!["users".to_owned(), "user".to_owned()]))
            .await
            .context(format!("getting membership for group {}", group.name))?;

        let user_vec: Vec<super::IdField> =
            serde_json::from_value(resp).context("parsing group membership")?;
        group.includes = user_vec.iter().map(|u| u.id.to_owned()).collect();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Context, Result};

    #[tokio::test]
    async fn test_fetching_groups_works() -> Result<()> {
        let tc = crate::connector_setup()
            .await
            .context("running tableau connector setup")?;
        let groups = get_basic_groups(&tc.coordinator.rest_client).await?;
        for (_k, v) in groups {
            println!("{:#?}", v);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_fetching_groups_with_users_works() -> Result<()> {
        let tc = crate::connector_setup()
            .await
            .context("running tableau connector setup")?;
        let mut groups = get_basic_groups(&tc.coordinator.rest_client).await?;
        get_group_users(&tc.coordinator.rest_client, &mut groups).await?;
        for (_k, v) in groups {
            println!("{:#?}", v);
        }
        Ok(())
    }

    #[test]
    fn test_jetty_group_from_group_works() {
        let g = Group::new("id".to_owned(), "name".to_owned(), vec!["me".to_owned()]);
        nodes::Group::from(g);
    }

    #[test]
    fn test_group_into_jetty_group_works() {
        let g = Group::new("id".to_owned(), "name".to_owned(), vec!["me".to_owned()]);
        let a: nodes::Group = g.into();
    }
}
