use chrono::{NaiveDate, NaiveTime};
use poll_promise::Promise;
use serde::{Deserialize, Serialize};
use egui::{widgets, Align, CursorIcon, Id, Layout, RichText, Ui, Widget, WidgetText};
use log::{debug, error, trace};
use strum::IntoEnumIterator;
use anyhow::Result;
use crate::modules::gui::{self, frequency_formatter, frequency_parser, generate_random_id, power_formatter, power_parser, Tab};
use crate::{types, GuiConfig, RT};
use crate::database;


/// The contact table tab
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct ContactTableTab {
    /// The egui ID
    id: Id,
    /// The contacts that are shown in the contact table
    #[serde(skip)]
    contacts: Vec<types::Contact>,
    /// The index of the first row in the contacts vec. This is critical for good performance.
    /// We only want to query the database for the contacts that are visible in the table, so we use this offset to keep track of where we are.
    #[serde(skip)]
    contacts_offset: usize,
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
    time_str: String,
    /// The duration string used when editing a duration column on a contact
    #[serde(skip)]
    duration_str: String,
    /// The index of the last visible row when the database was last queried
    last_row_idx: usize,
    /// The task that is currently running to query the database
    #[serde(skip)]
    query_task: Option<(usize, Promise<Result<Vec<types::Contact>>>)>,
    /// The task that is currently running to update a row in the database
    #[serde(skip)]
    update_task: Option<Promise<Result<types::Contact>>>,
    /// The task that is currently running to delete a row in the database
    #[serde(skip)]
    delete_task: Option<Promise<Result<types::Contact>>>,
    /// A flag to indicate if we should query the database again.
    /// This is used instead of a queue so we only query the database once at a time, but we can still ensure we have the latest data.
    #[serde(skip)]
    should_query: bool
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

    fn process_event(&mut self, config: &mut GuiConfig, event: &types::Event) {
        // Refresh the contacts table if the event is a refresh contacts event
        if let types::Event::RefreshContacts = event {
            self.should_query = true;
        };
    }

    fn ui(&mut self, config: &mut GuiConfig, ui: &mut Ui) {
        use egui_extras::Column;
        
        // Process any pending delete task
        if let Some(contact) = self.delete_task.take_if(|t| t.ready().is_some()) {
            let contact = contact.block_and_take();

            // Since we deleted the contact, we should query the database again
            self.should_query = true;
        }

        // Process any pending update task
        if let Some(contact) = self.update_task.take_if(|t| t.ready().is_some()) {
            let contact = contact.block_and_take();

            // Since we updated the contact, we should query the database again
            self.should_query = true;
        }

        // If we finished querying the database, process the response
        if let Some((offset, promise)) = self.query_task.take_if(|(_, t)| t.ready().is_some()) {
            // Take the query result
            match promise.block_and_take() {
                Ok(contacts) => {
                    // Update the contacts vec
                    self.contacts = contacts;
                    // Update the index offset
                    self.contacts_offset = offset;
                },
                Err(err) => error!("Failed to query the database for contacts: {err}")
            }
        }

        // Enforce a minimum width for the tab. The tab will automatically add horizontal scrollbars if the window is too small.
        // This stops us from making the table unreasonably small.
        ui.set_min_width(300.0);

        // Get the total number of contacts in the database
        let total_rows = config.db_api.get_contacts_metadata().unwrap().n_contacts;

        // The index of the first and last visible row
        let mut first_row_idx = None;
        let mut last_row_idx = 0;

        egui_extras::TableBuilder::new(ui)
        .columns(Column::initial(50.0).at_least(50.0), 1) // Callsign
        .columns(Column::initial(70.0).at_least(70.0), 1) // Frequency
        .columns(Column::initial(35.0).at_least(35.0), 1) // Mode
        .columns(Column::initial(40.0).at_least(40.0), 2) // TX and RX RST
        .columns(Column::initial(55.0).at_least(55.0), 2) // TX and RX Power
        .column(Column::initial(70.0).at_least(70.0)) // Date
        .column(Column::initial(50.0).at_least(50.0)) // Time
        .column(Column::initial(50.0).at_least(50.0)) // Duration
        .columns(Column::remainder().at_least(50.0).clip(true), 1) // Note
        .cell_layout(Layout::top_down(Align::Center))
        .resizable(true)
        .striped(true)
        .min_scrolled_height(20.0)
        .sense(egui::Sense::click())
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
                    self.should_query = true;

                }

            }

        }).body(|body| {

            let mut should_update_row = None;

            // Create a new row for each contact
            body.rows(20.0, total_rows, |mut row| {

                // Get the first and last row index
                let row_index = row.index();

                // Update the first and last row index
                if first_row_idx.is_none() {
                    first_row_idx = Some(row_index);
                }
                last_row_idx = row_index;

                // Calculate the contact vec index relative to the offset
                let contacts_index = row_index.wrapping_sub(self.contacts_offset);

                // Get the contact that this row belongs to
                let contact = match self.contacts.get_mut(contacts_index) {
                    Some(c) => c,
                    None => {
                        // Show "Loading..." for the callsign column
                        row.col(|ui| {
                            ui.label("Loading...");
                        });

                        // Show nothing for the remaining columns. We still call row.col() so you can still scroll with your mouse anywhere in the table.
                        for _ in 0..9 {
                            row.col(|ui| {});
                        }

                        return;
                    }
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
                            should_update_row = Some(contact.clone());
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
                            should_update_row = Some(contact.clone());
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
                                    should_update_row = Some(contact.clone());
                                };
                            }

                        });

                        // Save if a combobox option was clicked
                        if saved && !contact.mode.is_other() {
                            // Stop editing the column
                            self.editing_column = None;

                            // Update the contact
                            should_update_row = Some(contact.clone());
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
                            should_update_row = Some(contact.clone());
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
                            should_update_row = Some(contact.clone());
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
                            should_update_row = Some(contact.clone());
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
                            should_update_row = Some(contact.clone());
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
                                should_update_row = Some(contact.clone());
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
                                should_update_row = Some(contact.clone());
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

                // ===== DURATION COLUMN ===== //
                let (_rect, response) = row.col(|ui| {

                    // This column is currently being edited, show a dragvalue widget
                    if self.editing_column.is_some_and(|(idx, c)| idx == row_index && c.is_duration()) {

                        // Show a textedit widget
                        let w = widgets::TextEdit::singleline(&mut self.duration_str)
                        .clip_text(true)
                        .show(ui);

                        // The textedit lost focus, implying that the user wants to save the changes
                        if w.response.lost_focus() {

                            // Try to parse the duration string into a duration in seconds type
                            if let Some(d) = gui::duration_parser(&self.duration_str) {
                                // Only update the duration if the user tried to enter a valid duration
                                if !self.duration_str.is_empty() {

                                    contact.duration = d;

                                    // Update the contact
                                    should_update_row = Some(contact.clone());

                                }
                            }

                            // Stop editing the column
                            self.editing_column = None;

                        }

                        // Focuses the textedit when the column is being edited
                        w.response.request_focus();

                    }
                    // This column isn't being edited, show a label
                    else {

                        // Calculate the duration of the contact and format it as a pretty string
                        let st = contact.date.and_time(contact.time);
                        let et = st.checked_add_signed(chrono::TimeDelta::seconds(contact.duration as i64)).unwrap();
                        let dur = gui::seconds_formatter(et.signed_duration_since(st).num_seconds() as u64);

                        // Show a label widget
                        widgets::Label::new(dur)
                        .truncate(true)
                        .selectable(false)
                        .ui(ui);

                    }

                });
                if response.double_clicked() {
                    self.editing_column = Some((row_index, database::ContactTableColumn::Duration));

                    // Clear the duration string
                    self.duration_str.clear();
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
                            should_update_row = Some(contact.clone());
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
                    if ui.button("Lookup callsign").on_hover_text("You must have a callsign lookup tab open to see the result").clicked() {

                        // Lookup the contact
                        config.events.push((None, types::Event::LookupCallsign(contact.callsign.clone())));

                        // Close the menu after the button was clicked
                        ui.close_menu();

                    }

                    // A button to delete the contact
                    let response = ui.add_enabled(self.delete_task.is_none(), widgets::Button::new("Delete contact"));
                    if response.clicked() {
                        // Delete the contact
                        self.delete_task = Some(config.db_api.delete_contact_promise(contact.id.as_ref().unwrap().id.clone()));

                        // Close the menu after the button was clicked
                        ui.close_menu();
                    }

                });

            });

            // Update the contact if the user modified a column
            if let Some(contact) = should_update_row {
                self.update_task = Some(config.db_api.update_contact_promise(contact));
            }

        });

        // Should we query the database? This is set to true if the user has scrolled or resized the table
        self.should_query |= self.last_row_idx != last_row_idx;

        // If we should query the database and we aren't already querying it, do so
        if self.should_query && self.query_task.is_none() {
            let _eg = RT.enter();
            // Get the number of visible rows
            let n_visible_rows = last_row_idx.saturating_sub(first_row_idx.unwrap_or_default()) + 1;

            // Query the database
            self.query_task = Some((
                first_row_idx.unwrap_or_default(),
                config.db_api.get_contacts_promise(
                first_row_idx.unwrap_or_default(),
                Some(n_visible_rows),
                self.sort_column,
                Some(self.sort_dir)
            )));

            // Update the last row index
            self.last_row_idx = last_row_idx;

            // Update the should query flag
            self.should_query = false;
        }

    }

}
impl Default for ContactTableTab {
    fn default() -> Self {
        Self {
            id: generate_random_id(),
            contacts: Default::default(),
            contacts_offset: Default::default(),
            sort_column: Default::default(),
            sort_dir: Default::default(),
            editing_column: Default::default(),
            date_str: Default::default(),
            time_str: Default::default(),
            duration_str: Default::default(),
            last_row_idx: Default::default(),
            query_task: Default::default(),
            update_task: Default::default(),
            delete_task: Default::default(),
            should_query: true
        }
    }
}
impl std::fmt::Debug for ContactTableTab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContactTableTab")
        .field("id", &self.id)
        .field("contacts", &self.contacts)
        .field("sort_column", &self.sort_column)
        .field("sort_dir", &self.sort_dir)
        .field("editing_column", &self.editing_column)
        .field("date_str", &self.date_str)
        .field("time_str", &self.time_str)
        .field("last_last_row_idx", &self.last_row_idx)
        .finish()
    }
}
