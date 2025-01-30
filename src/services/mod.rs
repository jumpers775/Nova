pub mod error;
pub mod local;
pub mod manager;
pub mod models;
pub mod traits;

pub use error::ServiceError;
pub use local::LocalMusicProvider;
pub use manager::ServiceManager;
pub use models::{Album, Artist, PlayableItem, Track};
pub use traits::MusicProvider;
