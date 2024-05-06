//
// The callsign lookup abstraction interface. This allows the GUI to perform callsign lookups in a non-blocking manner.
//

use std::{sync::Arc, time::{SystemTime, UNIX_EPOCH}};

use anyhow::{Context, Result};
use chrono::NaiveDate;
use log::{debug, error};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{runtime::Handle, sync::Mutex};
use geoutils::Location;

use super::types::{Event, SpawnedFuture};


const PROGRAM_NAME: &str = env!("CARGO_PKG_NAME");


/// A callsign-lookup abstraction for the GUI.
/// 
/// This performs callsign lookups with two different APIs:
/// 1. https://api.hamdb.org/ (default)
/// 2. https://hamqth.com/ (requires username and password to get a session token, but has better support for some callsigns)
/// 
/// If credentials for *hamqth* are provided, it will be used in favor of *hamdb*.
#[derive(Debug)]
pub struct CallsignLookup {
    /// A handle to the async runtime
    handle: Handle,
    /// Optional HamQTH credentials `(username, password)`
    credentials: Option<(String, String)>,
    /// Optional HamQTH session ID
    hamqth_id: Arc<Mutex<(u64, String)>>
}
impl CallsignLookup {
    /// Create a new CallsignLookup instance.
    /// 
    /// For some non-US callsigns, HamDB may not have information about the callsign, so we can use HamQTH instead,
    /// but its API requires a username and password, so that can optionally be provided as `(username, password)`.
    pub fn new(handle: Handle, credentials: Option<(String, String)>) -> Self {
        Self {
            handle,
            credentials,
            hamqth_id: Default::default()
        }
    }

    async fn refresh_hamqth_session_id(username: String, password: String) -> Result<String> {
        let url = format!("https://hamqth.com/xml.php?u={username}&p={password}");

        let response = reqwest::get(url).await.map_err(CallsignLookupError::FailedRequest)?
        .text().await.map_err(CallsignLookupError::FailedRequest)?;

        debug!("Raw reseponse: {response}");

        let id = serde_xml_rs::from_str::<HamQTHAuthResponseWrapper>(&response).map_err(|_err| CallsignLookupError::HamQTHAuthFailure)?.inner.session_id;
        if id.is_empty() {
            return Err(CallsignLookupError::HamQTHAuthFailure)?;
        }

        Ok(id)
    }

    /// Gets the hamqth session id if credentials were provided.
    /// 
    /// This will reuse the cached id for 45 minutes, and then it get a new id
    async fn get_hamqth_session_id(credentials: Option<(String, String)>, hamqth_id: Arc<Mutex<(u64, String)>>) -> Result<String> {
        
        // Ensure we have credentials
        let (username, password) = credentials.ok_or(CallsignLookupError::CallsignNotFound)?;

        // The session ID
        let id;

        // Get the current epoch
        let epoch_now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

        // Get the cached id and its creation date
        let (epoch_old, cached_id) = &mut *hamqth_id.lock().await;

        // The cached ID is older than 45 minutes, renew the session id
        if epoch_now - *epoch_old > 2700 {
            debug!("Cached ID has expired");
            id = Self::refresh_hamqth_session_id(username, password).await?;

            // Update the ID
            epoch_old.clone_from(&epoch_now);
            cached_id.clone_from(&id);
        }
        // The cached ID is still valid, so use that
        else {
            id = cached_id.to_string();
        }

        Ok(id)
    }

    async fn query_hamdb(callsign: String) -> Result<CallsignInformation> {
        let hamdb_url = format!("https://api.hamdb.org/{callsign}/json/{PROGRAM_NAME}");

        let response = reqwest::get(hamdb_url).await.map_err(CallsignLookupError::FailedRequest)?
        .json::<serde_json::Value>().await.map_err(CallsignLookupError::FailedRequest)?;

        let value = response.get("hamdb")
            .ok_or(CallsignLookupError::InvalidResponseBody)?
            .get("callsign")
            .ok_or(CallsignLookupError::InvalidResponseBody)?;

        let data = serde_json::from_value::<HamDBResponse>(value.clone()).context("Failed to query HamDB API")?;

        if data.callsign == "NOT_FOUND" {
            Err(CallsignLookupError::CallsignNotFound)?
        } else {
            Ok(data.to_callsign_information())
        }
    }

    async fn query_hamqth(callsign: String, session_id: String) -> Result<CallsignInformation> {
        let url = format!("https://hamqth.com/xml.php?id={session_id}&callsign={callsign}&prg={PROGRAM_NAME}");

        let response = reqwest::get(url).await.map_err(CallsignLookupError::FailedRequest)?
        .text().await.map_err(CallsignLookupError::FailedRequest)?;

        Ok(serde_xml_rs::from_str::<HamQTHResponseWrapper>(&response).context("Failed to query HamQTH API")?.inner.to_callsign_information())
    }

    pub fn lookup_callsign(&mut self, callsign: impl ToString) -> SpawnedFuture {
        let callsign = callsign.to_string();
        let credentials = self.credentials.clone();
        let hamqth_id = self.hamqth_id.clone();

        self.handle.spawn(async move {

            // Query the HamDB API first
            let hamdb_query = Self::query_hamdb(callsign.clone()).await;

            // If HamDB gave the response we wanted, return it, otherwise try again with HamQTH
            if let Ok(callsign_info) = hamdb_query {
                return Ok(Event::CallsignLookedUp(Box::new(callsign_info)));
            }

            debug!("HamDB query failed, retrying with HamQTH");

            // Get the session HamQTH ID and then query the API with that ID
            let session_id = Self::get_hamqth_session_id(credentials, hamqth_id).await?;
            Ok(Event::CallsignLookedUp(Box::new(Self::query_hamqth(callsign, session_id).await?)))

        })
    }
}

/// The HamDB API response
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HamDBResponse {
    #[serde(alias = "call")]
    callsign: String,
    class: String,
    expires: String,
    status: String,
    grid: String,
    lat: String,
    lon: String,
    #[serde(alias = "fname")]
    first_name: String,
    #[serde(alias = "mi")]
    middle_name: String,
    #[serde(alias = "name")]
    last_name: String,
    suffix: String,
    #[serde(alias = "addr1")]
    address1: String,
    #[serde(alias = "addr2")]
    address2: String,
    state: String,
    zip: String,
    country: String
}
impl ToCallsignInformation for HamDBResponse {
    fn to_callsign_information(mut self) -> CallsignInformation {

        // Format the name into a pretty string `FIRST MIDDLE LAST`
        let name = {
            let name = format!("{} {} {}", self.first_name, self.middle_name, self.last_name);

            let words: Vec<&str> = name.split_whitespace().collect();

            words.join(" ")
        };

        // Make the grid square all uppercase
        self.grid.make_ascii_uppercase();

        // Convert the latitude and longitude into a Location type
        let location = {
            // Parse the latitude and longitude strings into f64 type
            let lat = self.lat.parse::<f64>().unwrap_or_else(|_err| {
                error!("Failed to parse latitude string into a f64 type (input: {})", self.lon);
                0.0
            });
            let lon = self.lon.parse::<f64>().unwrap_or_else(|_err| {
                error!("Failed to parse longitude string into a f64 type (input: {})", self.lon);
                0.0
            });

            Location::new(lat, lon)
        };

        // Format the address (resisting the urge to use breaking bad as an example address here :D)
        let address = {
            let words: Vec<&str> = self.address1.split_whitespace().collect();

            words.join(" ")
        };

        // Format the city and state
        let city_state = {
            let city_state = format!("{}, {}", self.address2, self.state);

            let words: Vec<&str> = city_state.split_whitespace().collect();

            words.join(" ")
        };

        // Format the operator class
        let class = match self.class.as_str() {
            "" => "Unknown",
            "N" => "Novice",
            "T" => "Technician",
            "G" => "General",
            "E" => "Extra",
            _ => &self.class
        }.to_string();

        // Format the license expiration date into YYYY-MM-DD (why must there be more than 1 date format in an API!)
        let expires = {

            let date_str: String;

            // Format the date into `YYYY-MM-DD`
            if let Ok(date) = NaiveDate::parse_from_str(&self.expires, "%m/%d/%Y") {
                date_str = date.format("%Y-%m-%d").to_string();
            }
            // The expiration date is empty, so say "Unknown"
            else if self.expires.is_empty() {
                date_str = "Unknown".to_string();
            }
            // Couldn't format the date, so we assume it's already in the right format
            else {
                date_str = self.expires;
            }

            date_str
        };

        CallsignInformation {
            callsign: self.callsign,
            name,
            grid: self.grid,
            location,
            country: self.country,
            address,
            city_state,
            class,
            expires
        }
    }
}


/// A wrapper for the HamQTH API response
#[derive(Debug, Serialize, Deserialize)]
struct HamQTHResponseWrapper {
    #[serde(alias = "search")]
    inner: HamQTHResponse
}

/// The HamQTH API response
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct HamQTHResponse {
    callsign: String,
    #[serde(alias = "nick")]
    nickname: String,
    qth: String,
    country: String,
    adif: String,
    itu: String,
    cq: String,
    grid: String,
    #[serde(alias = "adr_name")]
    address_name: String,
    #[serde(alias = "adr_street1")]
    address1: String,
    #[serde(alias = "adr_street2")]
    address2: String,
    #[serde(alias = "adr_street3")]
    address3: String,
    #[serde(alias = "adr_city")]
    address_city_state: String,
    #[serde(alias = "adr_zip")]
    address_zip: String,
    #[serde(alias = "adr_country")]
    address_country: String,
    #[serde(alias = "adr_adif")]
    address_adif: String,
    district: String,
    us_state: String,
    us_county: String,
    oblast: String,
    dok: String,
    iota: String,
    qsl_via: String,
    lotw: String,
    eqsl: String,
    qsl: String,
    qsldirect: String,
    email: String,
    jabber: String,
    icq: String,
    msn: String,
    skype: String,
    birth_year: String,
    #[serde(alias = "lic_year")]
    licensed_since_year: String,
    picture: String,
    #[serde(alias = "latitude")]
    lat: String,
    #[serde(alias = "longitude")]
    lon: String,
    continent: String,
    utc_offset: String,
    facebook: String,
    twitter: String,
    gplus: String,
    youtube: String,
    linkedin: String,
    flicker: String,
    vimeo: String
}
impl ToCallsignInformation for HamQTHResponse {
    fn to_callsign_information(mut self) -> CallsignInformation {

        // Convert the callsign to all uppercase
        self.callsign.make_ascii_uppercase();

        // Format the operator's name. This uses their name if available, or their nickname as a fallback value
        let name = {
            if !self.address_name.is_empty() {
                self.address_name
            } else {
                self.nickname
            }
        };

        // Make the grid square all uppercase
        self.grid.make_ascii_uppercase();

        // Convert the latitude and longitude into a Location type
        let location = {
            // Parse the latitude and longitude strings into f64 type
            let lat = self.lat.parse::<f64>().unwrap_or_else(|_err| {
                error!("Failed to parse latitude string into a f64 type (input: {})", self.lon);
                0.0
            });
            let lon = self.lon.parse::<f64>().unwrap_or_else(|_err| {
                error!("Failed to parse longitude string into a f64 type (input: {})", self.lon);
                0.0
            });

            Location::new(lat, lon)
        };

        // The operator's country, then street address country, and then the continent as a fallback value
        let country = {
            if !self.country.is_empty() {
                self.country
            } else if !self.address_country.is_empty() {
                self.address_country
            } else {
                self.continent
            }
        };

        // The operator's street address, using "Unavailable" as a fallback value
        let address = {
            if !self.address1.is_empty() {
                self.address1
            } else {
                "Unvailable".to_string()
            }
        };

        // Format the operator's city and state, if available
        let city_state = {

            let words: Vec<&str> = self.address_city_state.split_whitespace().collect();

            let mut city_state = words.join(" ");

            // Find all indexes where a comma exists
            let comma_indicies: Vec<usize> = city_state.char_indices().filter_map(|(c_idx, c)| {
                if c == ',' {
                    Some(c_idx)
                } else {
                    None
                }
            }).collect();

            // Remove all commas
            for idx in comma_indicies {
                city_state.remove(idx);
            }

            // Find the last space in the string (that separates the state from the city)
            let mut last_space_idx = None;
            for (c_idx, c) in city_state.char_indices() {
                if c == ' ' {
                    last_space_idx = Some(c_idx);
                }
            }

            // Insert a comma
            if let Some(idx) = last_space_idx {
                city_state.insert(idx, ',');
            }

            city_state
        };

        // HamQTH doesn't provided the license class or expiration date so we just use unknown here
        let class = "Unknown".to_string();
        let expires = "Unknown".to_string();

        CallsignInformation {
            callsign: self.callsign,
            name,
            grid: self.grid,
            location,
            country,
            address,
            city_state,
            class,
            expires
        }
    }
}


/// A wrapper for the HamQTH Auth API response
#[derive(Debug, Serialize, Deserialize)]
struct HamQTHAuthResponseWrapper {
    #[serde(alias = "session")]
    inner: HamQTHAuthResponse
}
/// The HamQTH Auth API response
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct HamQTHAuthResponse {
    session_id: String
}


/// Information about a callsign
#[derive(Debug, Clone)]
pub struct CallsignInformation {
    /// The callsign of the operator
    pub callsign: String,
    /// The name of the operator
    pub name: String,
    /// The grid square locator of the station
    pub grid: String,
    /// The location (latitude and longitude) of the station
    pub location: Location,
    /// The country of the operator
    pub country: String,
    /// The street address of the operator
    pub address: String,
    /// The city and state of the operator
    pub city_state: String,
    /// The license class of the operator
    pub class: String,
    /// The expiration date of the operator's license
    pub expires: String,
}

/// A trait to convert a HamQTH or HamDB response into the `CallsignInformation` type
trait ToCallsignInformation {
    /// Converts the response into the `CallsignInformation` type
    fn to_callsign_information(self) -> CallsignInformation;
}

/// Errors regarding the callsign lookup module
#[derive(Debug, Error)]
pub enum CallsignLookupError {
    #[error("Callsign Lookup: The request failed: {0}")]
    FailedRequest(reqwest::Error),
    #[error("Callsign Lookup: The response body was invalid")]
    InvalidResponseBody,
    #[error("Callsign Lookup: Couldn't find the callsign")]
    CallsignNotFound,
    #[error("Callsign Lookup: Failed to renew HamQTH session ID, is your username and password correct?")]
    HamQTHAuthFailure
}
