//
// A PSKReporter abstraction interface
//


use std::collections::HashMap;

use super::{gui::{self, Tab}, map};
use anyhow::Result;
use egui::{emath::TSTransform, Id, Mesh, Rect, Widget};
use log::debug;
use serde::{Deserialize, Serialize};
use tokio::{runtime::Handle, task::JoinHandle};


#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PSKReporterTab {
    id: Id,
    #[serde(skip)]
    map: Option<map::MapWidget>
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
                self.map = Some(map::MapWidget::new(ui.ctx(), config));
                self.map.as_mut().unwrap()
            }
        };

        if ui.button("Test").clicked() {
            let fut = test();
            let resp = config.runtime.block_on(fut);

            debug!("Result: {resp:?}");
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

    debug!("Raw text:\n
    {response}");

    response.truncate(response.len() - 13);
    let _ = response.drain(..46);

    let data: PSKReporterApiResponse = serde_json::from_str(&response)?;

    debug!("API Response:\n{}", serde_json::to_string_pretty(&data)?);

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct PSKReporterApiResponse {
    #[serde(alias = "currentSeconds")]
    current_epoch: u64,
    #[serde(alias = "receptionReport")]
    reports: Vec<ReceptionReport>
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
