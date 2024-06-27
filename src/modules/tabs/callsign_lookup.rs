//
// Contains code belonging to the callsign lookup tab
//

use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::{Context, Result};
use chrono::NaiveDate;
use geo::Coord;
use log::{debug, error};
use poll_promise::Promise;
use serde::{Deserialize, Serialize};
use egui::{widgets, Align, Id, Layout, Ui, Widget, WidgetText};
use thiserror::Error;
use crate::{modules::gui::{generate_random_id, Tab}, types, GuiConfig, RT};


/// The name of the program
const PROGRAM_NAME: &str = env!("CARGO_PKG_NAME");


/// The callsign lookup tab
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct CallsignLookupTab {
    id: Id,
    callsign: String,
    #[serde(skip)]
    callsign_info: Option<CallsignInformation>,
    #[serde(skip)]
    task: Option<Promise<Result<CallsignInformation>>>
}
impl CallsignLookupTab {
    async fn query_hamdb(callsign: String) -> Result<CallsignInformation> {
        let hamdb_url = format!("https://api.hamdb.org/{callsign}/json/{PROGRAM_NAME}");

        let response = reqwest::get(hamdb_url).await.map_err(Error::FailedRequest)?
        .json::<serde_json::Value>().await.map_err(Error::FailedRequest)?;

        let value = response.get("hamdb")
            .ok_or(Error::InvalidResponseBody)?
            .get("callsign")
            .ok_or(Error::InvalidResponseBody)?;

        // TODO: Use map_err instead of context
        let data = serde_json::from_value::<HamDBResponse>(value.clone()).context("Failed to query HamDB API")?;

        if data.callsign == "NOT_FOUND" {
            Err(Error::CallsignNotFound)?
        } else {
            Ok(data.to_callsign_information())
        }
    }

    async fn query_hamqth(callsign: String, session_id: String) -> Result<CallsignInformation> {
        let url = format!("https://hamqth.com/xml.php?id={session_id}&callsign={callsign}&prg={PROGRAM_NAME}");

        let response = reqwest::get(url).await.map_err(Error::FailedRequest)?
        .text().await.map_err(Error::FailedRequest)?;

        Ok(serde_xml_rs::from_str::<HamQTHResponseWrapper>(&response).context("Failed to query HamQTH API")?.inner.to_callsign_information())
    }

    /// Queries the HamDB/HamQTH API about the provided callsign
    fn lookup_callsign_promise(&self, config: &mut Config) -> Promise<Result<CallsignInformation>> {
        let callsign = self.callsign.to_string();
        let hamqth_id = match RT.block_on(config.get_hamqth_session_id()) {
            Ok(id) => Some(id.to_string()),
            Err(err) => None
        };

        let _eg = RT.enter();
        Promise::spawn_async(async move {

            // Try the query the HamDB API first
            let hamdb_error = match Self::query_hamdb(callsign.clone()).await {
                Ok(callsign_info) => return Ok(callsign_info),
                Err(e) => e
            };

            // If we have a HamQTH session ID, try querying the HamQTH API
            if let Some(hamqth_id) = hamqth_id {
                debug!("HamDB query failed, retrying with HamQTH:\n{hamdb_error:?}");
                // Query the HamQTH API with the session ID
                let callsign_info = Self::query_hamqth(callsign, hamqth_id).await?;

                // Return the callsign information
                return Ok(callsign_info);
            }

            // We couldn't find the callsign, so return an error
            Err(Error::CallsignNotFound)?

        })
    }
}
impl Tab for CallsignLookupTab {
    fn id(&self) -> Id {
        self.id
    }

    fn title(&mut self) -> WidgetText {
        "Callsign Lookup".into()
    }

    fn process_event(&mut self, config: &mut GuiConfig, event: &types::Event) {
        // If the event is a lookup callsign event, start a new lookup task
        if let types::Event::LookupCallsign(callsign) = event {
            // Only want to start a new lookup task if we don't already have one running
            if self.task.is_none() {
                self.task = Some(self.lookup_callsign_promise(&mut config.callsign_lookup_config));
            }
        }
    }

    fn ui(&mut self, config: &mut GuiConfig, ui: &mut Ui) {

        // Process any finished lookup task
        if let Some(info) = self.task.take_if(|t| t.ready().is_some()) {
            // Update the callsign info if the lookup was successful, otherwise print an error
            match info.block_and_take() {
                Ok(info) => {
                    self.callsign_info = Some(info);
                },
                Err(err) => error!("Failed to lookup callsign: {err}")
            }
        }

        // A callsign was searched
        if let Some(info) = &self.callsign_info {

            // Vertically center the callsign label
            ui.vertical_centered(|ui| {

                // Show the callsign label
                ui.strong(&info.callsign);

                // Add a horizontal separator
                ui.separator();

                // Show the labels for each value
                widgets::Label::new(format!("Name:   {}", info.name)).ui(ui);
                widgets::Label::new(format!("Grid:   {}", info.grid)).ui(ui);
                widgets::Label::new(format!("Country:   {}", info.country)).ui(ui);
                widgets::Label::new(format!("City/State:   {}", info.city_state)).ui(ui);
                widgets::Label::new(format!("Address:   {}", info.address)).ui(ui);
                widgets::Label::new(format!("License Class:   {}", info.class)).ui(ui);
                widgets::Label::new(format!("License Expires:   {}", info.expires)).ui(ui);

            });
        }
        // No callsign has been searched yet
        else {
            ui.vertical_centered(|ui| {
                ui.label("Search for a callsign");
            });
        }

        // The callsign textbox and search button
        ui.with_layout(Layout::right_to_left(Align::Max), |ui| {

            // Show a button to search for the callsign. The button is disabled if a lookup task is already running
            let response = ui.add_enabled(self.task.is_none(), widgets::Button::new("\u{1F50D}"));
            if response.clicked() {
                self.task = Some(self.lookup_callsign_promise(&mut config.callsign_lookup_config));
            }

            // Show a textedit box for the callsign
            widgets::TextEdit::singleline(&mut self.callsign)
            .hint_text("Callsign")
            .desired_width(f32::INFINITY)
            .ui(ui);

        });

    }
}
impl Default for CallsignLookupTab {
    fn default() -> Self {
        Self {
            id: generate_random_id(),
            callsign: Default::default(),
            callsign_info: Default::default(),
            task: Default::default()
        }
    }
}
impl std::fmt::Debug for CallsignLookupTab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallsignLookupTab")
        .field("id", &self.id)
        .field("callsign", &self.callsign)
        .field("callsign_info", &self.callsign_info)
        .finish()
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

        // Convert the latitude and longitude into a Coord type
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

            geo::coord! { x: lon, y: lat }
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

            geo::coord! { x: lon, y: lat }
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
    pub location: Coord,
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
pub enum Error {
    #[error("The request failed: {0}")]
    FailedRequest(reqwest::Error),
    #[error("The response body was invalid")]
    InvalidResponseBody,
    #[error("Couldn't find the callsign")]
    CallsignNotFound,
    #[error("Failed to renew HamQTH session ID, is your username and password correct?")]
    HamQTHAuthFailure
}

/// The callsign lookup module config
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// The username to use with the HamQTH API
    pub username: String,
    /// The password to use with the HamQTH API
    pub password: String,
    #[serde(skip)]
    /// The HamQTH session ID
    hamqth_session_id: (u64, String)
}
impl Config {
    pub async fn get_hamqth_session_id(&mut self) -> Result<&str> {

        // Ensure we have credentials
        if self.username.is_empty() || self.password.is_empty() {
            return Err(Error::HamQTHAuthFailure)?;
        }

        // Get the current epoch
        let epoch_now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

        // If the cached ID is older than 45 minutes, renew the session id
        if epoch_now - self.hamqth_session_id.0 > 2_700 {

            // Format the authentication URL
            let url = format!("https://hamqth.com/xml.php?u={}&p={}", self.username, self.password);

            // Query the HamQTH API a new session ID
            let response = reqwest::get(url).await.map_err(Error::FailedRequest)?
            .text().await.map_err(Error::FailedRequest)?;

            // Try to parse the response into a session ID
            let id = serde_xml_rs::from_str::<HamQTHAuthResponseWrapper>(&response)
                .map_err(|_err| Error::HamQTHAuthFailure)?.inner.session_id;

            // If the session ID is empty, return an error
            if id.is_empty() {
                return Err(Error::HamQTHAuthFailure)?;
            }

            // Update the session ID cache
            self.hamqth_session_id = (epoch_now, id);

        }

        Ok(&self.hamqth_session_id.1)
    }
}
