//
// The database abstraction interface. This is responsible for providing simple abstractions for very complicated database queries to the database.
//


use std::{env::current_exe, future::IntoFuture, time::Duration};
use lazy_static::lazy_static;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use surrealdb::{engine::any::Any, opt::auth::Root, sql::{self, statements}, Surreal};
use tokio::runtime::Handle;
use super::types::{self, Event, RecoverableError, FutureEvent};


const DB_FOLDER: &str = "db";
/// The namespace of the database
const DB_NAMESPACE: &str = "primary";
/// The name for the contacts database
const DB_CONTACTS: &str = "contacts";
/// The name for the table that contains all of the logged radio contacts
const TABLE_CONTACT: &str = "contact";

/// The default record limit to be returned from the database.
/// 10k is a very generous limit and I advise that you avoid reaching it in the first place.
const DEFAULT_RECORD_LIMIT: usize = 10_000;


/// The interface to the database. This should be created only once, and shared with every tab in the GUI.
/// 
/// This is responsible for abstracting the complex database queries away into simple functions that the GUI can utilize.
/// - NOTE: The functions are blocking, so complicated queries may freeze the GUI. This may change in the future.
#[derive(Debug)]
pub struct DatabaseInterface {
    handle: Handle,
    db: Surreal<Any>
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
    pub fn new(handle: Handle, endpoint: Option<String>, credentials: Option<(&str, &str)>) -> Result<Self, String> {

        // Format the endpoint as a string. This is either the user-provided endpoint, or the default embedded database
        let endpoint = endpoint.unwrap_or({
            // Get the parent directory of the application
            let exe_path = current_exe().expect("Failed to get path of exe file");
            let exe_dir = exe_path.parent().expect("Failed to get parent directory of exe file");

            // Return the default endpoint
            format!("rocksdb://{}/{DB_FOLDER}", exe_dir.display())
        });

        // Connect to the database
        let db = handle.block_on(Self::connect_to_db(endpoint, credentials))?;

        Ok(Self {
            handle,
            db
        })
    }

    /// Tries to connect to the database at `endpoint`, optionally using the provided `credentials`
    /// 
    /// If this fails, the returned result contains a string that describes the issue
    async fn connect_to_db(endpoint: String, credentials: Option<(&str, &str)>) -> Result<Surreal<Any>, String> {
        
        debug!("Connecting to database ('{endpoint}')");

        // Connect to the database
        let db = surrealdb::engine::any::connect(&endpoint)
        .await
        .map_err(|err| {
            error!("Failed to connect to database ('{endpoint}'): {err}");
            format!("Failed to connect to database: {err}")
        })?;

        debug!("Switching namespace to '{DB_NAMESPACE}' and database to '{DB_CONTACTS}'");

        // Use the default namespace and contacts database
        db.use_ns(DB_NAMESPACE)
        .use_db(DB_CONTACTS)
        .await.map_err(|err| {
            error!("Failed to switch namespace to {DB_NAMESPACE} and database to {DB_CONTACTS}: {err}");
            format!("Failed to connect to database: {err}")
        })?;

        info!("Connected to database");

        // If credentials were provided, use them to log into the database
        if let Some((username, password)) = credentials {
            debug!("Authenticating with database");

            db.signin(Root {
                username,
                password
            })
            .await
            .map_err(|err| {
                error!("Failed to authenticate with database: {err}");
                format!("Failed to authenticate with database\n{err}")
            })?;

            info!("Authenticated with database");
        };

        Ok(db)

    }

    /// Switches to the `contacts` database
    fn switch_contacts(&self) {
        self.handle.block_on(async {
            self.db.use_db(DB_CONTACTS).await.expect("Failed to switch to the contacts database");
            debug!("Database context set to {DB_CONTACTS}");
        })
    }

    /// Inserts a contact into the contacts table
    /// 
    /// If the insert was successful, this function returns the contact that was just inserted.
    pub fn insert_contact(&self, contact: types::Contact) -> FutureEvent {
        let db = self.db.clone();
        self.handle.spawn(async move {
                
            // Create the create statement (create inception!)
            let stmt = statements::CreateStatement {
                what: sql::Values(vec![sql::Table(TABLE_CONTACT.into()).into()]),
                data: Some(sql::Data::ContentExpression(sql::to_value(&contact).unwrap())),
                ..Default::default()
            };

            // Execute the database query with a 1 second timeout
            let response: Option<types::Contact> = execute_query_single(db.query(stmt), Duration::from_secs(1)).await?;

            // Return the same contact that we sent to the db
            if let Some(contact) = response {
                info!("Contact with '{}' has been added to the database", contact.callsign);
                Ok(Event::AddedContact(contact.into()))
            } else {
                error!("Failed to add contact with '{}' to the database (the response was empty)", contact.callsign);
                Err(RecoverableError::DatabaseError("Failed to add contact to database
                The query was successful but the response was empty for unknown reasons".into()))
            }
        })
    }
    
    /// Updates a contact in the contacts table using the ID in the provided contact
    /// 
    /// If the update was successful, this function returns the contact after it was updated
    pub fn update_contact(&self, contact: types::Contact) -> FutureEvent {
        let db = self.db.clone();
        self.handle.spawn(async move {

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
            
            // Process the query response
            if let Some(contact) = response {
                info!("Contact with '{}' has been updated", contact.callsign);
                Ok(Event::UpdatedContact(contact.into()))
            } else {
                error!("Failed to update contact with '{}' (the response was empty", contact.callsign);
                Err(RecoverableError::DatabaseError("Failed to update contact
                The query was successful but the response was empty for unknown reasons".into()))
            }
        })
    }

    /// Deletes a contact from the contacts table
    /// 
    /// If the removal was successful, this function returns the contact that was just removed.
    pub fn delete_contact(&self, id: sql::Id) -> FutureEvent {
        let db = self.db.clone();
        self.handle.spawn(async move {

            // Create the delete statement
            let stmt = statements::DeleteStatement {
                what: sql::Values(vec![sql::Thing { tb: TABLE_CONTACT.into(), id: id.clone() }.into()]),
                only: true,
                output: Some(sql::Output::Before),
                ..Default::default()
            };

            // Execute the query
            let response: Option<types::Contact> = execute_query_single(db.query(stmt), Duration::from_secs(1)).await?;

            // Process the query response
            if let Some(contact) = response {
                info!("Contact with '{}' has been removed from the database", contact.callsign);
                Ok(Event::DeletedContact(contact.into()))
            } else {
                error!("Failed to remove contact:{id} from the database because it doesn't exist");
                Err(RecoverableError::DatabaseError("Failed to remove contact from the database because it doesn't exist".into()))
            }
        })
    }

    /// Deletes multiple contacts from the contacts table
    /// 
    /// If the removal was successful, this function returns the contacts that were just removed.
    pub fn delete_contacts(&self, ids: Vec<sql::Id>) -> FutureEvent {
        let db = self.db.clone();
        self.handle.spawn(async move {

            // Parse the provided ids into table records
            let mut records: Vec<sql::Value> = Vec::new();
            for id in ids {
                records.push(sql::Thing { tb: TABLE_CONTACT.into(), id }.into());
            }

            // Create the delete statement
            let stmt = statements::DeleteStatement {
                what: sql::Values(records),
                output: Some(sql::Output::Before),
                ..Default::default()
            };

            // Execute the query
            let response = execute_query(db.query(stmt), Duration::from_secs(1)).await?;

            Ok(Event::DeletedContacts(response))
        })
    }

    /// Get contacts from the contacts table
    /// 
    /// 1. `start_at` is the row that the database should start its query at. In most cases, this should be 0.
    /// 2. `sort_col` can be used to order the rows based on a specific column.
    /// 3. `sort_dir` can be used to change which direction the column should be ordered in.
    pub fn get_contacts(&self, start_at: usize, sort_col: Option<ContactTableColumn>, sort_dir: Option<ColumnSortDirection>) -> FutureEvent {
        let db = self.db.clone();
        self.handle.spawn(async move {
            
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
                limit: Some(sql::Limit(DEFAULT_RECORD_LIMIT.into())),
                start: Some(sql::Start(start_at.into())),
                ..Default::default()
            };

            let response = execute_query(db.query(stmt), Duration::from_secs(1)).await?;

            Ok(Event::GotContacts(response))

        })
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
) -> Result<Vec<T>, RecoverableError>
where
    T: for<'a> Deserialize<'a>
{
    // Convert the query into a future
    let fut = fut.into_future();

    // Execute the query with the user-provided timeout
    if let Ok(response) = tokio::time::timeout(timeout, fut).await {

        match response {
            // The DB finished executing the query
            Ok(mut response) => {

                // Parse the database response
                match response.take::<Vec<T>>(0) {
                    // Successfully deserialized data into type `T`
                    Ok(t) => Ok(t),
                    // Database failed to execute the query or deserialization failed
                    Err(err) => {
                        error!("Database failed to execute query: {err}");
                        Err(RecoverableError::DatabaseError(format!("Database failed to execute query\n{err}")))
                    }
                }

            },
            // The DB failed to execute the query
            Err(err) => {
                error!("Database failed to execute query: {err}");
                Err(RecoverableError::DatabaseError(format!("Database failed to execute query\n{err}")))
            }
        }

    }
    // The timeout was reached (the database took too long to respond)
    else {
        error!("Timed out ({timeout:?}) while waiting for the database to respond");
        Err(RecoverableError::DatabaseError("Timed out while waiting for the database to respond".to_string()))
    }

}

/// Executes a single database query and handles the myriad of possible errors for you, with an added timeout.
/// 
/// Use this function if you're expecting a single object to be returned, otherwise see [execute_query_timeout]
/// 
/// - NOTE: This function only supports one database query at a time, so if you give it multiple, you won't get the other results.
async fn execute_query_single<T>(
    fut: impl IntoFuture<Output = surrealdb::Result<surrealdb::Response>>,
    timeout: Duration
) -> Result<Option<T>, RecoverableError>
where
    T: for<'a> Deserialize<'a>
{
    // Convert the query into a future
    let fut = fut.into_future();

    // Execute the query with the user-provided timeout
    if let Ok(response) = tokio::time::timeout(timeout, fut).await {

        match response {
            // The DB finished executing the query
            Ok(mut response) => {

                // Parse the database response
                match response.take::<Option<T>>(0) {
                    // Successfully deserialized data into Option<T>
                    Ok(t) => Ok(t),
                    // Database failed to execute the query or deserialization failed
                    Err(err) => {
                        error!("Database failed to execute query: {err}");
                        Err(RecoverableError::DatabaseError(format!("Database failed to execute query\n{err}")))
                    }
                }

            },
            // The DB failed to execute the query
            Err(err) => {
                error!("Database failed to execute query: {err}");
                Err(RecoverableError::DatabaseError(format!("Database failed to execute query\n{err}")))
            }
        }

    }
    // The timeout was reached (the database took too long to respond)
    else {
        error!("Timed out ({timeout:?}) while waiting for the database to respond");
        Err(RecoverableError::DatabaseError("Timed out while waiting for the database to respond".to_string()))
    }

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
    // TODO: This column is hidden during iteration (meaning it's not visible on the GUI table), but it should be, so format this!
    #[strum(disabled)]
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
