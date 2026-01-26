use crate::config::StorageState;

pub struct WebData {
    pub image: StorageState,
    pub paste: StorageState,
    /// The link prefix to send in replies to users, e.g. "https://images.ghetty.space"
    pub link_prefix: String,
}
