//
// The database abstraction interface. This is responsible for providing simple abstractions for very complicated database queries to the database.
//


use std::{env::current_exe, future::IntoFuture, sync::{atomic::{AtomicBool, Ordering::SeqCst}, Arc}, time::Duration};
use lazy_static::lazy_static;
use log::{debug, error, info};
use poll_promise::Promise;
use serde::{Deserialize, Serialize};
use surrealdb::{engine::any::Any, opt::{auth::Root, IntoQuery}, sql::{self, statements, Field, Thing, Value}, Surreal};
use tokio::runtime::Handle;
use crate::RT;
use super::types::{self, Event};
use thiserror::Error;
use anyhow::{Context, Result};


const DB_FOLDER: &str = "db";
/// The namespace of the database
const DB_NAMESPACE: &str = "primary";
/// The name for the contacts database
const DB_CONTACTS: &str = "contacts";
/// The name for the metadata table
const TABLE_METADATA: &str = "metadata";
/// The name for the table that contains all of the logged radio contacts
const TABLE_CONTACT: &str = "contact";

lazy_static! {
    /// The metadata for the contact table
    static ref METADATA_CONTACT: Thing = Thing { tb: TABLE_METADATA.into(), id: "contact".into() };

    /// The statement that increments the number of contacts in the metadata table
    static ref STATEMENT_INCREMENT_N_CONTACTS: sql::Statement = sql::Statement::Update(statements::UpdateStatement {
        what: sql::Values(vec![sql::Value::Thing(METADATA_CONTACT.clone())]),
        data: Some(sql::Data::SetExpression(vec![(
            sql::idiom("n_contacts").unwrap(),
            sql::Operator::Inc,
            sql::Value::Number(sql::Number::Int(1))
        )])),
        ..Default::default()
    });

    /// The statement that decrements the number of contacts in the metadata table
    static ref STATEMENT_DECREMENT_N_CONTACTS: sql::Statement = sql::Statement::Update(statements::UpdateStatement {
        what: sql::Values(vec![sql::Value::Thing(METADATA_CONTACT.clone())]),
        data: Some(sql::Data::SetExpression(vec![(
            sql::idiom("n_contacts").unwrap(),
            sql::Operator::Dec,
            sql::Value::Number(sql::Number::Int(1))
        )])),
        ..Default::default()
    });
}

/// The default record limit to be returned from the database.
/// 1k is a very generous limit and I advise that you avoid reaching it in the first place.
const DEFAULT_RECORD_LIMIT: usize = 1_000;


/// The interface to the database. This should be created only once, and shared with every tab in the GUI.
/// 
/// This is responsible for abstracting the complex database queries away into simple functions that the GUI can utilize.
/// - NOTE: The functions are blocking, so complicated queries may freeze the GUI. This may change in the future.
#[derive(Debug)]
pub struct DatabaseInterface {
    /// The database connection
    db: Surreal<Any>,
    /// The metadata for the contacts table
    contacts_metadata: ContactsTableMetadata,
    /// A flag to indicate if the contacts metadata has changed. This allows us to be immediate-safe and only query the database for metadata when it has changed.
    contacts_metadata_changed: Arc<AtomicBool>
}
impl DatabaseInterface {
    /// Connects to a database
    /// 
    /// Credentials can optionally be provided as `(username, password)`. This is used to log into remote authenticated databases.
    /// 
    /// The endpoint can be the url of a remote database.
    /// If no endpoint is provided, this uses the default embedded database.
    /// To connect to a specific endpoint, use the following format:
    /// 1. Secure websocket `wss://example.com`
    /// 2. Insecure websocket `ws://example.com`
    /// 3. Secure HTTP `https://example.com`
    /// 4. Insecure HTTP `http://example.com`
    /// 5. For a temporary in-memory database, use `mem://`
    /// 
    /// - Note: For remote endpoints, use `wss` (WebSockets) if possible, and please don't use the insecure variant of WebSockets or HTTP.
    pub fn new(endpoint: Option<String>, credentials: Option<(&str, &str)>) -> Result<Self> {

        // Format the endpoint as a string. This is either the user-provided endpoint, or the default embedded database
        let endpoint = endpoint.unwrap_or({
            // Get the parent directory of the application
            let exe_path = current_exe().expect("Failed to get path of exe file");
            let exe_dir = exe_path.parent().expect("Failed to get parent directory of exe file");

            // Return the default endpoint
            format!("rocksdb://{}/{DB_FOLDER}", exe_dir.display())
        });

        // Connect to the database
        let db = RT.block_on(Self::connect_to_db(endpoint, credentials))?;

        // Get the metadata for the contacts table
        let contacts_table_metadata = Self::init_contacts_table_metadata(&db)?;

        Ok(Self {
            db,
            contacts_metadata: contacts_table_metadata,
            contacts_metadata_changed: Arc::new(AtomicBool::new(false))
        })
    }

    /// Tries to connect to the database at `endpoint`, optionally using the provided `credentials`
    /// 
    /// If this fails, the returned result contains a string that describes the issue
    async fn connect_to_db(endpoint: String, credentials: Option<(&str, &str)>) -> Result<Surreal<Any>> {
        
        debug!("Connecting to database ('{endpoint}')");

        // Connect to the database
        let db = surrealdb::engine::any::connect(&endpoint).await
        .map_err(DatabaseError::ConnectionFailure)?;

        debug!("Switching namespace to '{DB_NAMESPACE}' and database to '{DB_CONTACTS}'");

        // Use the default namespace and database
        db.use_ns(DB_NAMESPACE).use_db(DB_CONTACTS).await
        .map_err(|error| DatabaseError::NamespaceChangeFailure { ns: DB_NAMESPACE.into(), db: DB_CONTACTS.into(), error })?;

        info!("Connected to database");

        // If credentials were provided, use them to log into the database
        if let Some((username, password)) = credentials {
            debug!("Authenticating with database");

            db.signin(Root {
                username,
                password
            })
            .await
            .map_err(DatabaseError::AuthenticationFailure)?;
        };

        Ok(db)

    }

    /// Switches to the `contacts` database
    fn switch_contacts(&self) {
        RT.block_on(async {
            self.db.use_db(DB_CONTACTS).await.expect("Failed to switch to the contacts database");
            debug!("Database context set to {DB_CONTACTS}");
        })
    }

    /// Returns the contact table metadata record if it already exists, otherwise returns an empty record.
    fn init_contacts_table_metadata(db: &Surreal<Any>) -> Result<ContactsTableMetadata> {
        RT.block_on(async move {

            // Select the contact metadata record
            let stmt = statements::SelectStatement {
                expr: sql::Fields(vec![Field::All], false),
                what: sql::Values(vec![Value::Thing(METADATA_CONTACT.clone())]),
                ..Default::default()
            };

            // Execute the query
            let response = db.query(stmt).await?.take::<Option<ContactsTableMetadata>>(0)?;

            // Return the metadata if it already exists, otherwise return an empty metadata record
            Ok(response.unwrap_or_default())

        })
    }

    /// Inserts a contact into the contacts table
    /// 
    /// If the insert was successful, this function returns the contact that was just inserted.
    pub fn insert_contact_promise(&self, contact: types::Contact) -> Promise<Result<types::Contact>> {
        let db = self.db.clone();
        let contacts_metadata_changed = self.contacts_metadata_changed.clone();
        let _eg = RT.enter();
        Promise::spawn_async(async move {

            // Create the query
            // This is a transaction that inserts the contact into the database, and then increments the number of contacts in the metadata table.
            // If anything fails, everything is rolled back.
            let query = sql::Query(sql::Statements(vec![
                sql::Statement::Begin(Default::default()),
                sql::Statement::Create(sql::statements::CreateStatement {
                    what: sql::Values(vec![sql::Table(TABLE_CONTACT.into()).into()]),
                    data: Some(sql::Data::ContentExpression(sql::to_value(&contact).unwrap())),
                    ..Default::default()
                }),
                STATEMENT_INCREMENT_N_CONTACTS.clone(),
                sql::Statement::Commit(Default::default())
            ]));

            // Execute the database query with a 1 second timeout
            let response: Option<types::Contact> = execute_query_single(db.query(query), Duration::from_secs(1)).await?;

            // Get the contact and ensure the database response wasn't empty
            let contact = response.ok_or(DatabaseError::EmptyResponse)?;
            
            // Mark the metadata as changed
            contacts_metadata_changed.store(true, SeqCst);

            // Return the contact that was just inserted
            Ok(contact)

        })
    }

    /// Updates a contact in the contacts table using the ID in the provided contact
    /// 
    /// If the update was successful, this function returns the contact after it was updated
    pub fn update_contact_promise(&self, contact: types::Contact) -> Promise<Result<types::Contact>> {
        let db = self.db.clone();
        let _eg = RT.enter();
        Promise::spawn_async(async move {

            let id = contact.id.as_ref().unwrap().id.clone();

            // Create the update statement
            let stmt = statements::UpdateStatement {
                only: true,
                what: sql::Values(vec![sql::Thing { tb: TABLE_CONTACT.into(), id }.into()]),
                data: Some(sql::Data::ContentExpression(sql::to_value(&contact).unwrap())),
                output: Some(sql::Output::After),
                ..Default::default()
            };

            // Execute the query
            let response: Option<types::Contact> = execute_query_single(db.query(stmt), Duration::from_secs(1)).await?;
            
            // Get the updated contact and ensure the database response wasn't empty
            let contact = response.ok_or(DatabaseError::EmptyResponse)?;

            // Return the updated contact
            Ok(contact)

        })
    }

    /// Deletes a contact from the contacts table
    /// 
    /// If the removal was successful, this function returns the contact that was just removed.
    pub fn delete_contact_promise(&self, id: sql::Id) -> Promise<Result<types::Contact>> {
        let db = self.db.clone();
        let contacts_metadata_changed = self.contacts_metadata_changed.clone();
        let _eg = RT.enter();
        Promise::spawn_async(async move {

            // Create the delete query
            // This is a transaction that deletes the contact from the database, and then decrements the number of contacts in the metadata table.
            // If anything fails, everything is rolled back.
            let query = sql::Query(sql::Statements(vec![
                sql::Statement::Begin(Default::default()),
                sql::Statement::Delete(statements::DeleteStatement {
                    what: sql::Values(vec![sql::Thing { tb: TABLE_CONTACT.into(), id: id.clone() }.into()]),
                    only: true,
                    output: Some(sql::Output::Before),
                    ..Default::default()
                }),
                STATEMENT_DECREMENT_N_CONTACTS.clone(),
                sql::Statement::Commit(Default::default()),
            ]));

            // Execute the query
            let response: Option<types::Contact> = execute_query_single(db.query(query), Duration::from_secs(1)).await?;

            // Get the deleted contact and ensure the database response wasn't empty
            let contact = response.ok_or(DatabaseError::DoesNotExist)?;

            // Mark the metadata as changed
            contacts_metadata_changed.store(true, SeqCst);

            // Return the deleted contact
            Ok(contact)

        })
    }

    /// Deletes multiple contacts from the contacts table
    /// 
    /// If the removal was successful, this function returns the contacts that were just removed.
    pub fn delete_contacts_promise(&self, ids: Vec<sql::Id>) -> Promise<Result<Vec<types::Contact>>> {
        let db = self.db.clone();
        let contacts_metadata_changed = self.contacts_metadata_changed.clone();
        let _eg = RT.enter();
        Promise::spawn_async(async move {

            // Parse the provided ids into table records
            let mut records: Vec<sql::Value> = Vec::new();
            for id in ids {
                records.push(sql::Thing { tb: TABLE_CONTACT.into(), id }.into());
            }
            let n_records = records.len() as i64;

            // Create the delete query
            // This is a transaction that deletes the contacts from the database, and then decrements the number of contacts in the metadata table.
            // If anything fails, everything is rolled back.
            let query = sql::Query(sql::Statements(vec![
                sql::Statement::Begin(Default::default()),
                sql::Statement::Delete(statements::DeleteStatement {
                    what: sql::Values(records),
                    output: Some(sql::Output::Before),
                    ..Default::default()
                }),
                sql::Statement::Update(statements::UpdateStatement {
                    what: sql::Values(vec![sql::Value::Thing(METADATA_CONTACT.clone())]),
                    data: Some(sql::Data::SetExpression(vec![(
                        sql::idiom("n_contacts").unwrap(),
                        sql::Operator::Dec,
                        sql::Value::Number(sql::Number::Int(n_records))
                    )])),
                    ..Default::default()
                }),
                sql::Statement::Commit(Default::default()),
            ]));

            // Execute the query
            let response = execute_query(db.query(query), Duration::from_secs(1)).await?;

            // Mark the metadata as changed
            contacts_metadata_changed.store(true, SeqCst);

            // Return the deleted contacts event
            Ok(response)

        })
    }

    /// Get contacts from the contacts table
    /// 
    /// 1. `start_at` is the row that the database should start its query at. In most cases, this should be 0.
    /// 2. `limit` is the maximum number of rows to return. If this is `None`, the default limit will be used.
    /// 3. `sort_col` can be used to order the rows based on a specific column.
    /// 4. `sort_dir` can be used to change which direction the column should be ordered in.
    pub fn get_contacts_promise(&self, start_at: usize, limit: Option<usize>, sort_col: Option<ContactTableColumn>, sort_dir: Option<ColumnSortDirection>) -> Promise<Result<Vec<types::Contact>>> {
        let db = self.db.clone();
        let _eg = RT.enter();
        Promise::spawn_async(async move {
            
            // Initialize the `ORDER BY` columns vec
            let mut orders = Vec::new();

            // The user specified a column to sort by
            if let Some(sort_col) = sort_col {

                // Create a sort order with the provided column and sort direction, if the user provided a sort direction
                orders.push(sql::Order {
                    order: sort_col.as_idiom(),
                    direction: match sort_dir {
                        Some(d) => d == ColumnSortDirection::Ascending,
                        None => false
                    },
                    ..Default::default()
                });

            }
            // The user didn't specify a column to sort by, so use default sorting scheme
            else {
                orders.push(CALLSIGN_SORT.clone());
                orders.push(DATE_SORT.clone());
                orders.push(TIME_SORT.clone());
            }

            // Create the sql statement
            // The sql statement should be something like; SELECT * FROM contact ORDER BY callsign, date, time LIMIT 10000 START 0
            let stmt = statements::SelectStatement {
                expr: sql::Fields(vec![sql::Field::All], false),
                what: sql::Values(vec![sql::Table(TABLE_CONTACT.into()).into()]),
                order: Some(sql::Orders(orders)),
                limit: Some(sql::Limit(limit.unwrap_or(DEFAULT_RECORD_LIMIT).into())),
                start: Some(sql::Start(start_at.into())),
                ..Default::default()
            };

            // Execute the query
            let response = execute_query(db.query(stmt), Duration::from_secs(1)).await?;

            // Return the got contacts event
            Ok(response)

        })
    }

    /// Returns the metadata about the contacts table
    pub fn get_contacts_metadata(&mut self) -> Result<&ContactsTableMetadata> {
        // If the metadata has changed, query the database for the new metadata
        if self.contacts_metadata_changed.load(SeqCst) {
            // Query the database for the new metadata
            self.contacts_metadata = Self::init_contacts_table_metadata(&self.db)?;
            // Reset the metadata changed flag
            self.contacts_metadata_changed.store(false, SeqCst);
        }

        // Return the metadata
        Ok(&self.contacts_metadata)
    }
}

// Initialize some lazy static column sort order constants.
// This mainly exists to reduce code reuse.
lazy_static! {
    /// The default callsign column sort order
    static ref CALLSIGN_SORT: surrealdb::sql::Order = surrealdb::sql::Order {
        order: vec!["callsign".into()].into(),
        direction: true,
        ..Default::default()
    };
    /// The default date column sort order
    static ref DATE_SORT: surrealdb::sql::Order = surrealdb::sql::Order {
        order: vec!["date".into()].into(),
        direction: true,
        ..Default::default()
    };
    /// The default time column sort order
    static ref TIME_SORT: surrealdb::sql::Order = surrealdb::sql::Order {
        order: vec!["time".into()].into(),
        direction: true,
        ..Default::default()
    };
}


/// Executes a single database query and handles the myriad of possible errors for you, with an added timeout.
/// 
/// Use this function if you're expecting multiple objects to be returned, otherwise see [execute_query_timeout_single]
/// 
/// - NOTE: This function only supports one database query at a time, so if you give it multiple, you won't get the other results.
async fn execute_query<T>(
    fut: impl IntoFuture<Output = surrealdb::Result<surrealdb::Response>>,
    timeout: Duration
) -> Result<Vec<T>>
where
    T: for<'a> Deserialize<'a>
{
    // Convert the query into a future
    let fut = fut.into_future();

    // Execute the query with the provided timeout 
    let mut response = tokio::time::timeout(timeout, fut).await
        .map_err(|_e| DatabaseError::Timeout)?
        .map_err(DatabaseError::QueryFailed)?
        .take::<Vec<T>>(0).map_err(DatabaseError::QueryFailed)?;

    // Return the db response
    Ok(response)

}

/// Executes a single database query and handles the myriad of possible errors for you, with an added timeout.
/// 
/// Use this function if you're expecting a single object to be returned, otherwise see [execute_query_timeout]
/// 
/// - NOTE: This function only supports one database query at a time, so if you give it multiple, you won't get the other results.
async fn execute_query_single<T>(
    fut: impl IntoFuture<Output = surrealdb::Result<surrealdb::Response>>,
    timeout: Duration
) -> Result<Option<T>>
where
    T: for<'a> Deserialize<'a>
{
    // Convert the query into a future
    let fut = fut.into_future();

    // Execute the query with the provided timeout 
    let mut response = tokio::time::timeout(timeout, fut).await
        .map_err(|_e| DatabaseError::Timeout)?
        .map_err(DatabaseError::QueryFailed)?
        .take::<Option<T>>(0).map_err(DatabaseError::QueryFailed)?;

    // Return the db response
    Ok(response)

}


/// The direction in which a table column should be sorted
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum ColumnSortDirection {
    #[default]
    Ascending,
    Descending
}

/// A column in the contact table
/// 
/// Calling `.to_string()` will return the name of the column that should be displayed in the GUI,
/// whereas calling `.as_idiom()` will return a surrealdb idiom that contains the name of the column in the database.
#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize, strum_macros::Display, strum_macros::EnumIter, strum_macros::EnumIs)]
pub enum ContactTableColumn {
    Callsign,
    Frequency,
    Mode,
    #[strum(to_string = "TX RST")]
    TxRst,
    #[strum(to_string = "RX RST")]
    RxRst,
    #[strum(to_string = "TX Power")]
    TxPwr,
    #[strum(to_string = "RX Power")]
    RxPwr,
    Date,
    Time,
    Duration,
    Note
}
impl ContactTableColumn {
    /// Converts `self` into the surrealdb idiom that it represents (e.g. `Self::RxPwr` = `rx_power`)
    fn as_idiom(&self) -> sql::Idiom {
        sql::idiom(match self {
            ContactTableColumn::Callsign => "callsign",
            ContactTableColumn::Frequency => "frequency",
            ContactTableColumn::Mode => "mode",
            ContactTableColumn::TxRst => "tx_rst",
            ContactTableColumn::RxRst => "rx_rst",
            ContactTableColumn::TxPwr => "tx_power",
            ContactTableColumn::RxPwr => "rx_power",
            ContactTableColumn::Date => "date",
            ContactTableColumn::Time => "time",
            ContactTableColumn::Duration => "duration",
            ContactTableColumn::Note => "note"
        }).unwrap()
    }

    pub fn is_sortable(&self) -> bool {
        match self {
            ContactTableColumn::Callsign => true,
            ContactTableColumn::Frequency => false,
            ContactTableColumn::Mode => false,
            ContactTableColumn::TxRst => false,
            ContactTableColumn::RxRst => false,
            ContactTableColumn::TxPwr => false,
            ContactTableColumn::RxPwr => false,
            ContactTableColumn::Date => true,
            ContactTableColumn::Time => true,
            ContactTableColumn::Duration => false,
            ContactTableColumn::Note => false,
        }
    }
}

/// Contains metadata about the contacts table
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ContactsTableMetadata {
    /// The number of records in the contacts table
    pub n_contacts: usize
}

/// Errors regarding the database module
#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("Failed to connect to the database: {0}")]
    ConnectionFailure(surrealdb::Error),
    #[error("Failed to set namespace to {ns} and db to {db}: {error}")]
    NamespaceChangeFailure {
        ns: String,
        db: String,
        error: surrealdb::Error
    },
    #[error("Failed to authenticate with database: {0}")]
    AuthenticationFailure(surrealdb::Error),
    #[error("The database didn't respond in time")]
    NoResponse,
    #[error("The database failed to execute the query: {0}")]
    QueryFailed(surrealdb::Error),
    #[error("The query was aborted because the database took too long to respond")]
    Timeout,
    #[error("The query executed successfully but the response was empty for unknown reasons")]
    EmptyResponse,
    #[error("Contact doesn't exist")]
    DoesNotExist
}

