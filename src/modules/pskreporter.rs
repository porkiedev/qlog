//
// A PSKReporter abstraction interface
//


use std::collections::HashMap;

use crate::RT;

use super::{gui::{self, Tab}, maidenhead, map};
use anyhow::Result;
use egui::{emath::TSTransform, Id, Mesh, Rect, Widget};
use log::debug;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::{runtime::Handle, task::JoinHandle};


#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PSKReporterTab {
    id: Id,
    #[serde(skip)]
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
                let markers = map_widget.markers_mut();
                
                // TODO: Remove this. This adds some dummy markers to the map for debug purposes
                let mut rng = rand::thread_rng();
                for _ in 0..500 {
                    let m = map::DummyMapMarker {
                        location: geo::coord! { x: rng.gen_range(-180.0..180.0), y: rng.gen_range(-85.0..85.0) },
                        callsign: arrayvec::ArrayString::from("ACALLS1GN").unwrap()
                    };
                    markers.push(m);
                }

                self.map = Some(map_widget);
                self.map.as_mut().unwrap()
            }
        };

        if ui.button("Test").clicked() {
            let fut = test();
            // let resp = config.runtime.block_on(fut);
            RT.spawn(fut);
            // debug!("Result: {resp:?}");

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


// const URL: &str = "https://www.pskreporter.info/cgi-bin/pskquery5.pl?encap=1&callback=doNothing&statistics=1&noactive=1&nolocator=1&flowStartSeconds=-43200&frange=6000000-8000000&mode=FT8&senderCallsign=KF0CZM&lastDuration=406";
const URL: &str = "https://www.pskreporter.info/cgi-bin/pskquery5.pl?encap=1&callback=doNothing&statistics=1&noactive=1&nolocator=1&flowStartSeconds=-900&receiverCallsign=VE4REM&lastDuration=4216";

async fn test() -> Result<()> {
    // Query PSKReporter
    let mut response = reqwest::get(URL).await?
    .text().await?;

    // debug!("Raw text:\n{response}");

    response.truncate(response.len() - 13);
    let _ = response.drain(..46);

    let data: PSKReporterApiResponse = serde_json::from_str(&response).unwrap();

    debug!("API Response:\n{}", serde_json::to_string_pretty(&data)?);

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct PSKReporterApiResponse {
    #[serde(alias = "currentSeconds")]
    current_epoch: u64,
    #[serde(alias = "receptionReport")]
    reports: Vec<NewReceptionReport>,
    // #[serde(alias = "activeReceiver")]
    // receivers: Vec<ActiveReceiver>
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct ReceptionReport {
    #[serde(alias = "receiverCallsign")]
    rx_callsign: String,
    #[serde(alias = "receiverLocator")]
    rx_grid: String,
    #[serde(alias = "senderCallsign")]
    tx_callsign: String,
    #[serde(alias = "senderLocator")]
    tx_grid: String,
    frequency: u64,
    #[serde(alias = "flowStartSeconds")]
    start_epoch: u64,
    mode: String,
    #[serde(alias = "isReceiver")]
    is_receiver: u8,
    #[serde(alias = "senderRegion")]
    tx_region: String,
    #[serde(alias = "senderDXCC")]
    tx_dxcc: String,
    #[serde(alias = "senderDXCCCode")]
    tx_dxcc_code: String,
    #[serde(alias = "senderDXCCLocator")]
    tx_dscc_grid: String,
    #[serde(alias = "sNR")]
    snr: i16
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct NewReceptionReport {
    #[serde(alias = "receiverCallsign")]
    rx_callsign: arrayvec::ArrayString<20>,
    #[serde(alias = "receiverLocator")]
    rx_grid: arrayvec::ArrayString<10>,
    #[serde(alias = "senderCallsign")]
    tx_callsign: arrayvec::ArrayString<20>,
    #[serde(alias = "senderLocator")]
    tx_grid: arrayvec::ArrayString<20>,
    frequency: u64,
    mode: arrayvec::ArrayString<16>,
    #[serde(alias = "sNR")]
    snr: i16
}

type CallsignString = arrayvec::ArrayString<12>;
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct ActiveReceiver {
    /// The callsign of the receiving station
    callsign: arrayvec::ArrayString<12>,
    /// The grid locator of the receiving station
    #[serde(alias = "locator")]
    grid: arrayvec::ArrayString<10>,
    /// The mode of the receiving station
    mode: arrayvec::ArrayString<16>
}
