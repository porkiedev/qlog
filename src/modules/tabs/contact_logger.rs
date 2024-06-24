//
// Contains the code for the contact logger tab
//

use anyhow::Result;
use chrono::{NaiveDate, NaiveTime, Utc};
use log::{error, warn};
use poll_promise::Promise;
use serde::{Deserialize, Serialize};
use egui::{widgets, Id, Ui, Vec2, Widget, WidgetText};
use strum::IntoEnumIterator;
use crate::{modules::{gui::{frequency_formatter, frequency_parser, generate_random_id, power_formatter, power_parser}, types}, GuiConfig, Tab};

/// The contact logger tab
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct ContactLoggerTab {
    /// The egui ID
    id: Id,
    /// The contact. When possible, widgets will modify the values here directly
    input: types::Contact,
    /// The start date of the contact as a string
    start_date_str: String,
    /// The start time of the contact as a string
    start_time_str: String,
    /// The end date of the contact
    end_date: NaiveDate,
    /// The end time of the contact
    end_time: NaiveTime,
    /// The end date of the contact as a string
    end_date_str: String,
    /// The end time of the contact as a string
    end_time_str: String,
    /// The task that is currently running to insert the contact into the database
    #[serde(skip)]
    task: Option<Promise<Result<types::Contact>>>
}
impl ContactLoggerTab {
    /// Updates the start date and time of the contact to 'now'
    fn update_start_date_time(&mut self) {
        // Get the current date and time
        let dt = Utc::now();

        // Update the start date and time in the contact
        self.input.date = dt.date_naive();
        self.input.time = dt.time();

        // Update the date and time strings
        self.start_date_str = format!("{}", self.input.date.format("%Y-%m-%d"));
        self.start_time_str = format!("{}", self.input.time.format("%H:%M:%S"));
    }

    /// Updates the end date and time of the contact to `now`
    fn update_end_date_time(&mut self) {
        // Get the current date and time
        let dt = Utc::now();

        // Update the stop date and time
        self.end_date = dt.date_naive();
        self.end_time = dt.time();

        // Update the date and time strings
        self.end_date_str = format!("{}", self.end_date.format("%Y-%m-%d"));
        self.end_time_str = format!("{}", self.end_time.format("%H:%M:%S"));
    }
}
impl Tab for ContactLoggerTab {

    fn id(&self) -> Id {
        self.id
    }
    
    fn title(&mut self) -> WidgetText {
        "Contact Logger".into()
    }
    
    fn ui(&mut self, config: &mut GuiConfig, ui: &mut Ui) {

        // Process any pending tasks
        if let Some(task) = self.task.take_if(|t| t.ready().is_some()) {
            // If the contact was added successfully, send a refresh contacts event, otherwise print the error
            match task.block_and_take() {
                Ok(_contact) => config.events.push((None, types::Event::RefreshContacts)),
                Err(err) => error!("Failed to insert contact: {err}")
            }
        }

        // The horizontal spacing between widgets
        let spacing = ui.style().spacing.item_spacing.x;
        // The available width in the tab
        let available_width = ui.available_width() - spacing;

        // Horizontally group the callsign, start date/time textboxes, and the update time button
        ui.horizontal(|ui| {

            // Subtract the spacing and button width from the available width
            let available_width = available_width - spacing - 28.0;

            // Callsign textbox (50% width)
            ui.vertical(|ui| {
                ui.add(widgets::Label::new("Callsign").wrap(false));
                
                widgets::TextEdit::singleline(&mut self.input.callsign)
                .hint_text("Callsign")
                .clip_text(true)
                .min_size(Vec2::new(available_width * 0.5, 0.0))
                .desired_width(0.0)
                .show(ui);
            });

            // The start date textbox (25% width)
            ui.vertical(|ui| {
                ui.add(widgets::Label::new("Start date").wrap(false));

                // Render the date textedit widget
                let response = widgets::TextEdit::singleline(&mut self.start_date_str)
                .hint_text("Date in Y-M-D format")
                .clip_text(true)
                .min_size(Vec2::new(available_width * 0.25, 0.0))
                .desired_width(0.0)
                .show(ui)
                .response;

                // The widget lost focus (the user hit enter or clicked elsewhere). Try to parse the string into a valid date
                if response.lost_focus() {
                    match NaiveDate::parse_from_str(&self.start_date_str, "%Y-%m-%d") {
                        Ok(d) => self.input.date = d,
                        Err(err) => {
                            warn!("Failed to parse start date (input: '{}'): {err}", self.start_date_str);
                            self.start_date_str = format!("{}", self.input.date.format("%Y-%m-%d"));
                        }
                    }
                }
            });

            // The start time textbox (25% width)
            ui.vertical(|ui| {
                ui.add(widgets::Label::new("Start time").wrap(false));

                // Render the time textedit widget
                let response = widgets::TextEdit::singleline(&mut self.start_time_str)
                .hint_text("Time in HH:MM:SS format")
                .clip_text(true)
                .min_size(Vec2::new(available_width * 0.25, 0.0))
                .desired_width(0.0)
                .show(ui)
                .response;

                // The widget lost focus (the user hit enter or clicked elsewhere). Try to parse the string into a valid time
                if response.lost_focus() {
                    match NaiveTime::parse_from_str(&self.start_time_str, "%H:%M:%S") {
                        Ok(t) => self.input.time = t,
                        Err(err) => {
                            warn!("Failed to parse start time (input: '{}'): {err}", self.start_time_str);
                            self.start_time_str = format!("{}", self.input.time.format("%H:%M:%S"));
                        }
                    }
                }
            });

            // A button to refresh the date and time
            ui.vertical(|ui| {

                // Add some vertical space so our button lines up with the textboxes
                ui.add_space(17.0);

                // A button to refresh the date and time
                let b = ui.button("\u{21BB}");

                // If the button was clicked, update the date and time of the contact
                if b.clicked() {
                    self.update_start_date_time();
                };

                // On hover, display some text explaining what the button does
                b.on_hover_text("Refresh the date and time");

            });

        });

        // Horizontally group the tx/rx RST, end date/time textboxes, and the update time button
        ui.horizontal(|ui| {

            // let available_width = ui.available_width() - ui.style().spacing.item_spacing.x;
            // Subtract the spacing and button width from the available width
            let available_width = available_width - spacing - 28.0;

            // The TX RST textbox
            ui.vertical(|ui| {
                ui.add(widgets::Label::new("TX RST").wrap(false));

                widgets::TextEdit::singleline(&mut self.input.tx_rst)
                .hint_text("TX RST")
                .clip_text(true)
                .min_size(Vec2::new((available_width * 0.25) - spacing * 0.5, 0.0))
                .desired_width(0.0)
                .show(ui);
            });

            // The RX RST textbox
            ui.vertical(|ui| {
                ui.add(widgets::Label::new("RX RST").wrap(false));

                widgets::TextEdit::singleline(&mut self.input.rx_rst)
                .hint_text("RX RST")
                .clip_text(true)
                .min_size(Vec2::new((available_width * 0.25) - spacing * 0.5, 0.0))
                .desired_width(0.0)
                .show(ui);
            });

            // The end date textbox (25% width)
            ui.vertical(|ui| {
                ui.add(widgets::Label::new("End date").wrap(false));

                // Render the date textedit widget
                let response = widgets::TextEdit::singleline(&mut self.end_date_str)
                .hint_text("Date in Y-M-D format")
                .clip_text(true)
                .min_size(Vec2::new(available_width * 0.25, 0.0))
                .desired_width(0.0)
                .show(ui)
                .response;

                // The widget lost focus (the user hit enter or clicked elsewhere). Try to parse the string into a valid date
                if response.lost_focus() {
                    match NaiveDate::parse_from_str(&self.end_date_str, "%Y-%m-%d") {
                        Ok(d) => self.end_date = d,
                        Err(err) => {
                            warn!("Failed to parse end date (input: '{}'): {err}", self.end_date_str);
                            self.end_date_str = format!("{}", self.end_date.format("%Y-%m-%d"));
                        }
                    }
                }
            });

            // The end time textbox (25% width)
            ui.vertical(|ui| {
                ui.add(widgets::Label::new("End time").wrap(false));

                // Render the time textedit widget
                let response = widgets::TextEdit::singleline(&mut self.end_time_str)
                .hint_text("Time in HH:MM:SS format")
                .clip_text(true)
                .min_size(Vec2::new(available_width * 0.25, 0.0))
                .desired_width(0.0)
                .show(ui)
                .response;

                // The widget lost focus (the user hit enter or clicked elsewhere). Try to parse the string into a valid time
                if response.lost_focus() {
                    match NaiveTime::parse_from_str(&self.end_time_str, "%H:%M:%S") {
                        Ok(t) => self.end_time = t,
                        Err(err) => {
                            warn!("Failed to parse end time (input: '{}'): {err}", self.end_time_str);
                            self.end_time_str = format!("{}", self.end_time.format("%H:%M:%S"));
                        }
                    }
                }
            });

            // A button to refresh the date and time
            ui.vertical(|ui| {

                // Add some vertical space so our button lines up with the textboxes
                ui.add_space(17.0);

                // A button to refresh the date and time
                let b = ui.button("\u{21BB}");

                // If the button was clicked, update the stop date and time of the contact
                if b.clicked() {
                    self.update_end_date_time();
                };

                // On hover, display some text explaining what the button does
                b.on_hover_text("Refresh the date and time");

            });

        });

        // Horizontally group the Mode checkbox, TX/RX power, and frequency drag values
        ui.horizontal(|ui| {

            // Mode comobobox
            ui.vertical(|ui| {
                ui.add(widgets::Label::new("Mode").wrap(false));

                egui::ComboBox::from_id_source("mode_combobox")
                .selected_text(self.input.mode.to_string())
                .show_ui(ui, |ui| {
                    
                    // Iterate through each mode variant and create a selectable value
                    for mode in types::Mode::iter() {
                        // Get the name of the mode
                        let text = mode.to_string();

                        // Create the selectable value
                        ui.selectable_value(&mut self.input.mode, mode, text);
                    }

                });
            });

            // TX power drag value
            ui.vertical(|ui| {
                ui.add(widgets::Label::new("TX Power").wrap(false));

                widgets::DragValue::new(&mut self.input.tx_power)
                .speed(100.0)
                .custom_formatter(power_formatter)
                .custom_parser(power_parser)
                .update_while_editing(false)
                .ui(ui);

            });

            // RX power drag value
            ui.vertical(|ui| {
                ui.add(widgets::Label::new("RX Power").wrap(false));

                widgets::DragValue::new(&mut self.input.rx_power)
                .speed(100.0)
                .custom_formatter(power_formatter)
                .custom_parser(power_parser)
                .update_while_editing(false)
                .ui(ui);

            });

            // Frequency drag value
            ui.vertical(|ui| {
                ui.add(widgets::Label::new("Frequency").wrap(false));

                widgets::DragValue::new(&mut self.input.frequency)
                .speed(1000.0)
                .custom_formatter(frequency_formatter)
                .custom_parser(frequency_parser)
                .update_while_editing(false)
                .ui(ui);

            });

        });

        // The 'note' textbox
        ui.vertical(|ui| {
            ui.add(widgets::Label::new("Note").wrap(false));

            widgets::TextEdit::multiline(&mut self.input.note)
            .hint_text("A note about the contact")
            .clip_text(true)
            .desired_width(available_width)
            .show(ui);
        });

        // Add some space before the submit button
        ui.add_space(16.0);

        // The submit button
        ui.vertical_centered_justified(|ui| {
            let response = ui.add_enabled(self.task.is_none(), widgets::Button::new("Submit"));
            if response.clicked() {

                // Calculate the duration of the contact using the start and end date/time and store it in the contact
                let elapsed = self.end_date.and_time(self.end_time).signed_duration_since(self.input.date.and_time(self.input.time)).num_seconds();
                // Ensure the duration is positive, showing an error if it is negative
                if elapsed.is_negative() {
                    config.notification_read = false;
                    config.notifications.push(types::Notification::Error("The end time must be after the start time".to_string()));
                    return;
                }
                // Update the duration of the contact
                self.input.duration = elapsed as u64;

                // Insert the contact into the database
                self.task = Some(config.db_api.insert_contact_promise(self.input.clone()));

            };
        });
    }
    
}
impl Default for ContactLoggerTab {
    fn default() -> Self {
        let mut s = Self {
            id: generate_random_id(),
            input: Default::default(),
            start_date_str: Default::default(),
            start_time_str: Default::default(),
            end_date: Default::default(),
            end_time: Default::default(),
            end_date_str: Default::default(),
            end_time_str: Default::default(),
            task: Default::default()
        };

        // Update the date and time to 'now' when this tab is first created
        s.update_start_date_time();
        s.update_end_date_time();

        s
    }
}
impl std::fmt::Debug for ContactLoggerTab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContactLoggerTab")
        .field("id", &self.id)
        .field("input", &self.input)
        .field("start_date_str", &self.start_date_str)
        .field("start_time_str", &self.start_time_str)
        .field("end_date", &self.end_date)
        .field("end_time", &self.end_time)
        .field("end_date_str", &self.end_date_str)
        .field("end_time_str", &self.end_time_str)
        .finish()
    }
}
