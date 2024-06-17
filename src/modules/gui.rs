//
// The GUI code. This contains the immediate-mode code for the different types of GUI tabs.
//


use std::{ops::RangeInclusive, time::Duration};
use egui::{Id, Ui, WidgetText};
use log::warn;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use crate::GuiConfig;
use super::tabs::callsign_lookup::CallsignLookupTab;
use super::tabs::contact_logger::ContactLoggerTab;
use super::tabs::contacts::ContactTableTab;
use super::tabs::pskreporter::PSKReporterTab;
use super::tabs::settings::SettingsTab;
use super::types::{self, SpawnedFuture};


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
#[derive(Debug, Serialize, Deserialize, strum_macros::AsRefStr, strum_macros::EnumIter)]
pub enum TabVariant {
    /// The default welcome tab
    Welcome(Box<WelcomeTab>),
    /// A table that visualizes all of the logged contacts/QSOs
    ContactTable(Box<ContactTableTab>),
    /// A tab for logging contacts
    ContactLogger(Box<ContactLoggerTab>),
    /// A tab for looking up callsigns
    CallsignLookup(Box<CallsignLookupTab>),
    /// A tab for interfacing with PSKReporter
    PSKReporter(Box<PSKReporterTab>),
    /// A settings tab
    Settings(Box<SettingsTab>)
}
impl Tab for TabVariant {

    fn id(&self) -> Id {
        match self {
            TabVariant::Welcome(data) => data.id(),
            TabVariant::ContactTable(data) => data.id(),
            TabVariant::ContactLogger(data) => data.id(),
            TabVariant::CallsignLookup(data) => data.id(),
            TabVariant::PSKReporter(data) => data.id(),
            TabVariant::Settings(data) => data.id(),
        }
    }

    fn scroll_bars(&self) -> [bool; 2] {
        match self {
            TabVariant::Welcome(data) => data.scroll_bars(),
            TabVariant::ContactTable(data) => data.scroll_bars(),
            TabVariant::ContactLogger(data) => data.scroll_bars(),
            TabVariant::CallsignLookup(data) => data.scroll_bars(),
            TabVariant::PSKReporter(data) => data.scroll_bars(),
            TabVariant::Settings(data) => data.scroll_bars(),
        }
    }

    fn title(&mut self) -> WidgetText {
        match self {
            TabVariant::Welcome(data) => data.title(),
            TabVariant::ContactTable(data) => data.title(),
            TabVariant::ContactLogger(data) => data.title(),
            TabVariant::CallsignLookup(data) => data.title(),
            TabVariant::PSKReporter(data) => data.title(),
            TabVariant::Settings(data) => data.title(),
        }
    }

    fn init(&mut self, config: &mut GuiConfig) {
        match self {
            TabVariant::Welcome(data) => data.init(config),
            TabVariant::ContactTable(data) => data.init(config),
            TabVariant::ContactLogger(data) => data.init(config),
            TabVariant::CallsignLookup(data) => data.init(config),
            TabVariant::PSKReporter(data) => data.init(config),
            TabVariant::Settings(data) => data.init(config),
        }
    }

    fn process_event(&mut self, config: &mut GuiConfig, event: &types::Event) {
        match self {
            TabVariant::Welcome(data) => data.process_event(config, event),
            TabVariant::ContactTable(data) => data.process_event(config, event),
            TabVariant::ContactLogger(data) => data.process_event(config, event),
            TabVariant::CallsignLookup(data) => data.process_event(config, event),
            TabVariant::PSKReporter(data) => data.process_event(config, event),
            TabVariant::Settings(data) => data.process_event(config, event),
        }
    }

    fn ui(&mut self, config: &mut GuiConfig, ui: &mut Ui) {
        match self {
            TabVariant::Welcome(data) => data.ui(config, ui),
            TabVariant::ContactTable(data) => data.ui(config, ui),
            TabVariant::ContactLogger(data) => data.ui(config, ui),
            TabVariant::CallsignLookup(data) => data.ui(config, ui),
            TabVariant::PSKReporter(data) => data.ui(config, ui),
            TabVariant::Settings(data) => data.ui(config, ui),
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

/// Formats a f64 (in milliwatts) into a string (e.g. 5000 = `5.0 W`)
/// 
/// Used by egui drag value widgets
pub fn power_formatter(power: f64, _range: RangeInclusive<usize>) -> String {
    match power {
        p if p >= 1_000_000.0 => format!("{:.1} KW", power / 1_000_000.0),
        p if p >= 1_000.0 => format!("{:.1} W", power / 1_000.0),
        _ => format!("{power} mW")
    }
}

/// Parses an input string into a f64 in milliwatts
/// 
/// Used by egui drag value widgets
pub fn power_parser(input: &str) -> Option<f64> {
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
pub fn frequency_formatter(freq: f64, _range: RangeInclusive<usize>) -> String {
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
pub fn frequency_parser(input: &str) -> Option<f64> {
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
pub fn generate_random_id() -> Id {
    // Generate a new random ID
    Id::new(rand::thread_rng().next_u64())
}

/// A convenience function to add an async task to a task queue.
/// 
/// If `id` is provided, the task result will be bound to the GUI tab with that ID, and only that tab will receive the resulting value.
pub fn add_task_to_queue(queue: &mut Vec<(Option<Id>, SpawnedFuture)>, task: SpawnedFuture, id: Option<Id>) {
    queue.push((id, task));
}

/// A simple timer that sends a message (`true`) on the provided channel every [Duration] until the receiver is dropped
async fn channel_timer(tx: watch::Sender<bool>, duration: Duration) {
    while tx.send(true).is_ok() {
        tokio::time::sleep(duration).await;
    }
}
