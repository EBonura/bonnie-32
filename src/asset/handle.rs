use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetHandle(pub Uuid);

impl AssetHandle {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn null() -> Self {
        Self(Uuid::nil())
    }

    pub fn is_null(&self) -> bool {
        self.0.is_nil()
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl std::fmt::Debug for AssetHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AssetHandle({})", &self.0.to_string()[..8])
    }
}

impl std::fmt::Display for AssetHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for AssetHandle {
    fn default() -> Self {
        Self::null()
    }
}
