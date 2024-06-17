//
// The settings tab module for the GUI
//

use std::{fmt::Debug, ops::RangeInclusive};
use egui::{Id, Widget};
use egui_dock::{DockState, TabViewer};
use serde::{Deserialize, Serialize};
use strum::{EnumCount, IntoEnumIterator};
use super::{gui, map};

/// The settings tab for the GUI
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsTab {
    /// The ID of the tab
    id: Id,
    #[serde(skip)]
    tabs: DockState<Box<dyn SettingsTabTrait>>
}
impl gui::Tab for SettingsTab {
    fn id(&self) -> Id {
        self.id
    }

    fn title(&mut self) -> egui::WidgetText {
        "Settings".into()
    }

    fn ui(&mut self, config: &mut crate::GuiConfig, ui: &mut egui::Ui) {

        // Render the settings tabs (i.e. the tabs that are shown in the settings menu)
        egui_dock::DockArea::new(&mut self.tabs)
        .id(self.id.with("_dock_area"))
        .show_inside(ui, &mut SettingsTabViewer { config });

    }
}
impl Default for SettingsTab {
    fn default() -> Self {
        Self {
            id: gui::generate_random_id(),
            tabs: DockState::new(vec![
                Box::new(PSKReporterSettingsTab),
                Box::new(MapSettingsTab)
            ])
        }
    }
}

struct SettingsTabViewer<'a> {
    config: &'a mut crate::GuiConfig
}
impl<'a> TabViewer for SettingsTabViewer<'a> {
    type Tab = Box<dyn SettingsTabTrait>;
    
    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.title()
    }
    
    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        tab.ui(self.config, ui)
    }

    fn allowed_in_windows(&self, _tab: &mut Self::Tab) -> bool {
        false
    }

    fn closeable(&mut self, _tab: &mut Self::Tab) -> bool {
        false
    }
    
}

trait SettingsTabTrait: Debug {
    fn title(&mut self) -> egui::WidgetText;
    fn ui(&mut self, config: &mut crate::GuiConfig, ui: &mut egui::Ui);
}

/// The PSKReporter settings tab
#[derive(Debug)]
struct PSKReporterSettingsTab;
impl PSKReporterSettingsTab {
    /// The minimum and maximum refresh rate allowed. The range is inclusive, in seconds, and from 1 to 30 minutes.
    const REFRESH_RATE_RANGE: RangeInclusive<u16> = 60..=1800;
}
impl SettingsTabTrait for PSKReporterSettingsTab {
    fn title(&mut self) -> egui::WidgetText {
        "PSKReporter".into()
    }

    fn ui(&mut self, config: &mut crate::GuiConfig, ui: &mut egui::Ui) {

        // The refresh rate setting
        ui.group(|ui| {

            // A label to describe the refresh rate option
            ui.label("Refresh interval in seconds (How often to check for new PSKReporter data)");

            // A horizontal layout to group the refresh rate slider and rate limiting note
            ui.horizontal(|ui| {

                // A slider/drag value to set the refresh rate
                egui::widgets::DragValue::new(&mut config.pskreporter_config.refresh_rate)
                .clamp_range(Self::REFRESH_RATE_RANGE)
                .speed(0.1)
                .update_while_editing(false)
                .ui(ui);
                
                // A note about rate limiting
                egui::widgets::Label::new("?")
                .ui(ui)
                .on_hover_text("Note: There is a lower limit of 60 seconds to avoid being rate limited by the PSKReporter API.
                The rate limit can still be reached if you have multiple PSKReporter tabs open at a time.
                Please be considerate of the API.");

            });
        });

    }
}

/// The map settings tab
#[derive(Debug)]
struct MapSettingsTab;
impl SettingsTabTrait for MapSettingsTab {
    fn title(&mut self) -> egui::WidgetText {
        "Map".into()
    }

    fn ui(&mut self, config: &mut crate::GuiConfig, ui: &mut egui::Ui) {

        // The map tile provider
        ui.group(|ui| {

            // A label to describe the map tile provider option
            ui.label("Map provider");

            // A combobox to select the map tile provider
            egui::ComboBox::from_id_source("map_tile_provider_combobox")
            .selected_text(config.map_config.tile_provider.as_str())
            .show_ui(ui, |ui| {
                for tile_provider in map::TileProvider::iter() {
                    let text = tile_provider.as_str();
                    ui.selectable_value(&mut config.map_config.tile_provider, tile_provider, text);
                }
            });

            // If the tile provider requires extra configuration, show the extra configuration options
            match &mut config.map_config.tile_provider {
                // OSM Doesn't require any extra configuration
                map::TileProvider::OpenStreetMap => {},
                // MapBox requires an access token and a style choice
                map::TileProvider::MapBox { access_token, style_owner, style } => {
                    
                    // A label to describe the style owner option
                    ui.label("Style Owner");
                    // The style owner textbox
                    egui::widgets::TextEdit::singleline(style_owner)
                    .hint_text("The style owner name")
                    .ui(ui);

                    // A label to describe the style option
                    ui.label("Style");
                    // The style textbox
                    egui::widgets::TextEdit::singleline(style)
                    .hint_text("The style name")
                    .ui(ui);

                    // A label to describe the access token option
                    ui.label("Access token");
                    // The access token textbox
                    egui::widgets::TextEdit::singleline(access_token)
                    .hint_text("Your access token for the MapBox API")
                    .password(true)
                    .ui(ui);

                },
                // CartoCDN requires an access token and a style choice
                map::TileProvider::CartoCDN { access_token, style } => {

                    // A label to describe the style option
                    ui.label("Style");
                    // The style combobox
                    egui::ComboBox::from_id_source("map_cartocdn_style_combobox")
                    .selected_text(style.name())
                    .show_ui(ui, |ui| {
                        for style_opt in map::CartoCDNStyle::iter() {
                            let text = style_opt.name();
                            ui.selectable_value(style, style_opt, text);
                        }
                    });

                    // A label to describe the access token option
                    ui.label("Access token");
                    // The access token textbox
                    egui::widgets::TextEdit::singleline(access_token)
                    .hint_text("Your access token for the CartoCDN API")
                    .password(true)
                    .ui(ui);

                }
            }

        });

    }
}
