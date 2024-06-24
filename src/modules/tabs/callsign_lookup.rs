//
// Contains code belonging to the callsign lookup tab
//

use anyhow::Result;
use log::error;
use poll_promise::Promise;
use serde::{Deserialize, Serialize};
use egui::{widgets, Align, Id, Layout, Ui, Widget, WidgetText};
use crate::{callsign_lookup, modules::gui::{generate_random_id, Tab}, types, GuiConfig};


/// The callsign lookup tab
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct CallsignLookupTab {
    id: Id,
    callsign: String,
    #[serde(skip)]
    callsign_info: Option<callsign_lookup::CallsignInformation>,
    #[serde(skip)]
    task: Option<Promise<Result<callsign_lookup::CallsignInformation>>>
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
                self.task = Some(config.cl_api.lookup_callsign_promise(callsign))
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
                self.task = Some(config.cl_api.lookup_callsign_promise(&self.callsign));
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
