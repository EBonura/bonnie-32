//! Component-based Asset System
//!
//! Transform the engine from simple model references to a full component-based
//! asset system where:
//! - An "Asset" is a self-contained 3D object with embedded mesh + component definitions
//! - Asset Editor is the central hub for defining all asset components
//! - World Editor imports assets via the shared Asset Browser
//! - Runtime spawns entities with the appropriate ECS components from asset definitions
//!
//! ## Core Concept: Asset = Composition of Components
//!
//! Everything is a component - including mesh! Mesh data is embedded (not a file reference).
//!
//! ```text
//! Asset
//! ├── id: u64 (stable reference)
//! ├── name: String
//! ├── components: Vec<AssetComponent>
//! │   ├── Mesh { parts: Vec<MeshPart> }  // EMBEDDED mesh data
//! │   │   └── Each object has geometry + TextureRef::Id (points to shared textures)
//! │   ├── Collision { shape: CollisionShapeDef }
//! │   ├── Light { color, intensity, radius, offset }
//! │   ├── Trigger { trigger_id, on_enter, on_exit }
//! │   ├── Pickup { item_type: ItemType }
//! │   ├── Enemy { enemy_type, health, damage, patrol_radius }
//! │   └── ... (extensible)
//! └── metadata: category, tags, description
//! ```
//!
//! ## File Structure
//!
//! ```text
//! assets/
//! ├── textures-user/    # Shared textures (World + Assets use these)
//! │   └── *.ron         # UserTexture with embedded palette
//! └── assets/           # Self-contained asset bundles
//!     └── *.ron         # Mesh + texture refs + components
//! ```

mod asset;
mod component;
mod library;

pub use asset::{Asset, AssetError, generate_asset_id};
pub use component::{AssetComponent, CollisionShapeDef};
pub use library::{AssetLibrary, ASSETS_DIR};
