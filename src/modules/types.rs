//
// Contains several datatypes used by the application and/or database
//


use anyhow::Result;
use serde::{Deserialize, Serialize};
use chrono::{NaiveDate, NaiveTime};
use strum_macros::{Display, EnumIter};
use tokio::task::JoinHandle;
use super::callsign_lookup::CallsignInformation;


/// A radio contact
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Contact {
    /// The record ID from the database
    pub id: Option<surrealdb::sql::Thing>,
    /// The callsign of the receiving station during the contact
    pub callsign: String,
    /// The date that the contact began, in UTC
    pub date: NaiveDate,
    /// The time that the contact began, in UTC
    pub time: NaiveTime,
    /// The duration of the contact, in seconds
    pub duration: u64,
    /// The frequency at which the contact took place, in Hz
    pub frequency: u64,
    /// The mode used during the contact
    pub mode: Mode,
    /// The power used by the transmitting station, in milliwatts
    pub tx_power: u64,
    /// The power used by the receiving station, in milliwatts
    pub rx_power: u64,
    /// The signal report of the receiving station, as observed by the transmitting station
    pub tx_rst: String,
    /// The signal report of the transmitting station, as observed by the receiving station
    pub rx_rst: String,
    /// A note
    pub note: String
}


/// A mode or modulation type used in amateur radio
#[derive(Debug, Default, Serialize, Deserialize, EnumIter, Display, PartialEq, Eq, Clone, strum_macros::EnumIs)]
#[allow(clippy::upper_case_acronyms)]
pub enum Mode {
    #[default]
    /// Single sideband voice
    SSB,
    /// Carrier Wave (Morse Code)
    CW,
    /// Amplitude Modulation
    AM,
    /// Frequency Modulation
    FM,
    /// Phase Shift Keying at 31.25baud
    PSK31,
    /// Radio teletype
    RTTY,
    /// A form of 8FSK, optimized for the HF bands and only allows for exchanging signal reports
    FT8,
    /// Inspired by FT8, with the ability to exchange messages
    JS8CALL,
    /// A form of MFSK, optimized for the HF bands, and typically occupies a wide bandwidth
    OLIVIA,
    /// A form of MFSK, optimized for the HF bands, and typically occupies a narrow bandwidth when compared to other MFSK modes
    DOMINOEX,
    /// A different type of modulation, described as a string
    #[strum(to_string = "Other")]
    OTHER(String)
}

/// Notifications that should be shown to the user through the GUI.
/// 
/// This is useful for displaying general status, warnings, and errors to the user via the GUI.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum Notification {
    Info(String),
    Warning(String),
    Error(String),
}

/// An event that is made visible to every tab in the GUI.
/// 
/// This is typically used to synchronize multiple tabs. For example, if you insert a contact into the database,
/// the contact table tab should also be made aware of the change so it can update itself without querying the database again.
#[derive(Debug)]
pub enum Event {
    /// A contact was added to the database
    AddedContact(Box<Contact>),
    /// Contacts were fetched from the database
    GotContacts(Vec<Contact>),
    /// A contact in the database was updated
    UpdatedContact(Box<Contact>),
    /// A contact was deleted from the database
    DeletedContact(Box<Contact>),
    /// Multiple contacts were deleted from the database
    DeletedContacts(Vec<Contact>),
    /// A contact was looked up
    CallsignLookedUp(Box<CallsignInformation>)
}

/// The result of a task spawned on the tokio runtime.
/// 
/// The spawned future should be pushed onto the GUI task queue.
/// The GUI will check for completed futures serially and send update events out to the corresponding tabs.
pub type SpawnedFuture = JoinHandle<Result<Event>>;
