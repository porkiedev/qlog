use chrono::{NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};
use egui::{widgets, Align, CursorIcon, Id, Layout, RichText, Ui, Widget, WidgetText};
use log::{debug, trace};
use strum::IntoEnumIterator;
use crate::modules::gui::{add_task_to_queue, frequency_formatter, frequency_parser, generate_random_id, power_formatter, power_parser, Tab};
use crate::{types, GuiConfig};
use crate::database;


/// The contact table tab
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

    // Load contacts from database on initialization
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
                            add_task_to_queue(
                                &mut config.tasks,
                                config.db_api.update_contact(contact.clone()),
                                None
                            );
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
                            add_task_to_queue(
                                &mut config.tasks,
                                config.db_api.update_contact(contact.clone()),
                                None
                            );
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
                            add_task_to_queue(
                                &mut config.tasks,
                                config.db_api.update_contact(contact.clone()),
                                None
                            );
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
                            add_task_to_queue(
                                &mut config.tasks,
                                config.db_api.update_contact(contact.clone()),
                                None
                            );
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
                            add_task_to_queue(
                                &mut config.tasks,
                                config.db_api.update_contact(contact.clone()),
                                None
                            );
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
                                add_task_to_queue(
                                    &mut config.tasks,
                                    config.db_api.update_contact(contact.clone()),
                                    None
                                );
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
                                add_task_to_queue(
                                    &mut config.tasks,
                                    config.db_api.update_contact(contact.clone()),
                                    None
                                );
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
                            add_task_to_queue(
                                &mut config.tasks,
                                config.db_api.update_contact(contact.clone()),
                                None
                            );
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
                        add_task_to_queue(
                            &mut config.tasks,
                            config.cl_api.lookup_callsign(&contact.callsign),
                            None
                        );

                        // Close the menu after the button was clicked
                        ui.close_menu();

                    }

                    // A button to delete the contact
                    if ui.button("Delete contact").clicked() {

                        // Delete the contact
                        add_task_to_queue(
                            &mut config.tasks,
                            config.db_api.delete_contact(contact.id.as_ref().unwrap().id.clone()),
                            None
                        );

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
