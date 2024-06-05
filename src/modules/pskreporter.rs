//
// A PSKReporter abstraction interface
//


use std::{collections::HashMap, str::FromStr, time::Duration};

use crate::{GuiConfig, RT};

use super::{gui::{self, Tab}, maidenhead, map::{self, MapMarkerTrait}};
use anyhow::Result;
use egui::{emath::TSTransform, Id, Mesh, Rect, Widget};
use geo::{point, Coord, GeodesicArea, GeodesicBearing, GeodesicDistance, HaversineBearing, RhumbBearing};
use log::{debug, error, warn};
use poll_promise::Promise;
use rand::{Rng, RngCore, SeedableRng};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use thiserror::Error;
use tokio::{runtime::Handle, task::JoinHandle};


type CallsignString = arrayvec::ArrayString<20>;
type GridString = arrayvec::ArrayString<10>;
type ModeString = arrayvec::ArrayString<16>;


#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct PSKReporterTab {
    /// The ID of the tab
    id: Id,
    #[serde(skip)]
    map: Option<map::MapWidget<MapMarker>>,
    /// RNG used to generate random IDs for map markers
    #[serde(skip)]
    rng: rand::rngs::SmallRng,
    #[serde(skip)]
    /// The async task that queries the API and returns our map markers
    api_task: Option<Promise<Result<Vec<MapMarker>>>>,

    /// The callsign textbox
    callsign: String,
    /// Whether to filter for signals sent by the callsign, or received by the callsign
    sent_by: bool,
    /// The band filter
    band: Band,
    /// The mode filter
    mode: Mode,
    /// The last duration filter
    last: Last
}
impl Tab for PSKReporterTab {
    fn id(&self) -> egui::Id {
        self.id
    }

    fn title(&mut self) -> egui::WidgetText {
        "PSKReporter".into()
    }

    fn ui(&mut self, config: &mut crate::GuiConfig, ui: &mut egui::Ui) {

        // Get the map widget, initializing it if it doesn't exist
        let map = self.map.get_or_insert(map::MapWidget::new(ui.ctx()));

        // The pending task finished; process the result
        while self.api_task.as_ref().is_some_and(|p| p.poll().is_ready()) {
            // Take the result and replace it with a None value to indicate that the task is no longer pending
            let response = self.api_task.take().unwrap().block_and_take();

            // Parse the result, breaking out early if the result was an error
            let mut response = match response {
                Ok(r) => r,
                Err(err) => {
                    error!("Failed to query PSKReporter API: {err}");
                    break;
                }
            };

            // Get the map markers vec
            let markers = map.markers_mut();

            // Replace the old markers with the new ones
            *markers = response;

            // Update the map overlay now that the markers have been updated
            map.update_overlay();

        }

        // Render the widgets horizontally above the map
        ui.horizontal(|ui| {

            // Add a textbox to enter the callsign
            egui::widgets::TextEdit::singleline(&mut self.callsign)
            .hint_text("Callsign")
            .clip_text(true)
            .ui(ui);

            // Format the string for the sent_by combobox
            let sent_by_str = {
                if self.sent_by {
                    "Sent by"
                } else {
                    "Received by"
                }
            };

            // The sent_by/received_by combobox  
            egui::ComboBox::new("sent_by_combobox", "")
            .selected_text(sent_by_str)
            .show_ui(ui, |ui| {
                // The 'sent by' option was selected
                if ui.selectable_label(self.sent_by, "Sent by").clicked() {
                    self.sent_by = true;
                };
                // The 'received by' option was selected
                if ui.selectable_label(!self.sent_by, "Received by").clicked() {
                    self.sent_by = false;
                };
            });

            // The 'band' combobox
            egui::ComboBox::new("band_combobox", "Band")
            .selected_text(self.band.as_str())
            .show_ui(ui, |ui| { 
                // Iterate through the band options and render them as selectable labels
                for opt in Band::iter() {
                    let text = opt.as_str();
                    ui.selectable_value(&mut self.band, opt, text);
                }
            });

            // The 'mode' combobox
            egui::ComboBox::new("mode_combobox", "Mode")
            .selected_text(self.mode.as_str())
            .show_ui(ui, |ui| {
                // Iterate through the mode options and render them as selectable labels
                for opt in Mode::iter() {
                    let text = opt.as_str();
                    ui.selectable_value(&mut self.mode, opt, text);
                }
            });

            // The `last` combobox
            egui::ComboBox::new("last_combobox", "Last")
            .selected_text(self.last.as_str())
            .show_ui(ui, |ui| {
                // Iterate through the last duration options and render them as selectable labels
                for opt in Last::iter() {
                    let text = opt.as_str();
                    ui.selectable_value(&mut self.last, opt, text);
                }
            });

            // The search button to query the API. This is disabled if the API task is already running
            if ui.add_enabled(self.api_task.is_none(), egui::widgets::Button::new("Search")).clicked() {

                // Enter the tokio runtime
                let _eg = RT.enter();

                // We are filtering for signals sent by the callsign
                if self.sent_by {
                    // Spawn a task to query the API for signals sent by the callsign
                    self.api_task = Some(Promise::spawn_async(
                        ApiQueryBuilder::sent_by(self.callsign.clone(), self.band, self.mode, self.last.as_duration())
                    ));
                }
                // We are filtering for signals received by the callsign
                else {
                    // Spawn a task to query the API for signals received by the callsign
                    self.api_task = Some(Promise::spawn_async(
                        ApiQueryBuilder::received_by(self.callsign.clone(), self.band, self.mode, self.last.as_duration())  
                    ));
                }
                
            };

        });

        // Show the map widget
        map.ui(ui, config);

    }
}
impl Default for PSKReporterTab {
    fn default() -> Self {
        Self {
            id: gui::generate_random_id(),
            map: Default::default(),
            rng: rand::rngs::SmallRng::from_entropy(),
            api_task: Default::default(),
            callsign: Default::default(),
            sent_by: Default::default(),
            band: Band::All,
            mode: Mode::All,
            last: Last::Minutes15
        }
    }
}
impl std::fmt::Debug for PSKReporterTab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PSKReporterTab")
        .field("id", &self.id)
        .field("map", &self.map)
        .field("rng", &self.rng)
        .finish()
    }
}

/// A marker that's visible on the map
enum MapMarker {
    /// A transmitter on the pskreporter map
    Transmitter {
        /// The ID of the map marker
        id: u64,
        /// The location of the transmitter
        location: Coord<f64>,
        /// The grid locator of the transmitter
        grid: GridString,
        /// The callsign of the transmitter
        callsign: CallsignString,
        /// The mode of the transmitter
        mode: ModeString,
    },
    /// A receiver on the pskreporter map
    Receiver {
        /// The ID of the map marker
        id: u64,
        /// The location of the receiver
        location: Coord<f64>,
        /// The grid locator of the receiver
        grid: GridString,
        /// The callsign of the receiver
        callsign: CallsignString,
        /// The mode of the receiver
        mode: ModeString
    },
    /// A reception report regarding a transmitter on the pskreporter map
    ReceptionReportTransmitter {
        /// The ID of the map marker
        id: u64,
        /// The location of the transmitter
        location: Coord<f64>,
        /// The location of the receiver
        rx_location: Coord<f64>,
        /// The inner data about the reception report
        inner: ReceptionReport
    },
    /// A reception report regarding a receiver on the pskreporter map
    ReceptionReportReceiver {
        /// The ID of the map marker
        id: u64,
        /// The location of the receiver
        location: Coord<f64>,
        /// The location of the transmitter
        tx_location: Coord<f64>,
        /// The inner data about the reception report
        inner: ReceptionReport
    }
}
impl MapMarkerTrait for MapMarker {
    fn id(&self) -> u64 {
        *match self {
            MapMarker::Transmitter { id, .. } => id,
            MapMarker::Receiver { id, .. } => id,
            MapMarker::ReceptionReportTransmitter { id, .. } => id,
            MapMarker::ReceptionReportReceiver { id, .. } => id
        }
    }

    fn location(&self) -> &Coord<f64> {
        match self {
            MapMarker::Transmitter { location, .. } => location,
            MapMarker::Receiver { location, .. } => location,
            MapMarker::ReceptionReportTransmitter { location, .. } => location,
            MapMarker::ReceptionReportReceiver { location, .. } => location
        }
    }

    fn hovered_ui(&mut self, ui: &mut egui::Ui, config: &mut GuiConfig) {
        match self {
            MapMarker::Transmitter { grid, callsign, mode, .. } => {

                ui.heading("Transmitting Station");
                ui.label(format!("Callsign: {}", callsign));
                ui.label(format!("Grid: {}", grid));
                ui.label(format!("Mode: {}", mode));

            },
            MapMarker::Receiver { grid, callsign, mode, .. } => {

                ui.heading("Monitoring Station");
                ui.label(format!("Callsign: {}", callsign));
                ui.label(format!("Grid: {}", grid));
                ui.label(format!("Mode: {}", mode));

            },
            MapMarker::ReceptionReportTransmitter { location, rx_location, inner, .. } => {
                
                ui.heading("Reception Report");

                // The TX and RX station callsign and grid square
                ui.label(format!("TX Station: {}", inner.tx_callsign));
                ui.label(format!("TX Grid: {}", inner.tx_grid));
                ui.label(format!("RX Station: {}", inner.rx_callsign));
                ui.label(format!("RX Grid: {}", inner.rx_grid));

                // The frequency of the transmitting station, as heard by the receiver
                let freq = gui::frequency_formatter(inner.frequency as f64, 0..=0);
                ui.label(format!("Frequency: {freq}"));

                // The SNR of the transmitting station, as heard by the receiver
                ui.label(format!("SNR: {}dB", inner.snr));

                // The date and time of the report in UTC
                let time = chrono::DateTime::from_timestamp(inner.time as i64, 0).unwrap();
                ui.label(format!("Time (UTC): {}", time.format("%H:%M:%S")));
                ui.label(format!("Date (DMY): {}", time.format("%d/%m/%Y")));

                // The distance and bearing to the receiver
                // TODO: Add a measurement field to the config and support KM, not just miles
                let (mut bearing, mut distance) = point!(*rx_location).geodesic_bearing_distance(point!(*location));
                // Convert the distance to the preferred unit and convert the final bearing to an initial bearing
                distance = config.distance_unit.to_unit_from_meters(distance);
                bearing = (bearing + 360.0) % 360.0;

                ui.label(format!("Distance: {distance:.2} mi"));
                ui.label(format!("Bearing from RX to TX: {bearing:.0}\u{00B0}"));

            },
            MapMarker::ReceptionReportReceiver { location, tx_location, inner, .. } => {

                ui.heading("Reception Report");

                // The RX and TX station callsign and grid square
                ui.label(format!("RX Station: {}", inner.rx_callsign));
                ui.label(format!("RX Grid: {}", inner.rx_grid));
                ui.label(format!("TX Station: {}", inner.tx_callsign));
                ui.label(format!("TX Grid: {}", inner.tx_grid));

                // The frequency of the transmitting station, as heard by the receiver
                let freq = gui::frequency_formatter(inner.frequency as f64, 0..=0);
                ui.label(format!("Frequency: {freq}"));

                // The SNR of the transmitting station, as heard by the receiver
                ui.label(format!("SNR: {}dB", inner.snr));

                // The date and time of the report in UTC
                let time = chrono::DateTime::from_timestamp(inner.time as i64, 0).unwrap();
                ui.label(format!("Time (UTC): {}", time.format("%H:%M:%S")));
                ui.label(format!("Date (DMY): {}", time.format("%d/%m/%Y")));

                // The distance and bearing to the transmitter
                // TODO: Add a measurement field to the config and support KM, not just miles
                let (mut bearing, mut distance) = point!(*tx_location).geodesic_bearing_distance(point!(*location));
                // Convert the distance to the preferred unit and convert the final bearing to an initial bearing
                distance = config.distance_unit.to_unit_from_meters(distance);
                bearing = (bearing + 360.0) % 360.0;

                ui.label(format!("Distance: {distance:.2} mi"));
                ui.label(format!("Bearing from TX to RX: {bearing:.0}\u{00B0}"));

            }
        }
    }

    fn selected_ui(&mut self, ui: &mut egui::Ui, config: &mut GuiConfig) {
        self.hovered_ui(ui, config);
    }

    fn color(&self) -> image::Rgba<u8> {
        match self {
            MapMarker::Transmitter { .. } => image::Rgba([0, 0, 255, 255]),
            MapMarker::Receiver { .. } => image::Rgba([0, 0, 255, 255]),
            MapMarker::ReceptionReportTransmitter { .. } => image::Rgba([255, 0, 0, 255]),
            MapMarker::ReceptionReportReceiver { .. } => image::Rgba([255, 0, 0, 255]),
        }
    }

    fn draw_line_hovered(&self) -> Option<&Coord<f64>> {
        match self {
            MapMarker::Transmitter { .. } => None,
            MapMarker::Receiver { .. } => None,
            MapMarker::ReceptionReportTransmitter { rx_location, .. } => Some(rx_location),
            MapMarker::ReceptionReportReceiver { tx_location, .. } => Some(tx_location)
        }
    }
}


/// A simple API query builder for the PSKReporter API. This abstracts the details of the API and allows for simple querying of the API.
struct ApiQueryBuilder {
    query: HashMap<String, String>
}
impl ApiQueryBuilder {
    /// The PSKReporter API URL
    const URL: &'static str = "https://retrieve.pskreporter.info/query";

    /// Query the PSKReporter API for reception reports received by the given callsign on the specified band for the last `last` duration
    /// 
    /// # Arguments
    /// 
    /// * `callsign` - The callsign of the monitoring station
    /// * `band` - The band to query
    /// * `last` - Query over the last `last` duration
    async fn received_by(callsign: String, band: Band, mode: Mode, last: Duration) -> Result<Vec<MapMarker>> {

        // ===== CREATE AND EXECUTE QUERY ===== //

        // Create a hashmap of query parameters
        let mut query = HashMap::new();

        // Only query for the provided mode
        if let Some(mode_string) = mode.mode_string() {
            query.insert("mode".to_string(), mode_string.to_string());
        }

        // Only query for the last `last` duration
        let last_secs = -(last.as_secs() as i64);
        query.insert("flowStartSeconds".to_string(), last_secs.to_string());

        // Only query for the reception reports, not the active receivers
        query.insert("rronly".to_string(), "1".to_string());

        // Only query for the signals received by the provided callsign
        query.insert("receiverCallsign".to_string(), callsign.to_string());

        // Only query for the provided band
        if let Some((min_freq, max_freq)) = band.freq_range() {
            query.insert("frange".to_string(), format!("{}-{}", min_freq, max_freq));
        }
        
        // Create an instance of self
        let mut s = Self {
            query
        };

        // Execute the query
        let response = s.send().await?;

        // ===== PARSE API RESPONSE ===== //

        // Create an instance of rng
        let mut rng = rand::rngs::SmallRng::from_entropy();
        // Create the markers vec
        let mut markers = Vec::new();

        // Get the RX/monitor marker from the first reception report
        let rx_marker = if let Some(report) = response.reports.first() {
            // Convert the reception report into a receiver marker and return it
            MapMarker::Receiver {
                id: rng.next_u64(),
                location: maidenhead::grid_to_lat_lon(&report.rx_grid),
                grid: report.rx_grid,
                callsign: report.rx_callsign,
                mode: report.mode
            }
        }
        // There are no reception reports, so return an empty vec with no markers
        else {
            return Ok(markers);
        };

        // Iterate through the reception reports, convert them to map markers, and add them to the markers vec
        for report in response.reports {
            // Convert the reception report into a transmitter marker and push it into the markers vec
            markers.push(MapMarker::ReceptionReportTransmitter {
                id: rng.next_u64(),
                location: maidenhead::grid_to_lat_lon(&report.tx_grid),
                rx_location: *rx_marker.location(),
                inner: report
            });
        }

        // Add the receiver marker to the markers vec
        markers.push(rx_marker);

        // Return the markers vec
        Ok(markers)
    }

    /// Query the PSKReporter API for transmissions sent by the given callsign on the specified band for the last `last` duration
    /// 
    /// # Arguments
    /// 
    /// * `callsign` - The callsign of the transmitting station
    /// * `band` - The band to query
    /// * `last` - Query over the last `last` duration
    async fn sent_by(callsign: String, band: Band, mode: Mode, last: Duration) -> Result<Vec<MapMarker>> {

        // ===== CREATE AND EXECUTE QUERY ===== //
        // Create a hashmap of query parameters
        let mut query = HashMap::new();

        // Only query for the provided mode
        if let Some(mode_string) = mode.mode_string() {
            query.insert("mode".to_string(), mode_string.to_string());
        }

        // Only query for the last `last` duration
        let last_secs = -(last.as_secs() as i64);
        query.insert("flowStartSeconds".to_string(), last_secs.to_string());

        // Only query for the reception reports, not the active receivers
        query.insert("rronly".to_string(), "1".to_string());

        // Only query for the signals sent by the provided callsign
        query.insert("senderCallsign".to_string(), callsign.to_string());

        // Only query for the provided band
        if let Some((min_freq, max_freq)) = band.freq_range() {
            query.insert("frange".to_string(), format!("{}-{}", min_freq, max_freq));
        }

        // Create an instance of self
        let mut s = Self {
            query
        };

        // Execute the query
        let response = s.send().await?;

        // ===== PARSE API RESPONSE ===== //
        
        // Create an instance of rng
        let mut rng = rand::rngs::SmallRng::from_entropy();
        // Create the markers vec
        let mut markers = Vec::new();

        let tx_marker = if let Some(report) = response.reports.first() {
            // Convert the reception report into a transmitter marker and return it
            MapMarker::Transmitter {
                id: rng.next_u64(),
                location: maidenhead::grid_to_lat_lon(&report.tx_grid),
                grid: report.tx_grid,
                callsign: report.tx_callsign,
                mode: report.mode
            }
        } else {
            return Ok(markers);
        };

        for report in response.reports {
            markers.push(MapMarker::ReceptionReportReceiver {
                id: rng.next_u64(),
                location: maidenhead::grid_to_lat_lon(&report.rx_grid),
                tx_location: *tx_marker.location(),
                inner: report
            });
        }

        // Add the transmitter marker to the markers vec
        markers.push(tx_marker);

        // Return the markers vec
        Ok(markers)

    }

    /// For internal use only. Sends a query to the PSKReporter API and deserializes the response body into an ApiResponse type.
    async fn send(mut self) -> Result<ApiResponse> {

        // Insert the doNothing callback so we get a JSON response
        self.query.insert("callback".to_string(), "doNothing".to_string());

        // Convert the base Self::URL to a reqwest::Url
        let mut url = reqwest::Url::from_str(Self::URL)?;

        // Append the query parameters to the URL
        for (key, value) in &self.query {
            url.query_pairs_mut().append_pair(key, value);
        };

        // Execute the query
        let mut response = reqwest::get(url).await
        .map_err(Error::Request)?
        .text().await
        .map_err(Error::Request)?;

        // Trim whitespace from the response
        let trimmed_response = response.trim();

        // Deserialize the response body into an ApiResponse type
        let deserialized_response = serde_json::from_str::<ApiResponse>(&trimmed_response[10..trimmed_response.len()-2])
        .map_err(|e| {

            // If the response is a rate limit error, return that error
            if let Ok(response) = serde_json::from_str::<ApiResponseFailed>(trimmed_response) {
                if response.message == "Your IP has made too many queries too often. Please moderate your requests." {
                    return Error::RateLimited;
                }
            }

            // Otherwise, return the deserialization error
            Error::Deserialize(e)

        })?;

        Ok(deserialized_response)

    }

}

/// A band filter for the PSKReporter API
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, strum_macros::EnumIter)]
enum Band {
    /// All bands
    All,
    /// 2200M Band 135KHz
    B2200m,
    /// 630M Band 472KHz
    B630m,
    /// 160M Band 1.8MHz
    B160m,
    /// 80M Band 3.5MHz
    B80m,
    /// 60M Band 5.3MHz
    B60m,
    /// 40M Band 7MHz
    B40m,
    /// 30M Band 10.1MHz
    B30m,
    /// 20M Band 14MHz
    B20m,
    /// 17M Band 18MHz
    B17m,
    /// 15M Band 21MHz
    B15m,
    /// 12M Band 24MHz
    B12m,
    /// 10M Band 28MHz
    B10m,
    /// 6M Band 50MHz
    B6m,
    /// 2M Band 144MHz
    B2m,
    /// 1.25M Band 222MHz
    B1_25M,
    /// 70CM Band 420MHz
    B70CM,
    /// 33CM Band 902MHz
    B33CM,
    /// 23CM Band 1.24GHz
    B23CM,
    /// 2.4GHZ Band 2.4GHz
    F2_4GHZ,
    /// 3.4GHZ Band 3.4GHz
    F3_4GHZ,
    /// 5.8GHZ Band 5.8GHz
    F5_8GHZ,
    /// 10GHZ Band 10GHz
    F10GHZ,
    /// 24GHZ Band 24GHz
    F24GHZ,
    /// 47GHZ Band 47GHz
    F47GHZ,
    /// 76GHZ Band 76GHz
    F76GHZ
}
impl Band {
    /// Return the frequency range of the band, or None if the band is All
    fn freq_range(&self) -> Option<(u64, u64)> {
        match self {
            Band::All => None,
            Band::B2200m => Some((135_700, 137_800)),
            Band::B630m => Some((472_000, 479_000)),
            Band::B160m => Some((1_800_000, 2_000_000)),
            Band::B80m => Some((3_500_000, 4_000_000)),
            Band::B60m => Some((5_330_500, 5_407_800)),
            Band::B40m => Some((7_000_000, 7_300_000)), 
            Band::B30m => Some((10_100_000, 10_150_000)),
            Band::B20m => Some((14_000_000, 14_350_000)),
            Band::B17m => Some((18_068_000, 18_168_000)),
            Band::B15m => Some((21_000_000, 21_450_000)), 
            Band::B12m => Some((24_890_000, 24_990_000)), 
            Band::B10m => Some((28_000_000, 29_700_000)),
            Band::B6m => Some((50_000_000, 54_000_000)),
            Band::B2m => Some((144_000_000, 148_000_000)),
            Band::B1_25M => Some((219_000_000, 225_000_000)), 
            Band::B70CM => Some((420_000_000, 450_000_000)),
            Band::B33CM => Some((902_000_000, 928_000_000)), 
            Band::B23CM => Some((1_240_000_000, 1_300_000_000)),
            Band::F2_4GHZ => Some((2_300_000_000, 2_450_000_000)), 
            Band::F3_4GHZ => Some((3_300_000_000, 3_500_000_000)), 
            Band::F5_8GHZ => Some((5_650_000_000, 5_925_000_000)), 
            Band::F10GHZ => Some((10_000_000_000, 10_500_000_000)),
            Band::F24GHZ => Some((24_000_000_000, 24_250_000_000)), 
            Band::F47GHZ => Some((47_000_000_000, 47_200_000_000)), 
            Band::F76GHZ => Some((76_000_000_000, 81_000_000_000)), 
        }
    }

    /// Return the name of the band as a string
    fn as_str(&self) -> &'static str {
        match self {
            Band::All => "All",
            Band::B2200m => "2200M",
            Band::B630m => "630M",
            Band::B160m => "160M",
            Band::B80m => "80M",
            Band::B60m => "60M",
            Band::B40m => "40M",
            Band::B30m => "30M",
            Band::B20m => "20M",
            Band::B17m => "17M",
            Band::B15m => "15M",
            Band::B12m => "12M",
            Band::B10m => "10M",
            Band::B6m => "6M",
            Band::B2m => "2M",
            Band::B1_25M => "1.25M",
            Band::B70CM => "70CM",
            Band::B33CM => "33CM",
            Band::B23CM => "23CM",
            Band::F2_4GHZ => "2.4GHZ",
            Band::F3_4GHZ => "3.4GHZ",
            Band::F5_8GHZ => "5.8GHZ",
            Band::F10GHZ => "10GHZ",
            Band::F24GHZ => "24GHZ",
            Band::F47GHZ => "47GHZ",
            Band::F76GHZ => "76GHZ",
        }
    }
}

/// A mode filter for the PSKReporter API
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, strum_macros::EnumIter)]
enum Mode {
    /// All modes
    All,
    /// FT8 mode
    FT8,
    /// FT4 mode
    FT4,
    /// JS8 mode
    JS8,
    /// PSK31 mode
    PSK31,
    /// WSPR mode
    #[allow(clippy::upper_case_acronyms)]
    WSPR,
    /// CW mode
    CW
}
impl Mode {
    /// Return the mode string for the mode
    fn mode_string(&self) -> Option<&str> {
        match self {
            Mode::All => None,
            Mode::FT8 => Some("FT8"),
            Mode::FT4 => Some("FT4"),
            Mode::JS8 => Some("JS8"),
            Mode::PSK31 => Some("PSK31"),
            Mode::WSPR => Some("WSPR"),
            Mode::CW => Some("CW")
        }
    }

    /// Return the name of the mode as a string
    fn as_str(&self) -> &'static str {
        match self {
            Mode::All => "All",
            Mode::FT8 => "FT8",
            Mode::FT4 => "FT4",
            Mode::JS8 => "JS8",
            Mode::PSK31 => "PSK31",
            Mode::WSPR => "WSPR",
            Mode::CW => "CW"
        }
    }
}

/// A last-duration filter for the PSKReporter API
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, strum_macros::EnumIter)]
enum Last {
    Hours24,
    Hours12,
    Hours6,
    Hours3,
    Hours2,
    Hours1,
    Minutes30,
    Minutes15
}
impl Last {
    /// Return the duration of self
    fn as_duration(&self) -> Duration {
        match self {
            Last::Hours24 => Duration::from_secs(86_400),
            Last::Hours12 => Duration::from_secs(43_200),
            Last::Hours6 => Duration::from_secs(21_600),
            Last::Hours3 => Duration::from_secs(10_800),
            Last::Hours2 => Duration::from_secs(7_200),
            Last::Hours1 => Duration::from_secs(3_600),
            Last::Minutes30 => Duration::from_secs(1_800),
            Last::Minutes15 => Duration::from_secs(900)
        }
    }

    /// Return the name of the last duration as a string
    fn as_str(&self) -> &'static str {
        match self {
            Last::Hours24 => "24 Hours",
            Last::Hours12 => "12 Hours",
            Last::Hours6 => "6 Hours",
            Last::Hours3 => "3 Hours",
            Last::Hours2 => "2 Hours",
            Last::Hours1 => "1 Hour",
            Last::Minutes30 => "30 Minutes",
            Last::Minutes15 => "15 Minutes"
        }
    }
}

/// The error type for the PSKReporter module
#[derive(Debug, Error)]
enum Error {
    /// Failed to send a request to the API
    #[error("Failed to query API: {0}")]
    Request(reqwest::Error),
    /// Failed to deserialize API response body because it was invalid
    #[error("Failed to deserialize API response: {0}")]
    Deserialize(serde_json::Error),
    /// The API rate limit was exceeded
    #[error("API rate limit exceeded")]
    RateLimited
}


/// A successful response from the PSKReporter API
#[derive(Debug, Deserialize)]
struct ApiResponse {
    /// The current time in seconds since the epoch
    #[serde(alias = "currentSeconds")]
    current_epoch: u64,
    /// The array of reception reports returned by the API
    #[serde(alias = "receptionReport")]
    reports: Vec<ReceptionReport>,
}

/// A failed response from the PSKReporter API. This is used to safely handle the API rate limit error.
#[derive(Debug, Deserialize)]
struct ApiResponseFailed {
    /// The error message returned by the API
    message: String
}

/// A reception report from the PSKReporter API
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct ReceptionReport {
    /// The callsign of the receiving station
    #[serde(alias = "receiverCallsign")]
    rx_callsign: CallsignString,
    /// The grid square of the receiving station
    #[serde(alias = "receiverLocator")]
    rx_grid: GridString,
    /// The callsign of the transmitting station
    #[serde(alias = "senderCallsign")]
    tx_callsign: CallsignString,
    /// The grid square of the transmitting station
    #[serde(alias = "senderLocator")]
    tx_grid: GridString,
    /// The frequency that the station was heard on
    frequency: u64,
    /// The time the report was generated
    #[serde(alias = "flowStartSeconds")]
    time: u64,
    /// The mode that the transmitting station used
    mode: ModeString,
    /// The signal to noise ratio of the transmitting station
    #[serde(alias = "sNR")]
    snr: i8
}
