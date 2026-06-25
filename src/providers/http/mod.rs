mod direct_adapter;
mod http_client;
pub mod http_status;
mod webscraper_adapter;

pub use direct_adapter::DirectAdapter;
pub use http_client::HttpClient;
pub use webscraper_adapter::WebScraperAdapter;
