pub mod direct_adapter;
pub mod http_client;
pub mod webscraper_adapter;

pub use direct_adapter::DirectAdapter;
pub use http_client::HttpClient;
pub use webscraper_adapter::WebScraperAdapter;
