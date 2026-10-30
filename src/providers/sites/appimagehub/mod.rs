//! AppImageHub's shared openDesktop OCS API. Listing IDs are stable; download URLs expire.
mod appimagehub_adapter;
mod appimagehub_client;
mod appimagehub_dtos;

pub use appimagehub_adapter::AppImageHubAdapter;
pub use appimagehub_client::{AppImageHubClient, content_id};
