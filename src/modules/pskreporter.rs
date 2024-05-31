//
// A PSKReporter abstraction interface
//


use std::collections::HashMap;

use crate::RT;

use super::{gui::{self, Tab}, maidenhead, map::{self, MapMarkerTrait}};
use anyhow::Result;
use egui::{emath::TSTransform, Id, Mesh, Rect, Widget};
use geo::Coord;
use log::debug;
use rand::{Rng, RngCore, SeedableRng};
use serde::{Deserialize, Serialize};
use tokio::{runtime::Handle, task::JoinHandle};


type CallsignString = arrayvec::ArrayString<20>;
type GridString = arrayvec::ArrayString<10>;
type ModeString = arrayvec::ArrayString<16>;


#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PSKReporterTab {
    id: Id,
    #[serde(skip)]
    // map: Option<map::MapWidget<MapMarker>>
    map: Option<map::MapWidget<map::DummyMapMarker>>
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
                let mut rng = rand::rngs::SmallRng::from_entropy();
                let markers = map_widget.markers_mut();

                // TODO: Remove this. This adds some dummy markers to the map for debug purposes
                for _ in 0..25 {
                    let m = map::DummyMapMarker {
                        id: rng.next_u64(),
                        location: geo::coord! { x: rng.gen_range(-180.0..180.0), y: rng.gen_range(-85.0..85.0) }
                    };
                    markers.push(m);
                }
                map_widget.update_overlay();

                self.map = Some(map_widget);
                self.map.as_mut().unwrap()
            }
        };

        if ui.button("Test").clicked() {
            // let fut = test();
            // let resp = RT.block_on(fut).unwrap();

            // let markers = map.markers_mut();
            // markers.clear();
            // for report in resp.reports {
            //     let m = MapMarker::new(report);
            //     markers.push(m);
            // }
            // map.update_overlay();

            // // RT.spawn(fut);
            // // debug!("Result: {resp:?}");

        };

        // Show the map widget
        map.ui(ui, config);

    }
}
impl Default for PSKReporterTab {
    fn default() -> Self {
        Self {
            id: gui::generate_random_id(),
            map: Default::default()
        }
    }
}

struct MapMarker {
    location: Coord<f64>,
    inner: ReceptionReport
}
impl MapMarker {
    fn new(report: ReceptionReport) -> Self {
        let location = maidenhead::grid_to_lat_lon(&report.tx_grid);
        Self {
            location,
            inner: report
        }
    }
}
impl MapMarkerTrait for MapMarker {
    fn id(&self) -> u64 {
        0
    }

    fn location(&self) -> &geo::Coord<f64> {
        &self.location
    }

    fn hovered_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(format!("TX Station: {}", self.inner.tx_callsign));
        ui.label(format!("RX Station: {}", self.inner.rx_callsign));

        let freq = gui::frequency_formatter(self.inner.frequency as f64, 0..=0);
        ui.label(format!("Frequency: {freq}"));

        ui.label(format!("SNR: {}dB", self.inner.snr));
        ui.label(format!("At: {}", self.inner.time));
    }

    fn color(&self) -> image::Rgba<u8> {
        image::Rgba([0, 255, 0, 255])
    }
}

// const URL: &str = "https://www.pskreporter.info/cgi-bin/pskquery5.pl?encap=1&callback=doNothing&statistics=1&noactive=1&nolocator=1&flowStartSeconds=-43200&frange=6000000-8000000&mode=FT8&senderCallsign=KF0CZM&lastDuration=406";
// const URL: &str = "https://www.pskreporter.info/cgi-bin/pskquery5.pl?encap=1&callback=doNothing&statistics=1&noactive=1&nolocator=1&flowStartSeconds=-900&receiverCallsign=VE4REM&lastDuration=4216";
const URL: &str = "https://www.pskreporter.info/cgi-bin/pskquery5.pl?encap=1&callback=doNothing&statistics=1&noactive=1&nolocator=1&flowStartSeconds=-3600&mode=FT8&receiverCallsign=VE4REM&lastseqno=47010219750&lastDuration=402";

async fn test() -> Result<PSKReporterApiResponse> {
    // Query PSKReporter
    let mut response = reqwest::get(URL).await?
    .text().await?;

    response.truncate(response.len() - 13);
    let _ = response.drain(..46);

    // log::info!("Response:\n{response}");

    let data: PSKReporterApiResponse = serde_json::from_str(&response).unwrap();

    // debug!("API Response:\n{}", serde_json::to_string_pretty(&data)?);

    Ok(data)
}

#[derive(Debug, Serialize, Deserialize)]
struct PSKReporterApiResponse {
    #[serde(alias = "currentSeconds")]
    current_epoch: u64,
    #[serde(alias = "receptionReport")]
    reports: Vec<ReceptionReport>,
    // #[serde(alias = "activeReceiver")]
    // receivers: Vec<ActiveReceiver>
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
