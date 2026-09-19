mod direct_download;
mod http_client;
mod web_scraper;

pub use direct_download::DirectAdapter;
pub use web_scraper::WebScraperAdapter;
pub use http_client::{ConditionalDocumentResult, HttpAssetInfo, HttpClient};
