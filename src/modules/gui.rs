//
// The GUI code. This contains the immediate-mode code for the different types of GUI tabs.
//


use std::{ops::RangeInclusive, time::Duration};
use chrono::{NaiveDate, NaiveTime, Utc};
use egui::{widgets, Align, CursorIcon, Id, Layout, RichText, Ui, Vec2, Widget, WidgetText};
use log::{debug, error, trace, warn};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use tokio::sync::{oneshot, watch};
use crate::GuiConfig;
use super::{callsign_lookup, database, types::{self, SpawnedFuture}};


/// The tab trait. This should be implemented for each tab variant
pub trait Tab {
    /// The [Id] of the tab.
    /// 
    /// For non-interactive tabs, this can be a static value, however, for interactive tabs,
    /// this should be randomly generated via [generate_random_id()] at tab creation time, cached, and returned.
    fn id(&self) -> Id;

    /// Should horizontal and vertical scrollbars be created if necessary?
    /// 
    /// This is `[true, true]` by default.
    fn scroll_bars(&self) -> [bool; 2] {[true, true]}

    /// The title of the tab.
    fn title(&mut self) -> WidgetText;

    /// An initialization function. This is called by the GUI when the application is initializing all of the saved tabs.
    /// 
    /// - NOTE: This function is only called once, either at application startup or when the tab is created.
    #[allow(unused)]
    fn init(&mut self, config: &mut GuiConfig) {}

    /// A task processing function. This is called before each frame, for every tab, regardless of its visibility.
    /// 
    /// If your tab spawns async tasks and needs to check if the tasks are finished before rendering, you can do it in this function.
    /// 
    /// Also, if your tab makes changes that requires synchronization between other tabs, you are given mutable access to all other tabs,
    /// which you can use to iterate through every tab and call `process_event` with your event.
    #[allow(unused)]
    fn process_tasks(&mut self, config: &mut GuiConfig, mut other_tabs: Vec<&mut TabVariant>) {}

    /// If you want your tab to update on one (or more) of the events in `types::GlobalEvent`,
    /// implement this function and pattern match for the event that you want to respond to.
    #[allow(unused)]
    fn process_event(&mut self, config: &mut GuiConfig, event: &types::Event) {}

    /// Renders the UI layout for the tab.
    /// 
    /// - NOTE: Unlike `process_events`, this function is only called for visible tabs.
    fn ui(&mut self, config: &mut GuiConfig, ui: &mut Ui);
}


/// The different GUI tab variants
#[derive(Debug, Serialize, Deserialize)]
pub enum TabVariant {
    /// The default welcome tab
    Welcome(Box<WelcomeTab>),
    /// A table that visualizes all of the logged contacts/QSOs
    ContactTable(Box<ContactTableTab>),
    /// A tab for logging contacts
    ContactLogger(Box<ContactLoggerTab>),
    /// A tab for looking up callsigns
    CallsignLookup(Box<CallsignLookupTab>)
}
impl Tab for TabVariant {

    fn id(&self) -> Id {
        match self {
            TabVariant::Welcome(data) => data.id(),
            TabVariant::ContactTable(data) => data.id(),
            TabVariant::ContactLogger(data) => data.id(),
            TabVariant::CallsignLookup(data) => data.id(),
        }
    }

    fn scroll_bars(&self) -> [bool; 2] {
        match self {
            TabVariant::Welcome(data) => data.scroll_bars(),
            TabVariant::ContactTable(data) => data.scroll_bars(),
            TabVariant::ContactLogger(data) => data.scroll_bars(),
            TabVariant::CallsignLookup(data) => data.scroll_bars(),
        }
    }

    fn title(&mut self) -> WidgetText {
        match self {
            TabVariant::Welcome(data) => data.title(),
            TabVariant::ContactTable(data) => data.title(),
            TabVariant::ContactLogger(data) => data.title(),
            TabVariant::CallsignLookup(data) => data.title(),
        }
    }

    fn init(&mut self, config: &mut GuiConfig) {
        match self {
            TabVariant::Welcome(data) => data.init(config),
            TabVariant::ContactTable(data) => data.init(config),
            TabVariant::ContactLogger(data) => data.init(config),
            TabVariant::CallsignLookup(data) => data.init(config),
        }
    }

    fn process_tasks(&mut self, config: &mut GuiConfig, other_tabs: Vec<&mut TabVariant>) {
        match self {
            TabVariant::Welcome(data) => data.process_tasks(config, other_tabs),
            TabVariant::ContactTable(data) => data.process_tasks(config, other_tabs),
            TabVariant::ContactLogger(data) => data.process_tasks(config, other_tabs),
            TabVariant::CallsignLookup(data) => data.process_tasks(config, other_tabs),
        }
    }

    fn process_event(&mut self, config: &mut GuiConfig, event: &types::Event) {
        match self {
            TabVariant::Welcome(data) => data.process_event(config, event),
            TabVariant::ContactTable(data) => data.process_event(config, event),
            TabVariant::ContactLogger(data) => data.process_event(config, event),
            TabVariant::CallsignLookup(data) => data.process_event(config, event),
        }
    }

    fn ui(&mut self, config: &mut GuiConfig, ui: &mut Ui) {
        match self {
            TabVariant::Welcome(data) => data.ui(config, ui),
            TabVariant::ContactTable(data) => data.ui(config, ui),
            TabVariant::ContactLogger(data) => data.ui(config, ui),
            TabVariant::CallsignLookup(data) => data.ui(config, ui),
        }
    }
    
}
impl Default for TabVariant {
    fn default() -> Self {
        Self::Welcome(Box::default())
    }
}


/// The [TabVariant::Welcome] tab
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct WelcomeTab;
impl Tab for WelcomeTab {
    fn id(&self) -> Id {
        Id::new("welcome_tab")
    }

    fn title(&mut self) -> WidgetText {
        "Welcome to QLog".into()
    }

    fn ui(&mut self, _config: &mut GuiConfig, ui: &mut Ui) {
        ui.label("Welcome to QLog");
    }
}

/// The [TabVariant::ContactLogger] tab
#[derive(Debug, Serialize, Deserialize)]
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
    end_time_str: String
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
            if ui.button("Submit").clicked() {

                // Calculate the duration of the contact using the start and end date/time and store it in the contact
                let elapsed = self.end_date.and_time(self.end_time).signed_duration_since(self.input.date.and_time(self.input.time)).num_seconds();
                self.input.duration = elapsed as u64;

                // Insert the contact into the database
                config.tasks.push((None, config.db_api.insert_contact(self.input.clone())));
                debug!("contact insert task has been added to queue ({})", config.tasks.len());

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
            end_time_str: Default::default()
        };

        // Update the date and time to 'now' when this tab is first created
        s.update_start_date_time();
        s.update_end_date_time();

        s
    }
}

/// The [TabVariant::ContactTable] tab
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ContactTableTab {
    /// The egui ID
    id: Id,
    /// The contacts that are shown in the contact table
    #[serde(skip)]
    contacts: Vec<types::Contact>,
    /// The current column to sort the contacts by
    sort_column: Option<database::ContactTableColumn>,
    /// The current direction to sort the contacts in
    sort_dir: database::ColumnSortDirection,
    #[serde(skip)]
    /// The row and column that is currently being edited, if any (row_idx, column)
    editing_column: Option<(usize, database::ContactTableColumn)>,
    /// The date string used when editing a date column on a contact
    #[serde(skip)]
    date_str: String,
    /// The time string used when editing a time column on a contact
    #[serde(skip)]
    time_str: String
}
impl Tab for ContactTableTab {

    fn id(&self) -> Id {
        self.id
    }
    
    fn title(&mut self) -> WidgetText {
        "Contacts".into()
    }

    fn scroll_bars(&self) -> [bool; 2] {
        [true, false]
    }

    // Create a 1s timer that's used to query the db and update the table
    fn init(&mut self, config: &mut GuiConfig) {
        trace!("[ContactTableTab] Initializing table");

        // Load contacts from the database
        add_task_to_queue(
            &mut config.tasks,
            config.db_api.get_contacts(0, self.sort_column, Some(self.sort_dir)),
            Some(self.id)
        );
    }

    fn process_event(&mut self, config: &mut GuiConfig, event: &types::Event) {

        match event {
            // A new contact was added to the database, so update the table
            types::Event::AddedContact(_contact) => {
                add_task_to_queue(
                    &mut config.tasks,
                    config.db_api.get_contacts(0, self.sort_column, Some(self.sort_dir)),
                    Some(self.id)
                );
            },
            types::Event::GotContacts(contacts) => {
                self.contacts.clone_from(contacts);
            },
            // A contact in the database was updated, so update the table
            types::Event::UpdatedContact(_contact) => {
                add_task_to_queue(
                    &mut config.tasks,
                    config.db_api.get_contacts(0, self.sort_column, Some(self.sort_dir)),
                    Some(self.id)
                );
            }
            // A contact was deleted from the database, so remove that contact from the table (if it exists)
            types::Event::DeletedContact(contact) => {
                self.contacts.retain(|c| c.id != contact.id);
            },
            // Multiple contacts were deleted from the database, so remove all of them (if they exist)
            types::Event::DeletedContacts(contacts) => {
                for contact in contacts {
                    self.contacts.retain(|c| c.id != contact.id);
                }
            }
            _ => {}
        }

    }

    fn ui(&mut self, config: &mut GuiConfig, ui: &mut Ui) {
        use egui_extras::Column;

        // Enforce a minimum width for the tab. The tab will automatically add horizontal scrollbars if the window is too small.
        // This stops us from making the table unreasonably small.
        ui.set_min_width(300.0);

        egui_extras::TableBuilder::new(ui)
        .columns(Column::initial(50.0).at_least(50.0), 1) // Callsign
        .columns(Column::initial(70.0).at_least(70.0), 1) // Frequency
        .columns(Column::initial(35.0).at_least(35.0), 1) // Mode
        .columns(Column::initial(40.0).at_least(40.0), 2) // TX and RX RST
        .columns(Column::initial(55.0).at_least(55.0), 2) // TX and RX Power
        .column(Column::initial(70.0).at_least(70.0)) // Date
        .column(Column::initial(50.0).at_least(50.0)) // Time
        .columns(Column::remainder().at_least(50.0).clip(true), 1) // Note
        .cell_layout(Layout::top_down(Align::Center))
        .resizable(true)
        .striped(true)
        .min_scrolled_height(20.0).sense(egui::Sense::click())
        .header(20.0, |mut header| {

            // Iterate through each viewable column and render it
            for column in database::ContactTableColumn::iter() {

                // Highlight this column if it's selected
                let selected = self.sort_column == Some(column);
                header.set_selected(selected);

                // Render the column label and return the response of the whole column
                let response = header.col(|ui| {
                    let text = RichText::new(column.to_string()).strong();
                    let widget = widgets::Label::new(text).selectable(false);
                    ui.add(widget);
                }).1;

                // If the column is sortable, update the cursor on hover, and return true if clicked
                let clicked = match column.is_sortable() {
                    true => response.on_hover_cursor(CursorIcon::PointingHand).clicked(),
                    false => false,
                };

                // If the column was clicked, go through the sorting logic (ascending, descending, none)
                if clicked {

                    // If this column was clicked and a different column was already selected, reset the state to avoid ruining the sort logic
                    if let Some(c) = self.sort_column {
                        if c != column {
                            self.sort_column = None;
                            self.sort_dir = database::ColumnSortDirection::Ascending;
                        }
                    }
    
                    // The column is not selected, select it and sort in ascending order
                    if self.sort_column.is_none() && self.sort_dir == database::ColumnSortDirection::Ascending {
                        self.sort_column = Some(column);
                    }
                    // The column is already selected, switch to descending order
                    else if self.sort_column.is_some() && self.sort_dir == database::ColumnSortDirection::Ascending {
                        self.sort_dir = database::ColumnSortDirection::Descending;
                    }
                    // The column is already slected and in descending order, reset the sort state
                    else {
                        self.sort_column = None;
                        self.sort_dir = database::ColumnSortDirection::Ascending;
                    }
    
                    // Update the table now that our sort state changed
                    add_task_to_queue(
                        &mut config.tasks,
                        config.db_api.get_contacts(0, self.sort_column, Some(self.sort_dir)),
                        Some(self.id)
                    );

                }

            }

        }).body(|body| {

            // Create a new row for each contact
            body.rows(20.0, self.contacts.len(), |mut row| {

                // Get the index of this row
                let row_index = row.index();

                // Get the contact that this row belongs to
                let contact = match self.contacts.get_mut(row_index) {
                    Some(c) => c,
                    None => return
                };

                // ===== CALLSIGN COLUMN ===== //
                let (_rect, response) = row.col(|ui| {

                    // This column is currently being edited, show a textedit
                    if self.editing_column.is_some_and(|(idx, c)| idx == row_index && c.is_callsign()) {
                        // Show a textedit widget
                        let w = widgets::TextEdit::singleline(&mut contact.callsign)
                        .horizontal_align(Align::Center)
                        .desired_width(f32::INFINITY)
                        .margin(egui::Margin::same(2.0))
                        .show(ui);

                        // The textedit lost focus, implying that the user wants to save the changes
                        if w.response.lost_focus() {
                            // Stop editing the column
                            self.editing_column = None;

                            // Update the contact
                            config.tasks.push((None, config.db_api.update_contact(contact.clone())));
                        };

                        // Focuses the textedit when a column is being edited
                        w.response.request_focus();
                    }
                    // This column isn't being edited, show a label
                    else {
                        // Show a label widget
                        widgets::Label::new(&contact.callsign)
                        .truncate(true)
                        .selectable(false)
                        .ui(ui);
                    }

                });
                // The callsign column was double clicked; start editing the column
                if response.double_clicked() {
                    self.editing_column = Some((row_index, database::ContactTableColumn::Callsign));
                }
                
                // ===== FREQUENCY COLUMN ===== //
                let (_rect, response) = row.col(|ui| {

                    // This column is currently being edited, show a frequency edit widget
                    if self.editing_column.is_some_and(|(idx, c)| idx == row_index && c.is_frequency()) {

                        // Show a frequency edit widget
                        let w = widgets::DragValue::new(&mut contact.frequency)
                        .custom_formatter(frequency_formatter)
                        .custom_parser(frequency_parser)
                        .update_while_editing(false)
                        .ui(ui);

                        // The widget lost focus, implying that the user wants to save the changes
                        if w.lost_focus() {
                            // Stop editing the column
                            self.editing_column = None;

                            // Update the contact
                            add_task_to_queue(
                                &mut config.tasks,
                                config.db_api.update_contact(contact.clone()),
                                None
                            );
                        };

                        // Focuses the widget when a column is being edited
                        w.request_focus();
                    }
                    // This column isn't being edited, show a label
                    else {
                        // Show a label widget
                        widgets::Label::new(frequency_formatter(contact.frequency as f64, 0..=0))
                        .truncate(true)
                        .selectable(false)
                        .ui(ui);
                    }

                });
                // The frequency column was double clicked; start editing the column
                if response.double_clicked() {
                    self.editing_column = Some((row_index, database::ContactTableColumn::Frequency));
                }

                // ===== MODE COLUMN ===== //
                let (_rect, response) = row.col(|ui| {

                    // This column is currently being edited, show a frequency edit widget
                    if self.editing_column.is_some_and(|(idx, c)| idx == row_index && c.is_mode()) {

                        // Was a button in the combobox clicked (i.e. should we save the contact)?
                        let mut saved = false;

                        // Horizontally group the mode combobox (and textedit box if the 'other' mode was chosen)
                        ui.horizontal(|ui| {

                            // Show a mode combobox widget
                            egui::ComboBox::from_id_source("mode_combobox")
                            .selected_text(contact.mode.to_string())
                            .show_ui(ui, |ui| {

                                // Iterate through each mode variant and create a selectable value
                                for mode in types::Mode::iter() {
                                    // Get the name of the mode
                                    let text = mode.to_string();

                                    // Create the selectable value
                                    if ui.selectable_value(&mut contact.mode, mode.clone(), text).clicked() {
                                        saved |= true;
                                    }

                                }

                            });

                            // User selected the `other` mode, so render a textedit box that they can type the mode name into
                            if let types::Mode::OTHER(mode_name) = &mut contact.mode {
                                if ui.text_edit_singleline(mode_name).lost_focus() {
                                    // Stop editing the column
                                    self.editing_column = None;

                                    // Update the contact
                                    add_task_to_queue(
                                        &mut config.tasks,
                                        config.db_api.update_contact(contact.clone()),
                                        None
                                    );
                                };
                            }

                        });

                        // Save if a combobox option was clicked
                        if saved && !contact.mode.is_other() {
                            // Stop editing the column
                            self.editing_column = None;

                            // Update the contact
                            add_task_to_queue(
                                &mut config.tasks,
                                config.db_api.update_contact(contact.clone()),
                                None
                            );
                        };
                    }
                    // This column isn't being edited, show a label
                    else {
                        // Show a label widget
                        widgets::Label::new(contact.mode.to_string())
                        .truncate(true)
                        .selectable(false)
                        .ui(ui);
                    }

                });
                // The mode column was double clicked; start editing the column
                if response.double_clicked() {
                    self.editing_column = Some((row_index, database::ContactTableColumn::Mode));
                }

                // ===== TX RST COLUMN ===== //
                let (_rect, response) = row.col(|ui| {

                    // This column is currently being edited, show a textedit widget
                    if self.editing_column.is_some_and(|(idx, c)| idx == row_index && c.is_tx_rst()) {

                        // Show a textedit widget
                        let w = widgets::TextEdit::singleline(&mut contact.tx_rst)
                        .horizontal_align(Align::Center)
                        .desired_width(f32::INFINITY)
                        .margin(egui::Margin::same(2.0))
                        .show(ui);

                        // The textedit lost focus, implying that the user wants to save the changes
                        if w.response.lost_focus() {
                            // Stop editing the column
                            self.editing_column = None;

                            // Update the contact
                            config.tasks.push((None, config.db_api.update_contact(contact.clone())));
                        };

                        // Focuses the textedit when a column is being edited
                        w.response.request_focus();

                    }
                    // This column isn't being edited, show a label
                    else {

                        // Show a label widget
                        widgets::Label::new(&contact.tx_rst)
                        .truncate(true)
                        .selectable(false)
                        .ui(ui);

                    }

                });
                // The TX RST column was double clicked; start editing the column
                if response.double_clicked() {
                    self.editing_column = Some((row_index, database::ContactTableColumn::TxRst));
                }

                // ===== RX RST COLUMN ===== //
                let (_rect, response) = row.col(|ui| {

                    // This column is currently being edited, show a textedit widget
                    if self.editing_column.is_some_and(|(idx, c)| idx == row_index && c.is_rx_rst()) {

                        // Show a textedit widget
                        let w = widgets::TextEdit::singleline(&mut contact.rx_rst)
                        .horizontal_align(Align::Center)
                        .desired_width(f32::INFINITY)
                        .margin(egui::Margin::same(2.0))
                        .show(ui);

                        // The textedit lost focus, implying that the user wants to save the changes
                        if w.response.lost_focus() {
                            // Stop editing the column
                            self.editing_column = None;

                            // Update the contact
                            config.tasks.push((None, config.db_api.update_contact(contact.clone())));
                        };

                        // Focuses the textedit when a column is being edited
                        w.response.request_focus();

                    }
                    // This column isn't being edited, show a label
                    else {

                        // Show a label widget
                        widgets::Label::new(&contact.rx_rst)
                        .truncate(true)
                        .selectable(false)
                        .ui(ui);

                    }

                });
                // The RX RST column was double clicked; start editing the column
                if response.double_clicked() {
                    self.editing_column = Some((row_index, database::ContactTableColumn::RxRst));
                }

                // ===== TX POWER COLUMN ===== //
                let (_rect, response) = row.col(|ui| {

                    // This column is currently being edited, show a dragvalue widget
                    if self.editing_column.is_some_and(|(idx, c)| idx == row_index && c.is_tx_pwr()) {

                        // Show a dragvalue widget
                        let w = widgets::DragValue::new(&mut contact.tx_power)
                        .custom_formatter(power_formatter)
                        .custom_parser(power_parser)
                        .update_while_editing(false)
                        .ui(ui);

                        // The dragvalue lost focus, implying that the user wants to save the changes
                        if w.lost_focus() {
                            // Stop editing the column
                            self.editing_column = None;

                            // Update the contact
                            config.tasks.push((None, config.db_api.update_contact(contact.clone())));
                        };

                        // Focuses the dragvalue when the column is being edited
                        w.request_focus();

                    }
                    // This column isn't being edited, show a label
                    else {

                        // Show a label widget
                        widgets::Label::new(power_formatter(contact.tx_power as f64, 0..=0))
                        .truncate(true)
                        .selectable(false)
                        .ui(ui);

                    }

                });
                // The TX Power column was double clicked; start editing the column
                if response.double_clicked() {
                    self.editing_column = Some((row_index, database::ContactTableColumn::TxPwr));
                }

                // ===== RX POWER COLUMN ===== //
                let (_rect, response) = row.col(|ui| {

                    // This column is currently being edited, show a dragvalue widget
                    if self.editing_column.is_some_and(|(idx, c)| idx == row_index && c.is_rx_pwr()) {

                        // Show a dragvalue widget
                        let w = widgets::DragValue::new(&mut contact.rx_power)
                        .custom_formatter(power_formatter)
                        .custom_parser(power_parser)
                        .update_while_editing(false)
                        .ui(ui);

                        // The dragvalue lost focus, implying that the user wants to save the changes
                        if w.lost_focus() {
                            // Stop editing the column
                            self.editing_column = None;

                            // Update the contact
                            config.tasks.push((None, config.db_api.update_contact(contact.clone())));
                        };

                        // Focuses the dragvalue when the column is being edited
                        w.request_focus();

                    }
                    // This column isn't being edited, show a label
                    else {

                        // Show a label widget
                        widgets::Label::new(power_formatter(contact.rx_power as f64, 0..=0))
                        .truncate(true)
                        .selectable(false)
                        .ui(ui);

                    }

                });
                // The RX Power column was double clicked; start editing the column
                if response.double_clicked() {
                    self.editing_column = Some((row_index, database::ContactTableColumn::RxPwr));
                }

                // ===== DATE COLUMN ===== //
                let (_rect, response) = row.col(|ui| {

                    // This column is currently being edited, show a textedit widget
                    if self.editing_column.is_some_and(|(idx, c)| idx == row_index && c.is_date()) {

                        // Show a textedit widget
                        let w = widgets::TextEdit::singleline(&mut self.date_str)
                        .clip_text(true)
                        .show(ui);

                        // The textedit lost focus, implying that the user wants to save the changes
                        if w.response.lost_focus() {
                            // Try to parse the date string into a date type
                            if let Ok(d) = NaiveDate::parse_from_str(&self.date_str, "%Y-%m-%d") {
                                contact.date = d;

                                // Update the contact
                                config.tasks.push((None, config.db_api.update_contact(contact.clone())));
                            }

                            // Stop editing the column
                            self.editing_column = None;
                        };

                        // Focuses the textedit when the column is being edited
                        w.response.request_focus();

                    }
                    // This column isn't being edited, show a label
                    else {

                        // Show a label widget
                        widgets::Label::new(format!("{}", contact.date.format("%Y-%m-%d")))
                        .truncate(true)
                        .selectable(false)
                        .ui(ui);

                    }

                });
                // The date column was double clicked; start editing the column
                if response.double_clicked() {
                    self.editing_column = Some((row_index, database::ContactTableColumn::Date));

                    // Initialize the date string with the current date of the contact
                    self.date_str = format!("{}", contact.date.format("%Y-%m-%d"));
                }

                // ===== TIME COLUMN ===== //
                let (_rect, response) = row.col(|ui| {

                    // This column is currently being edited, show a textedit widget
                    if self.editing_column.is_some_and(|(idx, c)| idx == row_index && c.is_time()) {

                        // Show a textedit widget
                        let w = widgets::TextEdit::singleline(&mut self.time_str)
                        .clip_text(true)
                        .show(ui);

                        // The textedit lost focus, implying that the user wants to save the changes
                        if w.response.lost_focus() {
                            // Try to parse the time string into a time type
                            if let Ok(t) = NaiveTime::parse_from_str(&self.time_str, "%H:%M:%S") {
                                contact.time = t;

                                // Update the contact
                                config.tasks.push((None, config.db_api.update_contact(contact.clone())));
                            }

                            // Stop editing the column
                            self.editing_column = None;
                        };

                        // Focuses the textedit when the column is being edited
                        w.response.request_focus();

                    }
                    // This column isn't being edited, show a label
                    else {

                        // Show a label widget
                        widgets::Label::new(format!("{}", contact.time.format("%H:%M:%S")))
                        .truncate(true)
                        .selectable(false)
                        .ui(ui);

                    }

                });
                // The time column was double clicked; start editing the column
                if response.double_clicked() {
                    self.editing_column = Some((row_index, database::ContactTableColumn::Time));

                    // Initialize the time string with the current time of the contact
                    self.time_str = format!("{}", contact.time.format("%H:%M:%S"));
                }

                // ===== NOTE COLUMN ===== //
                let (_rect, response) = row.col(|ui| {

                    // This column is currently being edited, show a textedit widget
                    if self.editing_column.is_some_and(|(idx, c)| idx == row_index && c.is_note()) {

                        // Show a textedit widget
                        let w = widgets::TextEdit::singleline(&mut contact.note)
                        .horizontal_align(Align::Center)
                        .desired_width(f32::INFINITY)
                        .margin(egui::Margin::same(2.0))
                        .show(ui);

                        // The textedit lost focus, implying that the user wants to save the changes
                        if w.response.lost_focus() {
                            // Stop editing the column
                            self.editing_column = None;

                            // Update the contact
                            config.tasks.push((None, config.db_api.update_contact(contact.clone())));
                        };

                        // Focuses the textedit when a column is being edited
                        w.response.request_focus();

                    }
                    // This column isn't being edited, show a label
                    else {

                        // Show a label widget
                        widgets::Label::new(&contact.note)
                        .truncate(true)
                        .selectable(false)
                        .ui(ui);

                    }

                });
                // The note column was double clicked; start editing the column
                if response.double_clicked() {
                    self.editing_column = Some((row_index, database::ContactTableColumn::Note));
                }

                // Get the response for the whole row
                let response = row.response();

                // A right-click context menu
                response.context_menu(|ui| {

                    // A button to lookup the callsign
                    if ui.button("Lookup callsign").clicked() {
                        
                        // Lookup the contact
                        config.tasks.push((None, config.cl_api.lookup_callsign(&contact.callsign)));

                        // Close the menu after the button was clicked
                        ui.close_menu();

                    }

                    // A button to delete the contact
                    if ui.button("Delete contact").clicked() {

                        // Delete the contact
                        config.tasks.push((None, config.db_api.delete_contact(contact.id.as_ref().unwrap().id.clone())));

                        // Close the menu after the button was clicked
                        ui.close_menu();

                    }

                });

            });
    
        });

    }

}
impl Default for ContactTableTab {
    fn default() -> Self {
        Self {
            id: generate_random_id(),
            contacts: Default::default(),
            sort_column: Default::default(),
            sort_dir: Default::default(),
            editing_column: Default::default(),
            date_str: Default::default(),
            time_str: Default::default()
        }
    }
}

/// The [TabVariant::CallsignLookup] tab
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

    fn process_event(&mut self, config: &mut GuiConfig, event: &types::Event) {
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
                let fut = config.cl_api.lookup_callsign(&self.callsign);
                config.tasks.push((None, fut));
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


/// Formats a f64 (in milliwatts) into a string (e.g. 5000 = `5.0 W`)
/// 
/// Used by egui drag value widgets
fn power_formatter(power: f64, _range: RangeInclusive<usize>) -> String {
    match power {
        p if p >= 1_000_000.0 => format!("{:.1} KW", power / 1_000_000.0),
        p if p >= 1_000.0 => format!("{:.1} W", power / 1_000.0),
        _ => format!("{power} mW")
    }
}

/// Parses an input string into a f64 in milliwatts
/// 
/// Used by egui drag value widgets
fn power_parser(input: &str) -> Option<f64> {
    // Convert the input to lowercase
    let input = input.to_lowercase();

    // Try to cast the input into a f64
    let power = match input.chars().take_while(|c| {c.is_ascii_digit() || c == &'.'}).collect::<String>().parse::<f64>() {
        Ok(p) => p,
        Err(err) => {
            warn!("Failed to parse power (input: '{input}'): {err}");
            return None;
        }
    };

    let result;
    // The user is entering kilowatts
    if input.contains('k') {
        result = power * 1_000_000.0;
    }
    // The user is entering milliwatts
    else if input.contains('m') {
        result = power;
    }
    // Assume the user was entering watts
    else {
        result = power * 1000.0;
    }

    Some(result)
}

/// Formats a f64 (in hz) into a string (e.g. 5000 = `5.000KHz`)
/// 
/// Used by egui drag value widgets
fn frequency_formatter(freq: f64, _range: RangeInclusive<usize>) -> String {
    match freq {
        f if f >= 1_000_000_000.0 => format!("{:.3} GHz", freq / 1_000_000_000.0),
        f if f >= 1_000_000.0 => format!("{:.3} MHz", freq / 1_000_000.0),
        f if f >= 1_000.0 => format!("{:.3} KHz", freq / 1_000.0),
        _ => format!("{freq:.1} Hz")
    }
}

/// Parses an input string into a f64 in hz
/// 
/// Used by egui drag value widgets
fn frequency_parser(input: &str) -> Option<f64> {
    // Convert the input to lowercase
    let input = input.to_lowercase();

    // Try to cast the input to a f64
    let frequency = match input.chars().take_while(|c| {c.is_ascii_digit() || c == &'.'}).collect::<String>().parse::<f64>() {
        Ok(f) => f,
        Err(err) => {
            warn!("Failed to parse frequency (input: '{input}'): {err}");
            return None;
        }
    };

    let result;
    // The user is entering GHz
    if input.contains('g') {
        result = frequency * 1_000_000_000.0;
    }
    // The user is entering MHz
    else if input.contains('m') {
        result = frequency * 1_000_000.0;
    }
    // The user is entering KHz
    else if input.contains('k') {
        result = frequency * 1_000.0;
    }
    // The user is entering Hz
    else if input.contains('h') {
        result = frequency;
    }
    // The input didn't match or they didn't provide any letters, so assume they are entering MHz
    else {
        result = frequency * 1_000_000.0;
    }

    Some(result)
}

/// Generates a random [egui::Id]
/// 
/// This is typically used to differentiate between different tabs
fn generate_random_id() -> Id {
    // Generate a new random ID
    Id::new(rand::thread_rng().next_u64())
}

/// A convenience function to add an async task to a task queue.
/// 
/// If `id` is provided, the task result will be bound to the GUI tab with that ID, and only that tab will receive the resulting value.
fn add_task_to_queue(queue: &mut Vec<(Option<Id>, SpawnedFuture)>, task: SpawnedFuture, id: Option<Id>) {
    queue.push((id, task));
}

/// A simple timer that sends a message (`true`) on the provided channel every [Duration] until the receiver is dropped
async fn channel_timer(tx: watch::Sender<bool>, duration: Duration) {
    while tx.send(true).is_ok() {
        tokio::time::sleep(duration).await;
    }
}
