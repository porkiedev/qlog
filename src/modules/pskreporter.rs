//
// A PSKReporter abstraction interface
//


use std::collections::HashMap;

use crate::RT;

use super::{gui::{self, Tab}, maidenhead, map::{self, MapMarkerTrait}};
use anyhow::Result;
use egui::{emath::TSTransform, Id, Mesh, Rect, Widget};
use geo::Coord;
use log::{debug, error};
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
            let response = match response {
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

            // Iterate through the reception reports, convert them to map markers, and add them to the markers vec
            for report in response.reports {

                // Get the reciever location of the report. This populates rx_marker on the first iteration
                let rx_location = match &rx_marker {
                    Some(r) => *r.location(),
                    None => {
                        // Convert the receiver information into a marker and populate the rx marker option
                        let r = MapMarker::Receiver { id: self.rng.next_u64(), location: maidenhead::grid_to_lat_lon(&report.rx_grid) };
                        let location = *r.location();
                        rx_marker = Some(r);
                        location
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
        location: Coord<f64>
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
        if let Self::Transmitter { id, location, rx_location, inner } = self {
            
            ui.label(format!("TX Station: {}", inner.tx_callsign));
            ui.label(format!("RX Station: {}", inner.rx_callsign));

            let freq = gui::frequency_formatter(inner.frequency as f64, 0..=0);
            ui.label(format!("Frequency: {freq}"));

            ui.label(format!("SNR: {}dB", inner.snr));
            ui.label(format!("At: {}", inner.time));

        } else {
            ui.label("RECEIVER");
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
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
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
