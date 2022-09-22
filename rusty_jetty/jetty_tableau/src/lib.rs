#![allow(dead_code, unused)]

mod coordinator;
mod file_parse;
mod nodes;
mod rest;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use jetty_core::{
    connectors::{nodes::ConnectorData, ConnectorClient},
    jetty::{ConnectorConfig, CredentialsBlob},
    Connector,
};
use rest::TableauRestClient;
use serde::Deserialize;
use serde_json::json;
use std::collections::{HashMap, HashSet};

pub type TableauConfig = HashMap<String, String>;

/// Credentials for authenticating with Tableau.
///
/// The user sets these up by following Jetty documentation
/// and pasting their connection info into their connector config.
#[derive(Deserialize, Debug, Default)]
struct TableauCredentials {
    username: String,
    password: String,
    /// Tableau server name like 10ay.online.tableau.com *without* the `https://`
    server_name: String,
    site_name: String,
}

#[allow(dead_code)]
#[derive(Default)]
pub struct TableauConnector {
    config: TableauConfig,
    coordinator: coordinator::Coordinator,
}

#[async_trait]
impl Connector for TableauConnector {
    /// Validates the configs and bootstraps a Tableau connection.
    ///
    /// Validates that the required fields are present to authenticate to
    /// Tableau. Stashes the credentials in the struct for use when
    /// connecting.
    async fn new(
        config: &ConnectorConfig,
        credentials: &CredentialsBlob,
        _client: Option<ConnectorClient>,
    ) -> Result<Box<Self>> {
        let mut creds = TableauCredentials::default();
        let mut required_fields = HashSet::from([
            "server_name".to_owned(),
            "site_name".to_owned(),
            "username".to_owned(),
            "password".to_owned(),
        ]);

        for (k, v) in credentials.iter() {
            match k.as_ref() {
                "server_name" => creds.server_name = v.to_string(),
                "site_name" => creds.site_name = v.to_string(),
                "username" => creds.username = v.to_string(),
                "password" => creds.password = v.to_string(),
                _ => (),
            }

            required_fields.remove(k);
        }

        if !required_fields.is_empty() {
            return Err(anyhow![
                "Snowflake config missing required fields: {:#?}",
                required_fields
            ]);
        }

        let tableau_connector = TableauConnector {
            config: config.config.to_owned(),
            coordinator: coordinator::Coordinator::new(creds).await,
        };

        Ok(Box::new(tableau_connector))
    }

    async fn check(&self) -> bool {
        todo!()
    }

    async fn get_data(&mut self) -> ConnectorData {
        // let mut groups: Vec<jetty_core::connectors::nodes::Group> =
        //     self.coordinator.env.groups.clone().into_values().collect();
        // ConnectorData::new(groups, vec![], vec![], vec![], vec![])
        todo!()
    }
}

#[cfg(test)]
pub(crate) async fn connector_setup() -> Result<crate::TableauConnector> {
    use anyhow::Context;
    use jetty_core::Connector;

    let j = jetty_core::jetty::Jetty::new().context("creating Jetty")?;
    let creds = jetty_core::jetty::fetch_credentials().context("fetching credentials from file")?;
    let config = &j.config.connectors[0];
    let tc = crate::TableauConnector::new(config, &creds["tableau"], None)
        .await
        .context("reading tableau credentials")?;
    Ok(*tc)
}
