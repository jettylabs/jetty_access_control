//! Full CLI library for Jetty Core
//!

#![deny(missing_docs)]

mod ascii;
mod cmd;
mod diff;
mod init;
mod plan;
mod remove;
mod rename;
mod tui;
mod usage_stats;

use std::{
    collections::HashMap,
    env, fs,
    str::FromStr,
    sync::Arc,
    time::{self, Instant},
};

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use colored::Colorize;
use human_panic::setup_panic;
use indicatif::{ProgressBar, ProgressStyle};

use jetty_core::{
    access_graph::AccessGraph,
    connectors::{ConnectorClient, NewConnector},
    fetch_credentials,
    jetty::{ConnectorNamespace, CredentialsMap, JettyConfig},
    logging::{self, debug, error, info, warn},
    project::{self, groups_cfg_path_local},
    write::{
        assets::bootstrap::{update_asset_files, write_bootstrapped_asset_yaml},
        diff::get_diffs,
        new_groups,
        users::bootstrap::{update_user_files, write_bootstrapped_user_yaml},
    },
    Connector, Jetty,
};

use crate::{
    cmd::{JettyArgs, JettyCommand},
    usage_stats::{record_usage, UsageEvent},
};

/// Main CLI entrypoint.
pub async fn cli() -> Result<()> {
    // Setup logging
    let reload_handle = logging::setup(None);

    // Setup panic handler
    setup_panic!(Metadata {
        name: env!("CARGO_PKG_NAME").into(),
        version: env!("CARGO_PKG_VERSION").into(),
        authors: "Jetty Support <support@get-jetty.com>".into(),
        homepage: "get-jetty.com".into(),
    });
    // Get Jetty Config
    let jetty_config = JettyConfig::read_from_file(project::jetty_cfg_path_local()).ok();
    // Get args
    let args = if env::args().count() == 1 {
        // Invoke telemetry for empty args. If we executed `JettyArgs::parse()` first,
        // the program would exit before we got to publish usage.
        record_usage(UsageEvent::InvokedDefault, &jetty_config)
            .await
            .unwrap_or_else(|_| debug!("Failed to publish usage."));
        JettyArgs::parse()
    } else {
        let args = JettyArgs::parse();
        // Invoke telemetry
        let event = match args.command {
            JettyCommand::Init { .. } => UsageEvent::InvokedInit,
            JettyCommand::Fetch { .. } => UsageEvent::InvokedFetch {
                connector_types: if let Some(c) = &jetty_config {
                    c.connectors
                        .values()
                        .map(|c| c.connector_type.to_owned())
                        .collect()
                } else {
                    vec![]
                },
            },
            JettyCommand::Explore { .. } => UsageEvent::InvokedExplore,
            JettyCommand::Add => UsageEvent::InvokedAdd,
            JettyCommand::Bootstrap {
                no_fetch,
                overwrite,
            } => UsageEvent::InvokedBootstrap {
                no_fetch,
                overwrite,
            },
            JettyCommand::Diff { fetch } => UsageEvent::InvokedDiff { fetch },
            JettyCommand::Plan { fetch } => UsageEvent::InvokedPlan { fetch },
            JettyCommand::Apply { no_fetch } => UsageEvent::InvokedApply { no_fetch },
            JettyCommand::Subgraph { depth, .. } => UsageEvent::InvokedSubgraph { depth },
            JettyCommand::Remove { node_type, .. } => UsageEvent::InvokedRemove { node_type },
            JettyCommand::Rename { node_type, .. } => UsageEvent::InvokedRename { node_type },
        };
        record_usage(event, &jetty_config)
            .await
            .unwrap_or_else(|_| debug!("Failed to publish usage."));
        args
    };
    // Adjust logging levels based on args
    logging::update_filter_level(reload_handle, args.log_level);

    // ...and we're off!
    match &args.command {
        JettyCommand::Init {
            from,
            project_name,
            overwrite,
        } => {
            init::init(from, *overwrite, project_name).await?;
        }

        JettyCommand::Fetch {
            visualize,
            connectors,
        } => {
            fetch(connectors, visualize).await?;
        }

        JettyCommand::Explore {
            fetch: fetch_first,
            bind,
        } => {
            if *fetch_first {
                info!("Fetching all data first.");
                fetch(&None, &false).await?;
            }

            let jetty = new_jetty_with_connectors().await?;

            jetty.try_access_graph()?;
            jetty_explore::explore_web_ui(Arc::from(jetty.access_graph.unwrap()), bind).await;
        }
        JettyCommand::Add => {
            init::add().await?;
        }
        JettyCommand::Bootstrap {
            no_fetch,
            overwrite,
        } => {
            if !*no_fetch {
                info!("Fetching data before bootstrap");
                fetch(&None, &false).await?;
            };
            bootstrap(*overwrite).await?;
        }
        JettyCommand::Diff { fetch: fetch_first } => {
            if *fetch_first {
                info!("Fetching data before diff");
                fetch(&None, &false).await?;
            } else {
                println!("Generating diff based off existing data. Run `jetty diff -f` to fetch before generating the diff.")
            };
            diff::diff().await?;
        }
        JettyCommand::Plan { fetch: fetch_first } => {
            if *fetch_first {
                info!("Fetching data before plan");
                fetch(&None, &false).await?;
            } else {
                println!("Generating plan based off existing data. Run `jetty plan -f` to fetch before generating the plan.")
            };
            plan::plan().await?;
        }
        JettyCommand::Apply { no_fetch } => {
            if !*no_fetch {
                info!("Fetching data before apply. You can run `jetty apply -n` to run apply based on a previous fetch.");
                fetch(&None, &false).await?;
            };
            apply().await?;
        }
        JettyCommand::Subgraph { id, depth } => {
            let jetty = new_jetty_with_connectors().await?;

            let ag = jetty.try_access_graph()?;
            let parsed_uuid = uuid::Uuid::from_str(id)?;
            let binding = ag.extract_graph(
                ag.get_untyped_index_from_id(&parsed_uuid)
                    .ok_or_else(|| anyhow!("unable to find the right node"))?,
                *depth,
            );
            let generated_dot = binding.dot();
            println!("{generated_dot:?}");
        }
        JettyCommand::Remove { node_type, name } => remove::remove(node_type, name).await?,
        JettyCommand::Rename {
            node_type,
            old,
            new,
        } => rename::rename(node_type, old, new).await?,
    }

    Ok(())
}

async fn fetch(connectors: &Option<Vec<String>>, &visualize: &bool) -> Result<()> {
    let jetty = new_jetty_with_connectors().await?;

    let mut data_from_connectors = vec![];

    // Handle optionally fetching for only a few connectors
    let selected_connectors = if let Some(conns) = connectors {
        jetty
            .connectors
            .into_iter()
            .filter(|(name, _config)| conns.contains(&name.to_string()))
            .collect::<HashMap<_, _>>()
    } else {
        jetty.connectors
    };

    for (namespace, mut conn) in selected_connectors {
        let pb = basic_progress_bar(format!("Fetching {} data", namespace).as_str());

        let now = Instant::now();
        let data = conn.get_data().await;
        let pcd = (data, namespace.to_owned());

        data_from_connectors.push(pcd);

        pb.finish_with_message(format!(
            "Fetching {} data took {:.1} seconds",
            namespace,
            now.elapsed().as_secs_f32()
        ));
    }

    let pb = basic_progress_bar("Creating access graph");
    let now = Instant::now();

    // the last jetty was partially consumed by the fetch, so re-instantiating here
    let jetty = new_jetty_with_connectors().await?;

    let ag = AccessGraph::new_from_connector_data(data_from_connectors, &jetty)?;
    ag.serialize_graph(project::data_dir().join(project::graph_filename()))?;

    pb.finish_with_message(format!(
        "Access graph created in {:.1} seconds",
        now.elapsed().as_secs_f32()
    ));

    if visualize {
        info!("visualizing access graph");
        let now = Instant::now();
        ag.visualize("/tmp/graph.svg")
            .context("failed to visualize")?;
        info!(
            "Access graph visualized at \"/tmp/graph.svg\" (took {:.1} seconds)",
            now.elapsed().as_secs_f32()
        );
    } else {
        debug!("Skipping visualization.")
    };

    if let Err(e) = update_asset_files(&jetty) {
        warn!("failed to generate files for all assets: {}", e);
    };
    if let Err(e) = update_user_files(&jetty) {
        warn!("failed to generate files for all users: {}", e);
    };

    Ok(())
}

async fn bootstrap(overwrite: bool) -> Result<()> {
    let jetty = &new_jetty_with_connectors().await.map_err(|_| {
        anyhow!(
            "unable to find {} - make sure you are in a \
        Jetty project directory, or create a new project by running `jetty init`",
            project::jetty_cfg_path_local().display()
        )
    })?;

    // make sure there's an existing access graph
    jetty.try_access_graph()?;

    // Build all the yaml first
    let group_yaml = new_groups::get_env_config(jetty)?;
    let asset_yaml = jetty.generate_bootstrapped_policy_yaml()?;
    let user_yaml = jetty.generate_bootstrapped_user_yaml()?;

    // Now check for all the files
    if !overwrite {
        if groups_cfg_path_local().exists() {
            bail!("{} already exists; run `jetty bootstrap --overwrite` to overwrite the existing configuration", groups_cfg_path_local().to_string_lossy())
        }
        if project::assets_cfg_root_path_local().exists() {
            bail!("{} already exists; run `jetty bootstrap --overwrite` to overwrite the existing configuration", project::assets_cfg_root_path_local().to_string_lossy())
        }
        if project::users_cfg_root_path_local().exists() {
            bail!("{} already exists; run `jetty bootstrap --overwrite` to overwrite the existing configuration", project::users_cfg_root_path_local().to_string_lossy())
        }
    } else {
        if fs::remove_file(groups_cfg_path_local()).is_ok() {
            println!("removed existing groups file")
        };
        if fs::remove_dir_all(project::assets_cfg_root_path_local()).is_ok() {
            println!("removed existing asset directory")
        };
        if fs::remove_dir_all(project::users_cfg_root_path_local()).is_ok() {
            println!("removed existing users directory")
        };
    }

    // Now write the yaml files

    // groups
    if let Err(e) = new_groups::write_env_config(&group_yaml) {
        warn!("failed to write groups file: {}", e);
    }

    // assets
    write_bootstrapped_asset_yaml(asset_yaml)?;
    if let Err(e) = update_asset_files(jetty) {
        warn!("failed to generate files for all assets: {}", e);
    };

    // users
    write_bootstrapped_user_yaml(user_yaml)?;
    if let Err(e) = update_user_files(jetty) {
        warn!("failed to generate files for all users: {}", e);
    }

    println!(
        "{}",
        "\nSuccessfully bootstrapped your environment! 🎉🎉".green()
    );

    Ok(())
}

async fn apply() -> Result<()> {
    let jetty = &mut new_jetty_with_connectors().await.map_err(|_| {
        anyhow!(
            "unable to find {} - make sure you are in a \
        Jetty project directory, or create a new project by running `jetty init`",
            project::jetty_cfg_path_local().display()
        )
    })?;

    let diffs = get_diffs(jetty)?;

    // make sure there's an existing access graph
    let ag = jetty.try_access_graph()?;
    let connector_specific_diffs = diffs.split_by_connector();

    let tr = ag.translator();

    let local_diffs = connector_specific_diffs
        .iter()
        .map(|(k, v)| (k.to_owned(), tr.translate_diffs_to_local(v, k)))
        .collect::<HashMap<_, _>>();

    let mut results: HashMap<_, _> = HashMap::new();

    let pb = basic_progress_bar("Applying changes");
    let now = Instant::now();
    for (conn, diff) in local_diffs {
        results.insert(
            conn.to_owned(),
            jetty.connectors[&conn].apply_changes(&diff).await?,
        );
    }
    pb.finish_with_message(format!(
        "Changes applied in {:.1} seconds",
        now.elapsed().as_secs_f32()
    ));

    match fetch(&None, &false).await {
        Ok(_) => {
            diff::diff().await?;
        }
        Err(_) => {
            error!("unable to perform fetch");
            println!("We recommend running `jetty diff -f` to see if you should run apply again to correct any side-effects of your changes");
        }
    };

    for (c, result) in results {
        println!("{c}:\n{result}\n");
    }

    Ok(())
}

async fn get_connectors(
    creds: &HashMap<String, CredentialsMap>,
    selected_connectors: &HashMap<ConnectorNamespace, jetty_core::jetty::ConnectorConfig>,
) -> Result<HashMap<ConnectorNamespace, Box<dyn Connector>>> {
    let mut connector_map: HashMap<ConnectorNamespace, Box<dyn Connector>> = HashMap::new();

    for (namespace, config) in selected_connectors {
        connector_map.insert(
            namespace.to_owned(),
            match config.connector_type.as_str() {
                "dbt" => {
                    jetty_dbt::DbtConnector::new(
                        &selected_connectors[namespace],
                        &creds
                            .get(namespace.to_string().as_str())
                            .ok_or_else(|| {
                                anyhow!(
                                    "unable to find a connector called {} in {}",
                                    namespace,
                                    project::connector_cfg_path().display()
                                )
                            })?
                            .to_owned(),
                        Some(ConnectorClient::Core),
                        Some(project::data_dir().join(namespace.to_string())),
                    )
                    .await?
                }
                "snowflake" => {
                    jetty_snowflake::SnowflakeConnector::new(
                        &selected_connectors[namespace],
                        &creds
                            .get(namespace.to_string().as_str())
                            .ok_or_else(|| {
                                anyhow!(
                                    "unable to find a connector called {} in {}",
                                    namespace,
                                    project::connector_cfg_path().display()
                                )
                            })?
                            .to_owned(),
                        Some(ConnectorClient::Core),
                        Some(project::data_dir().join(namespace.to_string())),
                    )
                    .await?
                }
                "tableau" => {
                    jetty_tableau::TableauConnector::new(
                        &selected_connectors[namespace],
                        &creds
                            .get(namespace.to_string().as_str())
                            .ok_or_else(|| {
                                anyhow!(
                                    "unable to find a connector called {} in {}",
                                    namespace,
                                    project::connector_cfg_path().display()
                                )
                            })?
                            .to_owned(),
                        Some(ConnectorClient::Core),
                        Some(project::data_dir().join(namespace.to_string())),
                    )
                    .await?
                }
                _ => panic!(
                    "unknown connector type: {}",
                    config.connector_type.to_owned()
                ),
            },
        );
    }
    Ok(connector_map)
}

/// Create a new Jetty struct with all the connectors. Uses default locations for everything
pub async fn new_jetty_with_connectors() -> Result<Jetty> {
    let config = JettyConfig::read_from_file(project::jetty_cfg_path_local()).context(format!(
        "unable to find Jetty Config file at {} - you can set this up by running `jetty init`",
        project::jetty_cfg_path_local().to_string_lossy()
    ))?;

    let creds = fetch_credentials(project::connector_cfg_path()).map_err(|_| {
        anyhow!(
            "unable to find {} - you can set this up by running `jetty init`",
            project::connector_cfg_path().display()
        )
    })?;

    let connectors = get_connectors(&creds, &config.connectors).await?;

    Jetty::new_with_config(config, project::data_dir(), connectors)
}

fn basic_progress_bar(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(time::Duration::from_millis(120));

    pb.set_style(
        ProgressStyle::with_template("{spinner:.green} {msg}")
            .unwrap()
            // For more spinners check out the cli-spinners project:
            // https://github.com/sindresorhus/cli-spinners/blob/master/spinners.json
            .tick_strings(&[
                "▹▹▹▹▹",
                "▸▹▹▹▹",
                "▹▸▹▹▹",
                "▹▹▸▹▹",
                "▹▹▹▸▹",
                "▹▹▹▹▸",
                "▪▪▪▪▪",
            ]),
    );
    pb.set_message(msg.to_owned());
    pb
}
