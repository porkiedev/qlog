#![allow(unused)]

mod modules;

use std::{env::current_exe, fs, io::ErrorKind, sync::Arc};
use eframe::App;
use egui::{widgets, Id, RichText, Ui, Widget, WidgetText};
use egui_dock::{DockArea, DockState, TabViewer};
use log::{debug, error, info, trace};
use serde::{Deserialize, Serialize};
use modules::{callsign_lookup, database, gui::TabVariant, types};
use strum::{EnumCount, IntoEnumIterator};
use tokio::runtime::Runtime;
use modules::gui::Tab;


// Use mimalloc as the memory allocator
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;


fn main() {
    // Initialize logger
    env_logger::Builder::new().filter(Some(module_path!()), log::LevelFilter::Debug).init();

    // Start GUI
    let _ = eframe::run_native(
        "QLog",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Box::<Gui>::default())
    );
}

// The qlog GUI
struct Gui {
    /// The dock state for the tabs
    dock_state: DockState<TabVariant>,
    /// The tab viewer
    tab_viewer: GuiTabViewer
}
impl Default for Gui {
    fn default() -> Self {

        let (dock_state, config) = Self::get_configs();

        Self {
            dock_state,
            tab_viewer: GuiTabViewer { config }
        }
    }
}
impl App for Gui {
    // Save tab state
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        trace!("Saving application state...");

        // Get the parent directory of the exe file
        let exe_path = current_exe().expect("Failed to get path of exe file");
        let exe_dir = exe_path.parent().expect("Failed to get parent directory of exe file");
        
        // Save the dockstate config
        fs::write(exe_dir.join(Self::CONFIG_TABS_FILE), serde_json::to_vec_pretty(&self.dock_state).unwrap())
        .expect("Failed to save dockstate config");

        // Save the gui config
        fs::write(exe_dir.join(Self::CONFIG_GUI_FILE), serde_json::to_vec_pretty(&self.tab_viewer.config).unwrap())
        .expect("Failed to save gui config");

        trace!("Saved application state");
    }

    // TODO: Remove this after development. This is false so I can test the defaults during development
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Get a mutable reference to the gui config
        let config = &mut self.tab_viewer.config;

        // Render the top/menu bar
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {

                // A combobox to add a tab to the GUI
                egui::ComboBox::from_id_source("add_tab_combobox")
                .selected_text("Add Tab")
                .show_ui(ui, |ui| {
                    for tab_idx in 0..5_u32 {
                        
                        // Get the text for each tab variant
                        let text = match tab_idx {
                            0 => "Home",
                            1 => "Contacts",
                            2 => "Contact Logger",
                            3 => "Callsign Lookup",
                            4.. => "PSKReporter"
                        };

                        // Render a selectable label for each tab variant, adding the tab if it was clicked
                        if ui.selectable_label(false, text).clicked {

                            // Create the tab
                            let mut t = match tab_idx {
                                0 => TabVariant::Welcome(Default::default()),
                                1 => TabVariant::ContactTable(Default::default()),
                                2 => TabVariant::ContactLogger(Default::default()),
                                3 => TabVariant::CallsignLookup(Default::default()),
                                4.. => TabVariant::PSKReporter(Default::default())
                            };

                            // Initialize the tab
                            t.init(config);

                            // Push the new tab to the GUI
                            self.dock_state.push_to_focused_leaf(t);

                        }

                    }
                });

                // Check the async task queue and send out and necessary events
                // Note that tasks are processed sequentially, so all tasks should have timeouts on them,
                // because one long-running task will delay all of the other tasks
                while let Some((task_tab_id, task)) = config.tasks.first_mut() {

                    // The task is finished
                    if task.is_finished() {
                        match config.runtime.block_on(task).unwrap() {
                            Ok(event) => {

                                // The task is bound to a specific tab
                                if let Some(task_tab_id) = *task_tab_id {

                                    // Filter for the tab with a matching ID
                                    if let Some((_, tab)) = self.dock_state.iter_all_tabs_mut()
                                    .find(|(_, tab)| tab.id() == task_tab_id) {
                                        tab.process_event(config, &event);
                                    }

                                }
                                // The task is global
                                else {

                                    // Send the event to every tab
                                    for (_, tab) in self.dock_state.iter_all_tabs_mut() {
                                        tab.process_event(config, &event);
                                    }

                                }

                            },
                            Err(err) => {
                                config.notifications.push(types::Notification::Error(err.to_string()));
                                config.notification_read = false;
                            }
                        }

                        // Since the task is complete, remove it from the queue
                        config.tasks.remove(0);
                    }
                    // The task is not finished yet so wait to check next frame
                    else {
                        widgets::Spinner::new().ui(ui);
                        break;
                    }

                }

                // Limit the number of notifications to 32
                config.notifications.shrink_to(32);

                // A label to show the latest notification (if one exists)
                if let Some(notification) = config.notifications.last() {

                    // The notification hasn't been marked as read yet
                    if !config.notification_read {

                        // Get the visual of the GUI
                        let visuals = &ui.style().visuals;

                        // Create the text with different colors depending on the notification type
                        let text = match notification {
                            types::Notification::Info(t) => RichText::new(t),
                            types::Notification::Warning(t) => RichText::new(t).color(visuals.warn_fg_color),
                            types::Notification::Error(t) => RichText::new(t).color(visuals.error_fg_color)
                        };

                        // Render the text, from right to left
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {

                            // A checkmark button to mark the notification as read
                            if ui.button("\u{2714}").on_hover_text("Mark notification as read").clicked() {
                                config.notification_read = true;
                            }

                            // A label to show the notification
                            egui::Label::new(text)
                            .truncate(true)
                            .ui(ui);

                        });

                    }
                }
            
            });
        });

        // Iterate through each tab and process tasks before rendering the tab
        for wanted_tab_index in 0..self.dock_state.iter_all_tabs().count() {

            // Initialize variables for the tab that we want, and a vec containing all of the other tabs
            let mut wanted_tab = None;
            let mut other_tabs = Vec::new();

            // Iterate through each tab and populate the 
            for (tab_index, (_, tab)) in self.dock_state.iter_all_tabs_mut().enumerate() {
                // This is the tab we want, so put it in the wanted_tab var
                if tab_index == wanted_tab_index {
                    wanted_tab = Some(tab);
                }
                // This is not the tab we want, so put it in the other_tabs vec
                else {
                    other_tabs.push(tab);
                }
            }
    
            // If we found the tab we want (we always should), process tasks
            if let Some(tab) = wanted_tab {
                tab.process_tasks(config, other_tabs);
            }

        }

        // Render the dockable area
        DockArea::new(&mut self.dock_state)
        .show(ctx, &mut self.tab_viewer);

    }
}
impl Gui {
    const CONFIG_GUI_FILE: &'static str = "config-gui.json";
    const CONFIG_TABS_FILE: &'static str = "config-tabs.json";

    /// Returns the saved gui dockstate and config, creating a new one if it doesn't exist
    fn get_configs() -> (DockState<TabVariant>, GuiConfig) {
        trace!("Initializing application state...");

        // Get the parent directory of the exe file
        let exe_path = current_exe().expect("Failed to get path of exe file");
        let exe_dir = exe_path.parent().expect("Failed to get parent directory of exe file");

        // Get the GUI dockstate (or create a new one if it doesn't exist)
        let mut dockstate = match fs::read(exe_dir.join(Self::CONFIG_TABS_FILE)) {
            Ok(data) => serde_json::from_slice::<DockState<TabVariant>>(&data).expect("Failed to parse dockstate config"),
            Err(err) => {

                // If the dockstate config doesn't exist, use the default.
                // Otherwise, we failed for some other reason, and this deserves a panic.
                if err.kind() == ErrorKind::NotFound {
                    debug!("No dockstate config was found, using the default instead");
                    // Return a new dockstate with just a home tab
                    DockState::new(vec![TabVariant::Welcome(Default::default())])
                } else {
                    panic!("Failed to access dockstate config file: {err}")
                }

            }
        };

        // Get the GUI config (or create a new config if one doesn't exist)
        let mut gui_config = match fs::read(exe_dir.join(Self::CONFIG_GUI_FILE)) {
            Ok(data) => serde_json::from_slice::<GuiConfig>(&data).expect("Failed to parse gui config"),
            Err(err) => {

                if err.kind() == ErrorKind::NotFound {
                    debug!("No gui config was found, using the default instead");
                    // Return the default GuiConfig
                    GuiConfig::default()
                } else {
                    panic!("Failed to access gui config file: {err}")
                }

            }
        };

        // Initialize every tab
        for (_s, t) in dockstate.iter_all_tabs_mut() {
            t.init(&mut gui_config);
        }

        info!("Initialized application state");

        (dockstate, gui_config)
    }
}


/// The GUI tab viewer. This is responsible for rendering the layout of each [TabVariant]
pub struct GuiTabViewer {
    /// The GUI config
    config: GuiConfig
}
impl TabViewer for GuiTabViewer {
    type Tab = TabVariant;

    // Returns the ID of the tab
    //
    // For non-interactive tabs, this is a static value, but each interactive tab must have a unique ID otherwise stuff gets weird
    fn id(&mut self, tab: &mut Self::Tab) -> Id {
        tab.id()
    }

    fn scroll_bars(&self, tab: &Self::Tab) -> [bool; 2] {
        tab.scroll_bars()
    }

    // Renders the title for the tab
    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        tab.title()
    }

    // Renders the UI for the tab
    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        tab.ui(&mut self.config, ui)
    }
}



/// The GUI config
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
struct GuiConfig {
    /// The tokio async runtime
    #[serde(skip)]
    runtime: Runtime,
    /// The database API
    #[serde(skip)]
    db_api: database::DatabaseInterface,
    /// The callsign lookup API
    #[serde(skip)]
    cl_api: callsign_lookup::CallsignLookup,
    /// Notifications. This could be status, warning, or error messages that need to be shown at the root level of the GUI
    #[serde(skip)]
    notifications: Vec<types::Notification>,
    /// Has the latest notification been read? If true, the latest notification is hidden.
    #[serde(skip)]
    notification_read: bool,
    /// Async tasks. If an ID is provided, the event will only be sent to the tab with that ID, otherwise the update is global.
    /// This enforces synchronization between tabs.
    #[serde(skip)]
    pub tasks: Vec<(Option<Id>, types::SpawnedFuture)>,
    /// The selected index of the 'add tab' combobox in the top/menu bar
    #[serde(skip)]
    add_tab_idx: usize
}
impl Default for GuiConfig {
    fn default() -> Self {
        
        let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("Failed to build tokio runtime");
        let db = database::DatabaseInterface::new(runtime.handle().clone(), None, None).unwrap();
        // let db = database::DatabaseInterface::new(runtime.handle().clone(), Some("ws://127.0.0.1:8000".into()), None).unwrap();
        let cl_api = callsign_lookup::CallsignLookup::new(runtime.handle().clone(), None);

        Self {
            runtime,
            db_api: db,
            cl_api,
            notifications: Default::default(),
            notification_read: Default::default(),
            tasks: Default::default(),
            add_tab_idx: Default::default()
        }
    }
}
