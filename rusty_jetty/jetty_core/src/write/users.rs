//! Functionality to manage the write path for users

pub mod bootstrap;
pub mod diff;
pub mod parser;
mod update;

use std::collections::{BTreeSet, HashMap};

use anyhow::{Context, Result};
use bimap::BiHashMap;
use glob::glob;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{access_graph::NodeName, jetty::ConnectorNamespace, project};

pub use diff::get_membership_diffs;
pub use parser::get_validated_file_config_map;

use super::UpdateConfig;

/// Struct representing user configuration files
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserYaml {
    name: String,
    identifiers: HashMap<ConnectorNamespace, String>,
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    groups: BTreeSet<String>,
}

impl UpdateConfig for UserYaml {
    fn update_user_name(&mut self, old: &String, new: &str) -> Result<bool> {
        if &self.name == old {
            self.name = new.to_owned();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// No-op: if the name in the config is a match, delete the config file.
    fn remove_user_name(&mut self, name: &String) -> Result<bool> {
        Ok(true)
    }

    fn update_group_name(&mut self, old: &String, new: &str) -> Result<bool> {
        if self.groups.remove(old) {
            self.groups.insert(new.to_string());
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn remove_group_name(&mut self, name: &String) -> Result<bool> {
        Ok(self.groups.remove(name))
    }
}

/// Get the paths of all asset config files
fn get_config_paths() -> Result<glob::Paths> {
    // collect the paths to all the config files
    glob(
        format!(
            // the user files can be in whatever directory the user would like
            "{}/**/*.y*ml",
            project::users_cfg_root_path_local().to_string_lossy()
        )
        .as_str(),
    )
    .context("trouble generating config file paths")
}
