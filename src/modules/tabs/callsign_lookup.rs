//
// Contains code belonging to the callsign lookup tab
//

use serde::{Deserialize, Serialize};
use egui::{widgets, Align, Id, Layout, Ui, Widget, WidgetText};
use crate::{callsign_lookup, modules::gui::{add_task_to_queue, generate_random_id, Tab}, types, GuiConfig};


/// The callsign lookup tab
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CallsignLookupTab {
    id: Id,
    callsign: String,
    #[serde(skip)]
    callsign_info: Option<callsign_lookup::CallsignInformation>
}
impl Tab for CallsignLookupTab {
    fn id(&self) -> Id {
        self.id
    }

    fn title(&mut self) -> WidgetText {
        "Callsign Lookup".into()
    }

    fn process_event(&mut self, _config: &mut GuiConfig, event: &types::Event) {
        if let types::Event::CallsignLookedUp(callsign_info) = event {
            self.callsign_info = Some(*callsign_info.clone());
        }
    }

    fn ui(&mut self, config: &mut GuiConfig, ui: &mut Ui) {

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

            // Show a button to search for the callsign
            if ui.button("\u{1F50D}").clicked() {
                add_task_to_queue(
                    &mut config.tasks,
                    config.cl_api.lookup_callsign(&self.callsign),
                    Some(self.id)
                );
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
            callsign_info: Default::default()
        }
    }
}
