use std::collections::HashMap;

use anyhow::{bail, Result};
use colored::Colorize;
use inquire::{Confirm, Text};
use jetty_core::{
    connectors::NewConnector,
    jetty::{ConnectorConfig, ConnectorNamespace, CredentialsMap},
    Connector,
};
use jetty_snowflake::SnowflakeConnector;

use crate::{
    init::{
        inquiry::{
            autocomplete::FilepathCompleter,
            validation::{FilepathValidator, FilepathValidatorMode, PathType},
        },
        pki::KeyPair,
    },
    project::default_keypair_dir_path,
};

use super::validation::filled_validator;

pub(crate) async fn ask_snowflake_connector_setup(
    connector_namespace: ConnectorNamespace,
) -> Result<CredentialsMap> {
    let skip_message = &format!(
        "{}\n\nTo skip {} setup, enter {}. You can add connectors later by running {}.",
        "".yellow(),
        "dbt",
        "/skip".italic().yellow(),
        "jetty add".italic().yellow()
    );

    // Loop until a successful connection.
    loop {
        let snowflake_account_id = Text::new("Snowflake Account Identifier:")
            .with_validator(filled_validator)
            .with_placeholder("org-account_name")
            .with_help_message(&format!("This field can be the account locator (like 'cea29483' or 'cea29483.us-east-1') or org account name, dash-separated (like 'MRLDK-ESA98348') See https://tinyurl.com/snow-account-id for more.{skip_message}"))
            .prompt()?;
        if snowflake_account_id == "/skip" {
            bail!("skipped");
        }

        let admin_username = Text::new("Jetty admin username:")
            .with_default("jetty")
            .with_help_message(&format!("We will use this user to authenticate Jetty runs. To see all permissions across your account, the Jetty user needs the SECURITYADMIN role or equivalent.{skip_message}"))
            .prompt()?;
        if admin_username == "/skip" {
            bail!("skipped");
        }

        let warehouse = Text::new("Warehouse to query with:")
            .with_help_message(&format!("We will use this warehouse for any warehouse-required queries to manage permissions.{skip_message}"))
            .prompt()?;
        if warehouse == "/skip" {
            bail!("skipped");
        }

        let keypair_answer = Text::new("Input a path to a pkcs8 private key file (`.p8`) to use for authentication or leave blank to create a new keypair.")
            .with_validator(FilepathValidator::new(
                None,
                PathType::File,
                "File not found.".to_string(),
                FilepathValidatorMode::AllowedValues{allowed_values: vec!["/skip".to_owned(), "".to_owned()]},
            ))
            .with_autocomplete(FilepathCompleter::default())
            .with_help_message(&skip_message)
            .prompt()?;
        if keypair_answer == "/skip" {
            bail!("skipped");
        }
        let should_create_keypair = keypair_answer.is_empty();
        let keypair_filepath = if should_create_keypair {
            default_keypair_dir_path()
                .join(format!("{}.p8", connector_namespace))
                .to_string_lossy()
                .to_string()
        } else {
            keypair_answer
        };

        let keypair = if should_create_keypair {
            println!("Generating keypair...");
            let keypair = KeyPair::new()?;
            println!("Creating files...");
            keypair.save_to_files(keypair_filepath)?;
            println!("Keypair generated!");
            keypair
        } else {
            println!("Loading keypair...");
            KeyPair::from_path(keypair_filepath)?
        };

        println!("Authorize Jetty access to your account by running the following SQL statement in Snowflake.");
        println!(
            "\n{}\n",
            format!(
                "ALTER USER {} SET rsa_public_key='{}';",
                admin_username,
                keypair.public_inner()
            )
            .italic()
        );

        let mut confirmed = false;
        confirmed = Confirm::new(&format!("Enter 'y' once the ALTER USER is complete, or 'n' to skip Snowflake setup. You can add connectors later by running {}.", "jetty add".italic().yellow()))
        .prompt()?;
        if !confirmed {
            bail!("skipped");
        }

        let creds = HashMap::from([
            ("account".to_owned(), snowflake_account_id),
            ("user".to_owned(), admin_username),
            ("warehouse".to_owned(), warehouse),
            ("public_key_fp".to_owned(), keypair.fingerprint()),
            ("private_key".to_owned(), keypair.private_key()),
            ("role".to_owned(), "SECURITYADMIN".to_owned()),
        ]);
        let connector =
            SnowflakeConnector::new(&ConnectorConfig::default(), &creds, None, None).await?;
        if connector.check().await {
            println!("successful connection!");
            return Ok(creds);
        }
    }
}
