//! Pure calculations over activity data. No UI and no database access here,
//! except in `metrics`, which stores the results.

pub mod goals;
pub mod hr;
pub mod insights;
pub mod load;
pub mod metrics;
pub mod splits;
pub mod stream;
pub mod trends;
