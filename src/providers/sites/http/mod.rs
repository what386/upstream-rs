mod direct_download;
mod http_client;
mod web_scraper;

pub use direct_download::DirectAdapter;
pub use http_client::{ConditionalDocumentResult, HttpAssetInfo, HttpClient};
pub use web_scraper::WebScraperAdapter;
