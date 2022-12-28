//! functionality to update the config, when necessary

use std::fs;

use anyhow::Result;

use crate::{
    write::{new_groups, UpdateConfig},
    Jetty,
};

use super::{
    bootstrap::write_user_config_file, get_config_paths, get_validated_file_config_map,
    parser::read_config_file,
};

fn update_user_name(jetty: &Jetty, old: &String, new: &str) -> Result<()> {
    let validated_group_config = &new_groups::parse_and_validate_groups(&jetty)?;
    let mut config = get_validated_file_config_map(jetty, validated_group_config)?;

    for (path, user) in config.iter_mut() {
        if user.update_user_name(old, new)? {
            write_user_config_file(path, user)?
        }
    }
    Ok(())
}
fn remove_user_name(jetty: &Jetty, name: &String) -> Result<()> {
    let validated_group_config = &new_groups::parse_and_validate_groups(&jetty)?;
    let config = get_validated_file_config_map(jetty, validated_group_config)?;

    for (path, user) in config {
        if &user.name == name {
            fs::remove_file(path)?;
            return Ok(());
        }
    }
    Ok(())
}
fn update_group_name(jetty: &Jetty, old: &String, new: &str) -> Result<()> {
    let validated_group_config = &new_groups::parse_and_validate_groups(&jetty)?;
    let mut config = get_validated_file_config_map(jetty, validated_group_config)?;

    for (path, user) in config.iter_mut() {
        if user.update_group_name(old, new)? {
            write_user_config_file(path, &user)?
        }
    }
    Ok(())
}
fn remove_group_name(jetty: &Jetty, name: &String) -> Result<()> {
    let validated_group_config = &new_groups::parse_and_validate_groups(&jetty)?;
    let mut config = get_validated_file_config_map(jetty, validated_group_config)?;

    for (path, user) in config.iter_mut() {
        if user.remove_group_name(name)? {
            write_user_config_file(path, user)?
        }
    }
    Ok(())
}
