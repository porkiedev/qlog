//
// Contains several datatypes used by the application and/or database
//


use anyhow::Result;
use serde::{Deserialize, Serialize};
use chrono::{NaiveDate, NaiveTime};
use strum_macros::{Display, EnumIter};
use tokio::task::JoinHandle;


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

/// An event or request that is sent to every tab in the GUI.
/// 
/// This is typically used to synchronize multiple tabs. For example, if you insert a contact into the database,
/// the contact table tab should also be made aware of the change so it can update itself.
#[derive(Debug)]
pub enum Event {
    /// Refresh the contacts table
    RefreshContacts,
    /// Search for a callsign
    LookupCallsign(String),
}

/// The distance unit used by the GUI
#[derive(Debug, Serialize, Deserialize)]
pub enum DistanceUnit {
    Kilometers,
    Miles
}
impl DistanceUnit {
    /// Converts a distance in meters to the unit of `self`
    pub fn to_unit_from_meters(&self, meters: f64) -> f64 {
        match self {
            Self::Kilometers => meters / 1000.0,
            Self::Miles => meters * 0.0006213712
        }
    }
}

/// Converts a value from one range into a value in another range
/// 
/// Example: convert_range_u64(100, [0, 100], [0, 1000]) would return 100
pub fn convert_range_u64(val: u64, r1: [u64; 2], r2: [u64; 2]) -> u64 {
    (val - r1[0])
        * (r2[1] - r2[0])
        / (r1[1] - r1[0])
        + r2[0]
}

/// A module to serialize and deserialize `Arc<RwLock<T>>` types
/// 
/// NOTE: This performs blocking reads so you are responsible for ensuring that no deadlocks occur.
pub mod arc_rwlock_serde {
    use serde::{Deserialize, Serialize, Deserializer, Serializer};
    use tokio::sync::RwLock;
    use std::sync::Arc;

    pub fn serialize<S, T>(val: &Arc<RwLock<T>>, s: S) -> Result<S::Ok, S::Error>
    where S: Serializer, T: Serialize {
        T::serialize(&*val.blocking_read(), s)
    }
    
    pub fn deserialize<'de, D, T>(d: D) -> Result<Arc<RwLock<T>>, D::Error>
    where D: Deserializer<'de>, T: Deserialize<'de> {
        Ok(Arc::new(RwLock::new(T::deserialize(d)?)))
    }

}
