//! Parse asset configuration files

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::{anyhow, bail, Result};


use crate::{
    access_graph::{AccessGraph, AssetAttributes, NodeName},
    connectors::AssetType,
    jetty::ConnectorNamespace,
    Jetty,
};

use super::{CombinedPolicyState, DefaultPolicyState, PolicyState, YamlAssetDoc, YamlPolicy};

/// Parse the configuration into a policy state struct
pub(crate) fn parse_asset_config(
    val: &str,
    jetty: &Jetty,
    config_groups: &BTreeMap<String, BTreeMap<ConnectorNamespace, NodeName>>,
) -> Result<CombinedPolicyState> {
    let config_vec: Vec<YamlAssetDoc> = yaml_peg::serde::from_str(val)?;
    if config_vec.is_empty() {
        bail!("unable to parse configuration")
    };
    let config = config_vec[0].to_owned();
    let ag = jetty.try_access_graph()?;

    // make sure the asset exists
    let asset_name = get_asset_name(
        &config.identifier.name,
        &config.identifier.asset_type,
        &config.identifier.connector,
        ag,
    )?;

    // Iterate through the normal policies
    let res_policies = parse_policies(
        &asset_name,
        &config.policies,
        &config.identifier.connector,
        config_groups,
        jetty,
    )?;

    // Iterate through the normal policies
    let res_default_policies = parse_default_policies(
        &asset_name,
        &config.default_policies,
        &config.identifier.connector,
        config_groups,
        jetty,
    )?;

    Ok(CombinedPolicyState {
        policies: res_policies,
        default_policies: res_default_policies,
    })
}

fn parse_policies(
    asset_name: &NodeName,
    policies: &BTreeSet<YamlPolicy>,
    connector: &ConnectorNamespace,
    config_groups: &BTreeMap<String, BTreeMap<ConnectorNamespace, NodeName>>,
    jetty: &Jetty,
) -> Result<HashMap<(NodeName, NodeName), PolicyState>> {
    let ag = jetty.try_access_graph()?;
    let mut res_policies = HashMap::new();
    for policy in policies {
        let policy_state = PolicyState {
            privileges: match &policy.privileges {
                Some(p) => p.to_owned().into_iter().collect(),
                None => Default::default(),
            },
            metadata: Default::default(),
        };

        // validate groups
        if let Some(groups) = &policy.groups {
            for group in groups {
                let group_name = get_group_name(group, connector, config_groups)?;
                // Now add the matching group to the results map
                res_policies.insert(
                    (asset_name.to_owned(), group_name.to_owned()),
                    policy_state.to_owned(),
                );
            }
        };

        // Make sure all the users exist
        if let Some(users) = &policy.users {
            for user in users {
                let _user_name = get_user_name(user, ag)?;
                // Now add the matching user to the results map
                // Depending on whether its a default policy or not...

                res_policies.insert(
                    (asset_name.to_owned(), NodeName::User(user.to_owned())),
                    policy_state.to_owned(),
                );
            }
        };

        // Make sure the specified privileges are allowed/exist
        if let Some(privileges) = &policy.privileges {
            privileges_are_legal(privileges, &asset_name, jetty, connector, None)?;
        }
    }
    Ok(res_policies)
}

fn parse_default_policies(
    asset_name: &NodeName,
    default_policies: &BTreeSet<YamlPolicy>,
    connector: &ConnectorNamespace,
    config_groups: &BTreeMap<String, BTreeMap<ConnectorNamespace, NodeName>>,
    jetty: &Jetty,
) -> Result<HashMap<(NodeName, String, BTreeSet<AssetType>), DefaultPolicyState>> {
    let ag = jetty.try_access_graph()?;
    let mut res_policies = HashMap::new();
    for policy in default_policies {
        // validate groups
        let groups: Option<BTreeSet<NodeName>> = if let Some(groups) = &policy.groups {
            Some(
                groups
                    .into_iter()
                    .map(|g| get_group_name(g, connector, config_groups))
                    .collect::<Result<BTreeSet<NodeName>>>()?,
            )
        } else {
            None
        };

        // validate users
        let _users: Option<BTreeSet<NodeName>> = if let Some(users) = &policy.users {
            Some(
                users
                    .into_iter()
                    .map(|u| get_user_name(u, ag))
                    .collect::<Result<BTreeSet<NodeName>>>()?,
            )
        } else {
            None
        };

        // make sure that types are specified and that they are all legal
        match &policy.types {
            Some(types) => {
                let allowed_types = jetty.connector_manifests()[connector]
                    .asset_privileges
                    .to_owned()
                    .into_keys()
                    .collect::<HashSet<_>>();
                for asset_type in types {
                    if !allowed_types.contains(&asset_type) {
                        bail!(
                            "the type `{}` is not allowed for this connector",
                            asset_type.to_string()
                        )
                    }
                }
            }
            None => bail!("asset types must be specified for default policies"),
        }

        // Make sure the specified privileges are allowed/exist
        if let Some(privileges) = &policy.privileges {
            privileges_are_legal(
                privileges,
                &asset_name,
                jetty,
                connector,
                Some(policy.types.to_owned()),
            )?;
        }

        res_policies.insert(
            (
                asset_name.to_owned(),
                policy.path.to_owned().unwrap(),
                policy.types.to_owned().unwrap(),
            ),
            DefaultPolicyState {
                privileges: policy.privileges.to_owned().unwrap_or_default(),
                groups: groups.to_owned().unwrap_or_default(),
                users: groups.to_owned().unwrap_or_default(),
                metadata: policy
                    .metadata
                    .to_owned()
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
            },
        );
    }

    Ok(res_policies)
}

/// Get a nodename for the given group. This checks the config to make sure that the group exists/will exist, and gets the appropriate name for the connector.
/// Returns an error if the group name isn't legal
fn get_group_name(
    group: &String,
    connector: &ConnectorNamespace,
    config_groups: &BTreeMap<String, BTreeMap<ConnectorNamespace, NodeName>>,
) -> Result<NodeName> {
    // make sure the groups exist. Needs info from the group parsing. Use the resolved group name
    let group_name = config_groups
        .get(group)
        .ok_or(anyhow!(
            "unable to find a group called {group} in the configuration"
        ))?
        .get(connector)
        .unwrap();
    Ok(group_name.to_owned())
}

/// Validate that a user exists and get their nodename
fn get_user_name(user: &String, ag: &AccessGraph) -> Result<NodeName> {
    let matching_users = ag
        .graph
        .nodes
        .users
        .keys()
        .filter(|n| match n {
            NodeName::User(graph_user) => {
                if graph_user == user {
                    true
                } else {
                    false
                }
            }
            _ => false,
        })
        .collect::<Vec<_>>();
    if matching_users.is_empty() {
        bail!("unable to find user: {user}")
    }
    if matching_users.len() > 1 {
        bail!("found too many matching users for {user} 😳")
    }
    Ok(matching_users[0].to_owned())
}

/// Validate that an asset exists and get its NodeName
fn get_asset_name(
    name: &String,
    asset_type: &Option<AssetType>,
    connector: &ConnectorNamespace,
    ag: &AccessGraph,
) -> Result<NodeName> {
    let matching_assets = ag
        .graph
        .nodes
        .assets
        .keys()
        .filter(|n| match n {
            NodeName::Asset {
                connector: _ag_connector,
                asset_type: _ag_asset_type,
                path,
            } => {
                if connector == connector && asset_type == asset_type && &path.to_string() == name {
                    true
                } else {
                    false
                }
            }
            _ => false,
        })
        .collect::<Vec<_>>();
    if matching_assets.is_empty() {
        bail!("unable to find asset referenced")
    }
    if matching_assets.len() > 1 {
        bail!("found too many matching assets 😳")
    }

    Ok(matching_assets[0].to_owned())
}

/// determine whether a set of privileges are legal for a policy
fn privileges_are_legal(
    privileges: &BTreeSet<String>,
    asset_name: &NodeName,
    jetty: &Jetty,
    connector: &ConnectorNamespace,
    types: Option<Option<BTreeSet<AssetType>>>,
) -> Result<()> {
    let ag = jetty.try_access_graph()?;
    let connector_privileges = &jetty.connector_manifests()[connector].asset_privileges;
    // if types were passed, it's a default policy
    let allowed_privilege_set = if let Some(t) = types {
        let connector_privileges = &jetty.connector_manifests()[connector].asset_privileges;
        if let Some(ts) = t {
            ts.iter()
                .map(|t| connector_privileges[t].to_owned())
                .flatten()
                .collect::<HashSet<_>>()
        } else {
            connector_privileges
                .values()
                .flatten()
                .map(|v| v.to_owned())
                .collect::<HashSet<_>>()
        }
    }
    // Otherwise it's a normal policy, so get the allowed privileges for that type
    else {
        let asset_attribs = AssetAttributes::try_from(ag.get_node(asset_name)?.to_owned())?;

        connector_privileges[&asset_attribs.asset_type].to_owned()
    };
    for privilege in privileges {
        if !allowed_privilege_set.contains(privilege) {
            bail!("unsupported privilege: {privilege}")
        }
    }
    Ok(())
}

/// Validate that the wildcard path specified is allowed
fn path_is_legal(wildcard_path: &String) -> Result<()> {
    let segments = wildcard_path.split("/").collect::<Vec<_>>();
    let last_element_index = segments.len() - 1;
    for (idx, segment) in segments.into_iter().enumerate() {
        if segment.is_empty() {
            continue;
        }
        // we only allow wildcards
        if segment != "*" && segment != "**" {
            bail!("illegal wildcard path: {wildcard_path}; path elements must be `*` or `**`");
        }
        // "**" can only be used at the end
        if segment == "**" && idx != last_element_index {
            bail!("illegal wildcard path: {wildcard_path}; `**` can only be used at the end of a path");
        }
    }
    Ok(())
}
