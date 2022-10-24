//! Jetty CLI
//!

#![deny(missing_docs)]

mod ascii;

use std::{path::Path, sync::Arc, time::Instant};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;

use jetty_core::{
    access_graph::{translate::Translator, AccessGraph},
    connectors::ConnectorClient,
    fetch_credentials,
    jetty::ConnectorNamespace,
    logging::{self, info, warn, LevelFilter},
    Connector, Jetty,
};

use ascii::print_banner;

const TAGS_PATH: &str = "tags.yaml";

/// Jetty CLI: Open-source data access control for modern teams
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(subcommand)]
    command: JettyCommand,
    #[clap(short, long)]
    log_level: Option<LevelFilter>,
}

#[derive(Subcommand, Debug)]
enum JettyCommand {
    /// Initialize a Jetty project.
    Init,
    Fetch {
        /// Visualize the graph in an SVG file.
        #[clap(short, long, value_parser, default_value = "false")]
        visualize: bool,
        /// Connectors to collect for.
        #[clap(short, long, use_value_delimiter=true, value_delimiter=',', default_values_t = vec!["snowflake".to_owned(),"tableau".to_owned(),"dbt".to_owned()])]
        connectors: Vec<String>,
    },
    Explore {
        #[clap(short, long, value_parser, default_value = "false")]
        fetch_first: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    logging::setup(args.log_level);

    match &args.command {
        JettyCommand::Init => {
            print_banner();

            println!("Welcome to Jetty! We are so glad you're here.")
        }

        JettyCommand::Fetch {
            visualize,
            connectors,
        } => {
            fetch(connectors, visualize).await?;
        }

        JettyCommand::Explore { fetch_first } => match AccessGraph::deserialize_graph() {
            Ok(mut ag) => {
                if *fetch_first {
                    info!("Fetching all data first.");
                    fetch(
                        &vec![
                            "snowflake".to_owned(),
                            "tableau".to_owned(),
                            "dbt".to_owned(),
                        ],
                        &false,
                    )
                    .await?;
                }
                if Path::new(TAGS_PATH).exists() {
                    let tag_config = std::fs::read_to_string(TAGS_PATH);
                    match tag_config {
                        Ok(c) => {
                            ag.add_tags(&c)?;
                        }
                        Err(e) => {
                            bail!("found, but was unable to read {}\nerror: {}", TAGS_PATH, e)
                        }
                    }
                }

                jetty_explore::explore_web_ui(Arc::new(ag)).await;
            }
            Err(e) => info!(
                "Unable to find saved graph. Try running `jetty fetch`\nError: {}",
                e
            ),
        },
    }

    Ok(())
}

async fn fetch(connectors: &Vec<String>, &visualize: &bool) -> Result<()> {
    let jetty = Jetty::new()?;
    let creds = fetch_credentials()?;

    if connectors.is_empty() {
        warn!("No connectors, huh?");
        bail!("Select a connector");
    }

    let mut data_from_connectors = vec![];

    if connectors.contains(&"dbt".to_owned()) {
        info!("initializing dbt");
        let now = Instant::now();
        // Initialize connectors
        let mut dbt = jetty_dbt::DbtConnector::new(
            &jetty.config.connectors[&ConnectorNamespace("dbt".to_owned())],
            &creds["dbt"],
            Some(ConnectorClient::Core),
        )
        .await?;
        info!("dbt took {} seconds", now.elapsed().as_secs_f32());

        info!("getting dbt data");
        let now = Instant::now();
        let dbt_data = dbt.get_data().await;
        let dbt_pcd = (dbt_data, ConnectorNamespace("dbt".to_owned()));
        info!("dbt data took {} seconds", now.elapsed().as_secs_f32());
        data_from_connectors.push(dbt_pcd);
    }

    if connectors.contains(&"snowflake".to_owned()) {
        info!("intializing snowflake");
        let now = Instant::now();
        let mut snow = jetty_snowflake::SnowflakeConnector::new(
            &jetty.config.connectors[&ConnectorNamespace("snow".to_owned())],
            &creds["snow"],
            Some(ConnectorClient::Core),
        )
        .await?;
        info!("snowflake took {} seconds", now.elapsed().as_secs_f32());

        info!("getting snowflake data");
        let now = Instant::now();
        let snow_data = snow.get_data().await;
        let snow_pcd = (snow_data, ConnectorNamespace("snowflake".to_owned()));
        info!(
            "snowflake data took {} seconds",
            now.elapsed().as_secs_f32()
        );
        data_from_connectors.push(snow_pcd);
    }

    if connectors.contains(&"tableau".to_owned()) {
        info!("initializing tableau");
        let now = Instant::now();
        let mut tab = jetty_tableau::TableauConnector::new(
            &jetty.config.connectors[&ConnectorNamespace("tableau".to_owned())],
            &creds["tableau"],
            Some(ConnectorClient::Core),
        )
        .await?;
        info!("tableau took {} seconds", now.elapsed().as_secs_f32());

        info!("getting tableau data");
        let now = Instant::now();
        tab.setup().await?;
        let tab_data = tab.get_data().await;
        let tab_pcd = (tab_data, ConnectorNamespace("tableau".to_owned()));
        info!("tableau data took {} seconds", now.elapsed().as_secs_f32());
        data_from_connectors.push(tab_pcd);
    }

    info!("creating access graph");
    let now = Instant::now();

    // Build the translator
    let tr = Translator::new(&data_from_connectors);
    // Process the connector data
    let pcd = tr.local_to_processed_connector_data(data_from_connectors);
    let ag = AccessGraph::new(pcd)?;
    info!(
        "access graph creation took {} seconds",
        now.elapsed().as_secs_f32()
    );
    ag.serialize_graph()?;

    if visualize {
        info!("visualizing access graph");
        let now = Instant::now();
        ag.visualize("/tmp/graph.svg")
            .context("failed to visualize")?;
        info!(
            "access graph creation took {} seconds",
            now.elapsed().as_secs_f32()
        );
    } else {
        info!("Skipping visualization.")
    };

    Ok(())
}
