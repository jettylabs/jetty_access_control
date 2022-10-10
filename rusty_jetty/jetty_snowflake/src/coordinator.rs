use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;

use futures::future::join_all;
use futures::future::BoxFuture;
use futures::StreamExt;
use jetty_core::connectors;
use jetty_core::connectors::nodes;
use jetty_core::connectors::UserIdentifier;

use super::cual::{cual, get_cual_account_name, Cual};
use jetty_core::cual::Cualable;

use crate::entry_types;
use crate::Grant;
use crate::GrantType;

/// Number of metadata request to run currently (e.g. permissions).
/// 15 seems to give the best performance. In some circumstances, we may want to bump this up.
const CONCURRENT_METADATA_FETCHES: usize = 15;

/// Environment is a collection of objects pulled right out of Snowflake.
/// We process them to make jetty nodes and edges.
#[derive(Default, Debug)]
struct Environment {
    databases: Vec<entry_types::Database>,
    schemas: Vec<entry_types::Schema>,
    objects: Vec<entry_types::Object>,
    users: Vec<entry_types::User>,
    roles: Vec<entry_types::Role>,
    standard_grants: Vec<entry_types::StandardGrant>,
    future_grants: Vec<entry_types::FutureGrant>,
    role_grants: Vec<entry_types::GrantOf>,
}

// Now lets start filling up the environment

type RoleName = String;

pub(super) struct Coordinator<'a> {
    env: Environment,
    conn: &'a super::SnowflakeConnector,
    role_grants: HashMap<Grantee, HashSet<RoleName>>,
}

#[derive(Hash, Eq, PartialEq)]
enum Grantee {
    User(String),
    Role(String),
}

impl<'a> Coordinator<'a> {
    pub(super) fn new(conn: &'a super::SnowflakeConnector) -> Self {
        Self {
            env: Default::default(),
            role_grants: Default::default(),
            conn,
        }
    }

    pub(super) async fn get_data(&mut self) -> nodes::ConnectorData {
        // // Run in one group
        // Get all databases
        // Get all the schemas
        // Get all the users
        // Get all the roles

        let hold: Vec<BoxFuture<_>> = vec![
            Box::pin(self.conn.get_databases_future(&mut self.env.databases)),
            Box::pin(self.conn.get_schemas_future(&mut self.env.schemas)),
            Box::pin(self.conn.get_users_future(&mut self.env.users)),
            Box::pin(self.conn.get_roles_future(&mut self.env.roles)),
        ];

        let results = join_all(hold).await;
        for res in results {
            if let Err(e) = res {
                println!("{}", e)
            }
        }

        // try one object:
        let mut hold: Vec<BoxFuture<_>> = vec![];

        // for each schema, get objects
        let objects_mutex = Arc::new(Mutex::new(&mut self.env.objects));
        for schema in &self.env.schemas {
            let m = Arc::clone(&objects_mutex);
            hold.push(Box::pin(self.conn.get_objects_futures(schema, m)));
        }

        // for each role, get grants to that role
        let grants_to_role_mutex = Arc::new(Mutex::new(&mut self.env.standard_grants));
        for role in &self.env.roles {
            let m = Arc::clone(&grants_to_role_mutex);
            hold.push(Box::pin(self.conn.get_grants_to_role_future(role, m)));
        }

        // for each role, get grants of
        let target_arc = Arc::new(Mutex::new(&mut self.env.role_grants));
        for role in &self.env.roles {
            let m = Arc::clone(&target_arc);
            hold.push(Box::pin(self.conn.get_grants_of_role_future(role, m)));
        }

        // for each schema, get future grants
        let futre_grants_arc = Arc::new(Mutex::new(&mut self.env.future_grants));
        for schema in &self.env.schemas {
            let m = Arc::clone(&futre_grants_arc);
            hold.push(Box::pin(
                self.conn.get_future_grants_of_schema_future(schema, m),
            ));
        }

        // for database, get future grants, using the same Arc<Mutex>
        for database in &self.env.databases {
            let m = Arc::clone(&futre_grants_arc);
            hold.push(Box::pin(
                self.conn.get_future_grants_of_database_future(database, m),
            ));
        }

        let results = futures::stream::iter(hold)
            .buffer_unordered(CONCURRENT_METADATA_FETCHES)
            .collect::<Vec<_>>()
            .await;

        for res in results {
            if let Err(e) = res {
                println!("{}", e)
            }
        }

        self.role_grants = self.build_role_grants();

        nodes::ConnectorData {
            // 19 Sec
            groups: self.get_jetty_groups(),
            // 7 Sec
            users: self.get_jetty_users(),
            // 3.5 Sec
            assets: self.get_jetty_assets(),
            tags: self.get_jetty_tags(),
            policies: self.get_jetty_policies(),
            effective_permissions: HashMap::new(),
        }
    }

    /// Get the role grands into a nicer format
    fn build_role_grants(&self) -> HashMap<Grantee, HashSet<RoleName>> {
        let mut res: HashMap<Grantee, HashSet<RoleName>> = HashMap::new();
        for grant in &self.env.role_grants {
            let key = match &grant.granted_to[..] {
                "ROLE" => Grantee::Role(grant.grantee_name.to_owned()),
                "USER" => Grantee::User(grant.grantee_name.to_owned()),
                other => {
                    println!("skipping unexpected role type: {}", other);
                    continue;
                }
            };

            if let Some(v) = res.get_mut(&key) {
                v.insert(grant.role.to_owned());
            } else {
                res.insert(key, HashSet::from([grant.role.to_owned()]));
            }
        }
        res
    }

    /// Get standard grants grants by roles
    /// Snowflake doesn't allow permissions to be granted to users
    fn get_standard_grants_by_role(&self) -> HashMap<String, Vec<GrantType>> {
        let mut res: HashMap<String, Vec<GrantType>> = HashMap::new();
        for grant in &self.env.standard_grants {
            if let Some(v) = res.get_mut(grant.role_name()) {
                v.push(GrantType::Standard(grant.to_owned()));
            } else {
                res.insert(
                    grant.role_name().to_owned(),
                    vec![GrantType::Standard(grant.to_owned())],
                );
            }
        }
        res
    }

    /// Get future grants grants by roles
    /// Snowflake doesn't allow permissions to be granted to users
    fn get_future_grants_by_role(&self) -> HashMap<String, Vec<GrantType>> {
        let mut res: HashMap<String, Vec<GrantType>> = HashMap::new();
        for grant in &self.env.future_grants {
            if let Some(v) = res.get_mut(grant.role_name()) {
                v.push(GrantType::Future(grant.to_owned()));
            } else {
                res.insert(
                    grant.role_name().to_owned(),
                    vec![GrantType::Future(grant.to_owned())],
                );
            }
        }
        res
    }

    /// Helper fn to get role grants for a grantee
    fn get_role_grants(&self, grantee: &Grantee) -> HashSet<RoleName> {
        if let Some(g) = self.role_grants.get(grantee) {
            g.to_owned()
        } else {
            HashSet::new()
        }
    }

    /// Get groups from environment
    fn get_jetty_groups(&self) -> Vec<nodes::Group> {
        let mut res = vec![];
        for role in &self.env.roles {
            res.push(nodes::Group::new(
                role.name.to_owned(),
                HashMap::new(),
                self.get_role_grants(&Grantee::Role(role.name.to_owned())),
                HashSet::new(),
                HashSet::new(),
                HashSet::new(),
            ))
        }
        res
    }

    /// Get users from environment
    fn get_jetty_users(&self) -> Vec<nodes::User> {
        let mut res = vec![];
        for user in &self.env.users {
            res.push(nodes::User::new(
                user.name.to_owned(),
                HashSet::from([
                    UserIdentifier::Email(user.email.to_owned()),
                    UserIdentifier::FirstName(user.first_name.to_owned()),
                    UserIdentifier::LastName(user.last_name.to_owned()),
                ]),
                HashSet::from([user.display_name.to_owned(), user.login_name.to_owned()]),
                HashMap::new(),
                self.get_role_grants(&Grantee::User(user.name.to_owned())),
                HashSet::new(),
            ))
        }
        res
    }

    /// get assets from environment
    fn get_jetty_assets(&self) -> Vec<nodes::Asset> {
        let mut res = vec![];
        for object in &self.env.objects {
            let object_type = match &object.kind[..] {
                "TABLE" => connectors::AssetType::DBTable,
                "VIEW" => connectors::AssetType::DBView,
                _ => connectors::AssetType::Other,
            };

            res.push(nodes::Asset::new(
                object.cual(),
                "".to_owned(),
                object_type,
                HashMap::new(),
                // Policies applied are handled in get_jetty_policies
                HashSet::new(),
                HashSet::from([cual!(object.database_name, object.schema_name).uri()]),
                // Handled in child_of for parents.
                HashSet::new(),
                // We aren't extracting lineage from Snowflake right now.
                HashSet::new(),
                HashSet::new(),
                HashSet::new(),
            ));
        }

        for schema in &self.env.schemas {
            res.push(nodes::Asset::new(
                schema.cual(),
                format!("{}.{}", schema.database_name, schema.name),
                connectors::AssetType::DBSchema,
                HashMap::new(),
                // Policies applied are handled in get_jetty_policies
                HashSet::new(),
                HashSet::from([cual!(schema.database_name).uri()]),
                // Handled in child_of for parents.
                HashSet::new(),
                // We aren't extracting lineage from Snowflake right now.
                HashSet::new(),
                HashSet::new(),
                HashSet::new(),
            ));
        }

        for db in &self.env.databases {
            res.push(nodes::Asset::new(
                db.cual(),
                db.name.to_owned(),
                connectors::AssetType::DBDB,
                HashMap::new(),
                // Policies applied are handled in get_jetty_policies
                HashSet::new(),
                HashSet::new(),
                // Handled in child_of for parents.
                HashSet::new(),
                // We aren't extracting lineage from Snowflake right now.
                HashSet::new(),
                HashSet::new(),
                HashSet::new(),
            ));
        }

        res
    }

    /// get tags from environment
    /// NOT CURRENTLY IMPLEMENTED - This is an enterprise-only feature
    fn get_jetty_tags(&self) -> Vec<nodes::Tag> {
        vec![]
    }

    /// get policies from environment
    fn get_jetty_policies(&self) -> Vec<nodes::Policy> {
        let mut res = vec![];

        // For standard grants
        for (_role, grants) in self.get_standard_grants_by_role() {
            res.extend(self.conn.grants_to_policies(&grants))
        }

        // For future grants
        for (_role, grants) in self.get_future_grants_by_role() {
            res.extend(self.conn.grants_to_policies(&grants))
        }

        res
    }

    /// get effective_permissions from environment
    fn get_jetty_effective_permissions(&self) {}
}
