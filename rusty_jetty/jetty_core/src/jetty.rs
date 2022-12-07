//! Jetty Module
//!
use std::fs;
use std::path::{Path, PathBuf};
use std::{collections::HashMap, fmt::Display};

use anyhow::{anyhow, bail, Context, Result};

use log::{debug, info};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use yaml_peg::serde as yaml;

use crate::access_graph::AccessGraph;
use crate::connectors::ConnectorCapabilities;
use crate::project;

/// The user-defined namespace corresponding to the connector.
#[derive(Clone, Deserialize, Debug, Hash, PartialEq, Eq, Default, PartialOrd, Ord, Serialize)]
pub struct ConnectorNamespace(pub String);

impl Display for ConnectorNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Struct representing the jetty_config.yaml file.
#[allow(dead_code)]
#[derive(Deserialize, Serialize, Debug, Default, Clone)]
pub struct JettyConfig {
    version: String,
    name: String,
    /// All connector configs defined.
    pub connectors: HashMap<ConnectorNamespace, ConnectorConfig>,
    /// Whether the user allows Jetty to collect usage data for analytics.
    #[serde(default = "default_allow_usage_stats")]
    pub allow_anonymous_usage_statistics: bool,
    /// The project id used for telemetry.
    #[serde(default = "new_project_id")]
    pub project_id: String,
}

/// Default to allow for anonymous usage statistics.
fn default_allow_usage_stats() -> bool {
    true
}

/// Create a new random project id. Should only ever be called once
/// per project.
pub fn new_project_id() -> String {
    Uuid::new_v4().to_string()
}

impl JettyConfig {
    /// New === default for this simple constructor.
    pub fn new() -> Self {
        Self {
            version: "0.0.1".to_owned(),
            allow_anonymous_usage_statistics: true,
            ..Default::default()
        }
    }

    /// Use the default filepath to ingest the Jetty config.
    pub fn read_from_file<P: AsRef<Path>>(path: P) -> Result<JettyConfig> {
        let config_raw = fs::read_to_string(&path).context("Reading file")?;
        let mut config =
            yaml::from_str::<JettyConfig>(&config_raw).context("Deserializing config")?;
        // Rewrite any newly created fields (project_id) to the config file.
        fs::write(
            path,
            yaml::to_string(&config[0]).context("Serializing config")?,
        )
        .context("Writing file back")?;

        config.pop().ok_or_else(|| anyhow!["failed"])
    }

    /// Set the project name.
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    /// Get the name
    pub fn get_name(&self) -> String {
        self.name.to_owned()
    }

    /// Convert this config to a yaml string.
    pub fn to_yaml(&self) -> Result<String> {
        yaml::to_string(self).map_err(anyhow::Error::from)
    }
}

/// Config for all connectors in this project.
#[allow(dead_code)]
#[derive(Clone, Deserialize, Serialize, Default, Debug)]
pub struct ConnectorConfig {
    /// The connector type
    #[serde(rename = "type")]
    pub connector_type: String,
    /// Additional configuration, specific to the connector
    #[serde(flatten)]
    pub config: HashMap<String, String>,
}

impl ConnectorConfig {
    /// Basic constructor
    pub fn new(connector_type: String, config: HashMap<String, String>) -> Self {
        Self {
            connector_type,
            config,
        }
    }
}

/// A struct representing the built-in characteristics of a connector.
pub struct ConnectorManifest {
    /// The capabilities of the connector.
    pub capabilities: ConnectorCapabilities,
}

/// Alias for HashMap to hold credentials information.
pub type CredentialsMap = HashMap<String, String>;

/// Fetch the credentials from the Jetty connectors config.
pub fn fetch_credentials(path: PathBuf) -> Result<HashMap<String, CredentialsMap>> {
    debug!("Trying to read credentials from {:?}", path);
    let credentials_raw = fs::read_to_string(path)?;
    let mut config = yaml::from_str::<HashMap<String, CredentialsMap>>(&credentials_raw)?;

    config
        .pop()
        .ok_or_else(|| anyhow!["failed to generate credentials"])
}

/// Represents Jetty Core in its entirety.
pub struct Jetty {
    /// The main jetty_config.yaml
    pub config: JettyConfig,
    // connector_config: HashMap<String, ConnectorCredentials>,
    /// The directory where data (such as the materialized graph) should be stored
    data_dir: PathBuf,
    /// The access graph, if it exists
    pub access_graph: Option<AccessGraph>,
    /// The connector manifests. This gives information about the capabilities of each connector. It's like
    /// the config, but not user-configurable. It also provides a great source for a list of all available connectors.
    connector_manifests: Option<HashMap<ConnectorNamespace, ConnectorManifest>>,
}

impl Jetty {
    /// Convenience method for struct creation. Uses the default location for
    /// config files.
    pub fn new<P: AsRef<Path>>(jetty_config_path: P, data_dir: PathBuf) -> Result<Self> {
        let config =
            JettyConfig::read_from_file(jetty_config_path).context("Reading Jetty Config file")?;

        Ok(Jetty {
            config,
            data_dir,
            access_graph: None,
            connector_manifests: None,
        })
    }

    /// Load access graph from a file
    pub fn load_access_graph(&mut self) -> Result<()> {
        // try to load the graph
        match AccessGraph::deserialize_graph(project::data_dir().join(project::graph_filename())) {
            Ok(mut ag) => {
                // add the tags to the graph
                let tags_path = project::tags_cfg_path_local();
                if tags_path.exists() {
                    debug!("Getting tags from config.");
                    let tag_config = std::fs::read_to_string(&tags_path);
                    match tag_config {
                        Ok(c) => {
                            ag.add_tags(&c)?;
                        }
                        Err(e) => {
                            bail!(
                                "found, but was unable to read {:?}\nerror: {}",
                                tags_path,
                                e
                            )
                        }
                    };
                } else {
                    debug!("No tags file found. Skipping ingestion.")
                };
                self.access_graph = Some(ag);
                Ok(())
            }
            Err(e) => {
                info!(
                    "Unable to find saved graph. Try running `jetty fetch`\nError: {}",
                    e
                );
                Err(e)
            }
        }
    }

    /// Setter for the connector manifests
    pub fn set_connector_manifests(
        &mut self,
        manifests: HashMap<ConnectorNamespace, ConnectorManifest>,
    ) -> Result<()> {
        self.connector_manifests = Some(manifests);
        Ok(())
    }

    /// Getter for a reference to the connector manifests.
    pub fn connector_manifests(&self) -> &Option<HashMap<ConnectorNamespace, ConnectorManifest>> {
        &self.connector_manifests
    }
}
