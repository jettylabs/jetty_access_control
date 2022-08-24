//! Connectors module.
//!
use anyhow::Result;
use async_trait::async_trait;

use crate::jetty::{ConnectorConfig, CredentialsBlob};

/// The trait all connectors are expected to implement.
#[async_trait]
pub trait Connector {
    /// Instantiate a Connector from configuration.
    fn new(config: &ConnectorConfig, credentials: &CredentialsBlob) -> Result<Box<Self>>;
    /// Check if the Connector is properly set up and return the connection
    /// status (true for connected, false for not).
    async fn check(&self) -> bool;
}

/// Enum of identifiers used to resolve user identities
pub enum UserIdentifier {
    /// User's first name
    FirstName,
    /// User's last name
    LastName,
    /// User's full name
    FullName,
    /// User's email address
    Email,
    /// A platform specific identifier
    PlatformID,
}

/// Enum of known asset types

#[derive(Default)]
pub enum AssetType {
    /// Database Table
    DBTable,
    /// Database View
    DBView,
    /// Database Schema
    DBSchema,
    /// Database
    DBDB,
    /// Database Warehouse
    DBWarehouse,
    /// A catch-all that can be used by connector implementors
    #[default]
    Other,
}
