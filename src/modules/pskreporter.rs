//
// A PSKReporter abstraction interface
//


use std::collections::HashMap;

use crate::RT;

use super::{gui::{self, Tab}, maidenhead, map::{self, MapMarkerTrait}};
use anyhow::Result;
use egui::{emath::TSTransform, Id, Mesh, Rect, Widget};
use geo::{point, Coord, GeodesicBearing, GeodesicDistance};
use log::{debug, error, warn};
use poll_promise::Promise;
use rand::{Rng, RngCore, SeedableRng};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{runtime::Handle, task::JoinHandle};


type CallsignString = arrayvec::ArrayString<20>;
type GridString = arrayvec::ArrayString<10>;
type ModeString = arrayvec::ArrayString<16>;


#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct PSKReporterTab {
    id: Id,
    #[serde(skip)]
    map: Option<map::MapWidget<MapMarker>>,
    // map: Option<map::MapWidget<map::DummyMapMarker>>
    /// RNG used to generate random IDs for map markers
    #[serde(skip)]
    rng: rand::rngs::SmallRng,
    #[serde(skip)]
    /// The async task that queries the API and returns our map markers
    api_task: Option<Promise<Result<ApiResponse>>>
}
impl Tab for PSKReporterTab {
    fn id(&self) -> egui::Id {
        self.id
    }

    fn title(&mut self) -> egui::WidgetText {
        "PSKReporter".into()
    }

    fn ui(&mut self, config: &mut crate::GuiConfig, ui: &mut egui::Ui) {

        // Initialize and get the map widget
        let map = match &mut self.map {
            Some(m) => m,
            None => {
                let mut map_widget = map::MapWidget::new(ui.ctx());
                // let mut rng = rand::rngs::SmallRng::from_entropy();
                // let markers = map_widget.markers_mut();

                // // TODO: Remove this. This adds some dummy markers to the map for debug purposes
                // for _ in 0..25 {
                //     let m = map::DummyMapMarker {
                //         id: rng.next_u64(),
                //         location: geo::coord! { x: rng.gen_range(-180.0..180.0), y: rng.gen_range(-85.0..85.0) }
                //     };
                //     markers.push(m);
                // }
                // map_widget.update_overlay();

                self.map = Some(map_widget);
                self.map.as_mut().unwrap()
            }
        };

        // The pending task finished; process the result
        while self.api_task.as_ref().is_some_and(|p| p.poll().is_ready()) {
            // Take the result and replace it with a None value
            let response = self.api_task.take().unwrap().block_and_take();

            // Parse the result, breaking out early if the result was an error
            let mut response = match response {
                Ok(r) => r,
                Err(err) => {
                    error!("Failed to query PSKReporter API: {err}");
                    break;
                }
            };

            // Get and clear the existing map markers
            let markers = map.markers_mut();
            markers.clear();

            // The RX marker. This should be populated on the first iteration
            let mut rx_marker: Option<MapMarker> = None;
            let mut rx_not_found = false;

            // Iterate through the reception reports, convert them to map markers, and add them to the markers vec
            // while let Some(report) = response.reports.into_iter().next() {

            // }
            for report in response.reports {

                // Get the location of the receiver/monitoring station. This populates rx_marker on the first iteration
                // TODO: Move this outside of the function
                let rx_location = if rx_not_found {
                    // This should never happen but the PSKReporter API was reverse engineered and there's no guarantees that it'll give us the right monitor
                    geo::Coord::zero()
                } else {
                    // Return the location of the rx monitor if it's
                    match &rx_marker {
                        Some(r) => *r.location(),
                        None => {

                            // Found a matching receiver in the receivers vec; populate the rx marker option
                            if let Some(receiver) = response.receivers
                            .extract_if(|r| r.callsign == report.rx_callsign).next() {

                                // Convert the grid to coordinates
                                let location = maidenhead::grid_to_lat_lon(&report.rx_grid);

                                // Create a marker for the rx monitor and update the rx marker option
                                rx_marker = Some(MapMarker::Receiver {
                                    id: self.rng.next_u64(),
                                    location,
                                    inner: receiver
                                });

                                // Return the location
                                location
                            }
                            // The receiver was not found; update the rx_not_found variable and print a warning.
                            // This should never happen but the PSKReporter API was reverse engineered and there's no guarantees that we'll get what we expect.
                            else {
                                warn!("Failed to find monitoring station in receiver list");
                                rx_not_found = true;
                                geo::Coord::zero()
                            }

                        }
                    }
                };

                // Convert the reception report into a transmitter marker and push it into the markers vec
                markers.push(MapMarker::Transmitter {
                    id: self.rng.next_u64(),
                    location: maidenhead::grid_to_lat_lon(&report.tx_grid),
                    rx_location,
                    inner: report
                });

            }

            // Add the receiver maker to the markers vec
            if let Some(receiver) = rx_marker {
                markers.push(receiver);
            }

            // Update the map overlay now that we changed the markers
            map.update_overlay();

        }

        if ui.button("Test").clicked() {

            // If no task is currently running, spawn one
            if self.api_task.is_none() {
                let _eg = RT.enter();
                self.api_task = Some(Promise::spawn_async(test()));
            }

        };

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
            api_task: Default::default()
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
        /// The location of the receiver
        rx_location: Coord<f64>,
        /// The inner data about the reception report
        inner: ReceptionReport
    },
    /// A receiver with a reception report on the pskreporter map
    Receiver {
        /// The ID of the map marker
        id: u64,
        /// The location of the receiver
        location: Coord<f64>,
        /// The inner data about the receiver
        inner: ActiveReceiver
    }
}
impl MapMarkerTrait for MapMarker {
    fn id(&self) -> u64 {
        *match self {
            MapMarker::Transmitter { id, .. } => id,
            MapMarker::Receiver { id, .. } => id
        }
    }

    fn location(&self) -> &Coord<f64> {
        match self {
            MapMarker::Transmitter { location, .. } => location,
            MapMarker::Receiver { location, .. } => location
        }
    }

    fn hovered_ui(&mut self, ui: &mut egui::Ui) {
        match self {
            MapMarker::Transmitter { id, location, rx_location, inner } => {
                
                ui.heading("Reception Report");

                // The TX and RX station callsign and grid square
                ui.label(format!("TX Station: {}", inner.tx_callsign));
                ui.label(format!("TX Grid: {}", inner.tx_grid));
                ui.label(format!("RX Station: {}", inner.rx_callsign));
                ui.label(format!("RX Grid: {}", inner.rx_grid));

                // The frequency of the transmitting station
                let freq = gui::frequency_formatter(inner.frequency as f64, 0..=0);
                ui.label(format!("Frequency: {freq}"));

                // The SNR of the transmitting station, as heard by the receiver
                ui.label(format!("SNR: {}dB", inner.snr));

                // The date and time of the contact in UTC
                let time = chrono::DateTime::from_timestamp(inner.time as i64, 0).unwrap();
                ui.label(format!("Time (UTC): {}", time.format("%H:%M:%S")));
                ui.label(format!("Date (DMY): {}", time.format("%d/%m/%Y")));

                // The distance and bearing to the receiver
                // TODO: Add a measurement field to the config and support KM, not just miles
                let (mut bearing, mut distance) = point!(*location).geodesic_bearing_distance(point!(*rx_location));
                // Convert the distance to miles and add 180 degrees to the bearing so it's always positive
                distance *= 0.0006213712;
                bearing += 180.0;

                ui.label(format!("Distance: {distance:.2} mi"));
                ui.label(format!("Bearing to RX: {bearing:.0}\u{00B0}"));

            },
            MapMarker::Receiver { id, location, inner } => {

                // ui.label("Monitor");
                ui.heading("Monitoring Station");
                ui.label(format!("Callsign: {}", inner.callsign));
                ui.label(format!("Grid: {}", inner.grid));
                ui.label(format!("Mode: {}", inner.mode));

            }
        }
    }

    fn selected_ui(&mut self, ui: &mut egui::Ui) {
        match self {
            MapMarker::Transmitter { id, location, rx_location, inner } => {
                
                self.hovered_ui(ui);

            },
            MapMarker::Receiver { .. } => {
                // ui.label("Station Monitor");

                self.hovered_ui(ui);
            }
        }
    }

    fn color(&self) -> image::Rgba<u8> {
        match self {
            MapMarker::Transmitter { .. } => image::Rgba([255, 0, 0, 255]),
            MapMarker::Receiver { .. } => image::Rgba([0, 255, 0, 255])
        }
    }

    fn draw_line_hovered(&self) -> Option<&Coord<f64>> {
        match self {
            MapMarker::Transmitter { rx_location, .. } => Some(rx_location),
            MapMarker::Receiver { .. } => None
        }
    }
}


// const URL: &str = "https://www.pskreporter.info/cgi-bin/pskquery5.pl?encap=1&callback=doNothing&statistics=1&noactive=1&nolocator=1&flowStartSeconds=-43200&frange=6000000-8000000&mode=FT8&senderCallsign=KF0CZM&lastDuration=406";
// const URL: &str = "https://www.pskreporter.info/cgi-bin/pskquery5.pl?encap=1&callback=doNothing&statistics=1&noactive=1&nolocator=1&flowStartSeconds=-900&receiverCallsign=VE4REM&lastDuration=4216";
// const URL: &str = "https://www.pskreporter.info/cgi-bin/pskquery5.pl?encap=1&callback=doNothing&statistics=1&noactive=1&nolocator=1&flowStartSeconds=-3600&mode=FT8&receiverCallsign=VE4REM&lastseqno=47010219750&lastDuration=402";
// TODO: Get rid of the "encap=1" query to remove the XML wrapper (what does this do to rate limit messages?)
const RESPONSE: &str = include_str!("response.xml");


async fn test() -> Result<ApiResponse> {
    // Query PSKReporter
    // let mut response = reqwest::get(URL).await?
    // .text().await?;
    let _span = tracy_client::span!("Convert response to string");
    let mut response = RESPONSE.to_string();
    drop(_span);
    let _span = tracy_client::span!("Trim response string");
    let trimmed_response = response.trim();
    drop(_span);

    let _span = tracy_client::span!("Deserialize pskreporter api response");
    // Deserialize the API response body into an ApiResponse type
    let deserialized_response = serde_json::from_str::<ApiResponse>(&trimmed_response[10..trimmed_response.len()-2])
    .map_err(Error::Deserialize)?;
    drop(_span);
    // debug!("N Active Receivers: {}\nN Reception Reports: {}", deserialized_response.receivers.len(), deserialized_response.reports.len());
    // debug!("Deserialized reports:\n{:?}", deserialized_response.reports);

    Ok(deserialized_response)
}

#[derive(Debug, Error)]
enum Error {
    #[error("Test error")]
    Test,
    #[error("Failed to query API: {0}")]
    Request(reqwest::Error),
    /// Failed to deserialize API response body because it was invalid
    #[error("Failed to deserialize API response: {0}")]
    Deserialize(serde_json::Error)
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiResponse {
    #[serde(alias = "currentSeconds")]
    current_epoch: u64,
    #[serde(alias = "receptionReport")]
    reports: Vec<ReceptionReport>,
    #[serde(alias = "activeReceiver")]
    receivers: Vec<ActiveReceiver>
}

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
    snr: i16
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct ActiveReceiver {
    /// The callsign of the receiving station
    callsign: CallsignString,
    /// The grid locator of the receiving station
    #[serde(alias = "locator")]
    grid: GridString,
    /// The mode of the receiving station
    mode: ModeString
}
