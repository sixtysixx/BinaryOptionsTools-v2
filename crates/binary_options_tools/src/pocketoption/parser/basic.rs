use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::pocketoption::{
    error::PocketResult, types::update::float_time, utils::basic::get_index,
};

/// Represents an update stream entry with asset details and timestamp.
#[derive(Debug, Deserialize)]
#[allow(unused)]
pub struct UpdateStream {
    active: String,
    #[serde(with = "float_time")]
    time: DateTime<Utc>,
    value: f64,
}

/// Enumerates possible asset types in lowercase format.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum AssetType {
    Stock,
    Currency,
    Commodity,
    Cryptocurrency,
    Index,
}

/// Struct for loading historical data periods with asset info and pagination details.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoadHistoryPeriod {
    pub asset: String,
    pub period: i64,
    pub time: i64,
    pub index: u64,
    pub offset: i64,
}

impl LoadHistoryPeriod {
    /// Creates a new LoadHistoryPeriod instance.
    ///
    /// # Arguments
    ///
    /// * `asset` - The asset symbol as a string-like type.
    /// * `time` - The time value as i64.
    /// * `period` - The period value as i64.
    /// * `offset` - The offset value as i64.
    ///
    /// # Returns
    ///
    /// A PocketResult containing the new LoadHistoryPeriod instance or an error.
    pub fn new(asset: impl ToString, time: i64, period: i64, offset: i64) -> PocketResult<Self> {
        Ok(LoadHistoryPeriod {
            asset: asset.to_string(),
            period,
            time,
            index: get_index()?,
            offset,
        })
    }
}
