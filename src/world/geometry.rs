//! Core geometry types for TR1-style levels
//!
//! Pure data structures with minimal behavior.
//! All rendering/collision logic lives in separate modules.

use serde::{Serialize, Deserialize};
use crate::rasterizer::{Vec3, Vec2, Vertex, Face as RasterFace};

/// Axis-aligned bounding box
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// Check if a point is inside the box
    pub fn contains(&self, point: Vec3) -> bool {
        point.x >= self.min.x && point.x <= self.max.x
            && point.y >= self.min.y && point.y <= self.max.y
            && point.z >= self.min.z && point.z <= self.max.z
    }

    /// Expand bounds to include a point
    pub fn expand(&mut self, point: Vec3) {
        self.min.x = self.min.x.min(point.x);
        self.min.y = self.min.y.min(point.y);
        self.min.z = self.min.z.min(point.z);
        self.max.x = self.max.x.max(point.x);
        self.max.y = self.max.y.max(point.y);
        self.max.z = self.max.z.max(point.z);
    }

    /// Get center of the box
    pub fn center(&self) -> Vec3 {
        Vec3::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
            (self.min.z + self.max.z) * 0.5,
        )
    }
}

/// A face (triangle or quad) in a room
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Face {
    /// Vertex indices (4 elements; for triangles, indices[3] == indices[2])
    pub indices: [usize; 4],
    /// True if this is a triangle (only 3 unique vertices)
    pub is_triangle: bool,
    /// Texture/material ID
    pub texture_id: usize,
    /// Render both sides (for thin walls, etc.)
    pub double_sided: bool,
}

impl Face {
    /// Create a quad face
    pub fn quad(v0: usize, v1: usize, v2: usize, v3: usize, texture_id: usize) -> Self {
        Self {
            indices: [v0, v1, v2, v3],
            is_triangle: false,
            texture_id,
            double_sided: false,
        }
    }

    /// Create a triangle face
    pub fn tri(v0: usize, v1: usize, v2: usize, texture_id: usize) -> Self {
        Self {
            indices: [v0, v1, v2, v2], // Duplicate last vertex for uniform handling
            is_triangle: true,
            texture_id,
            double_sided: false,
        }
    }

    /// Set double-sided rendering
    pub fn with_double_sided(mut self, double_sided: bool) -> Self {
        self.double_sided = double_sided;
        self
    }
}

/// Portal connecting two rooms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Portal {
    /// Target room ID
    pub target_room: usize,
    /// Portal corners in room-relative coordinates (4 vertices)
    pub vertices: [Vec3; 4],
    /// Portal facing direction (points into the room)
    pub normal: Vec3,
}

impl Portal {
    pub fn new(target_room: usize, vertices: [Vec3; 4], normal: Vec3) -> Self {
        Self {
            target_room,
            vertices,
            normal: normal.normalize(),
        }
    }

    /// Get portal center
    pub fn center(&self) -> Vec3 {
        Vec3::new(
            (self.vertices[0].x + self.vertices[1].x + self.vertices[2].x + self.vertices[3].x) * 0.25,
            (self.vertices[0].y + self.vertices[1].y + self.vertices[2].y + self.vertices[3].y) * 0.25,
            (self.vertices[0].z + self.vertices[1].z + self.vertices[2].z + self.vertices[3].z) * 0.25,
        )
    }
}

/// A room in the level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    /// Unique room identifier
    pub id: usize,
    /// Room position in world space
    pub position: Vec3,
    /// Vertices in room-relative coordinates
    pub vertices: Vec<Vec3>,
    /// Faces referencing vertices by index
    pub faces: Vec<Face>,
    /// Portals to adjacent rooms
    #[serde(default)]
    pub portals: Vec<Portal>,
    /// Bounding box (room-relative) - computed from vertices, not serialized
    #[serde(skip)]
    pub bounds: Aabb,
    /// Ambient light level (0.0 = dark, 1.0 = bright)
    #[serde(default = "default_ambient")]
    pub ambient: f32,
}

fn default_ambient() -> f32 {
    0.5
}

impl Room {
    pub fn new(id: usize, position: Vec3) -> Self {
        Self {
            id,
            position,
            vertices: Vec::new(),
            faces: Vec::new(),
            portals: Vec::new(),
            bounds: Aabb::new(
                Vec3::new(f32::MAX, f32::MAX, f32::MAX),
                Vec3::new(f32::MIN, f32::MIN, f32::MIN),
            ),
            ambient: 0.5,
        }
    }

    /// Add a vertex and return its index
    pub fn add_vertex(&mut self, x: f32, y: f32, z: f32) -> usize {
        let v = Vec3::new(x, y, z);
        self.bounds.expand(v);
        self.vertices.push(v);
        self.vertices.len() - 1
    }

    /// Add a quad face
    pub fn add_quad(&mut self, v0: usize, v1: usize, v2: usize, v3: usize, texture_id: usize) {
        self.faces.push(Face::quad(v0, v1, v2, v3, texture_id));
    }

    /// Add a triangle face
    pub fn add_tri(&mut self, v0: usize, v1: usize, v2: usize, texture_id: usize) {
        self.faces.push(Face::tri(v0, v1, v2, texture_id));
    }

    /// Add a portal to another room
    pub fn add_portal(&mut self, target_room: usize, vertices: [Vec3; 4], normal: Vec3) {
        self.portals.push(Portal::new(target_room, vertices, normal));
    }

    /// Recalculate bounds from vertices (call after loading from file)
    pub fn recalculate_bounds(&mut self) {
        if self.vertices.is_empty() {
            self.bounds = Aabb::default();
            return;
        }

        self.bounds = Aabb::new(
            Vec3::new(f32::MAX, f32::MAX, f32::MAX),
            Vec3::new(f32::MIN, f32::MIN, f32::MIN),
        );

        for v in &self.vertices {
            self.bounds.expand(*v);
        }
    }

    /// Check if a world-space point is inside this room's bounds
    pub fn contains_point(&self, point: Vec3) -> bool {
        // Convert point to room-relative coordinates
        let relative = Vec3::new(
            point.x - self.position.x,
            point.y - self.position.y,
            point.z - self.position.z,
        );
        self.bounds.contains(relative)
    }

    /// Get world-space bounds
    pub fn world_bounds(&self) -> Aabb {
        Aabb::new(
            Vec3::new(
                self.bounds.min.x + self.position.x,
                self.bounds.min.y + self.position.y,
                self.bounds.min.z + self.position.z,
            ),
            Vec3::new(
                self.bounds.max.x + self.position.x,
                self.bounds.max.y + self.position.y,
                self.bounds.max.z + self.position.z,
            ),
        )
    }

    /// Convert room geometry to rasterizer format (vertices + faces)
    /// Returns world-space vertices ready for rendering
    pub fn to_render_data(&self) -> (Vec<Vertex>, Vec<RasterFace>) {
        let mut vertices = Vec::with_capacity(self.faces.len() * 4);
        let mut faces = Vec::with_capacity(self.faces.len() * 2);

        for face in &self.faces {
            let base_idx = vertices.len();

            // Get the 4 vertices (or 3 for triangles, with last duplicated)
            let v0 = self.vertices[face.indices[0]];
            let v1 = self.vertices[face.indices[1]];
            let v2 = self.vertices[face.indices[2]];
            let v3 = self.vertices[face.indices[3]];

            // Convert to world space
            let world_v0 = v0 + self.position;
            let world_v1 = v1 + self.position;
            let world_v2 = v2 + self.position;
            let world_v3 = v3 + self.position;

            // Calculate face normal from first triangle
            let edge1 = world_v1 - world_v0;
            let edge2 = world_v2 - world_v0;
            let normal = edge1.cross(edge2).normalize();

            // Create vertices with UVs and normals
            vertices.push(Vertex::new(world_v0, Vec2::new(0.0, 0.0), normal));
            vertices.push(Vertex::new(world_v1, Vec2::new(1.0, 0.0), normal));
            vertices.push(Vertex::new(world_v2, Vec2::new(1.0, 1.0), normal));
            vertices.push(Vertex::new(world_v3, Vec2::new(0.0, 1.0), normal));

            // Create two triangles for the quad
            faces.push(RasterFace::with_texture(base_idx, base_idx + 1, base_idx + 2, face.texture_id));

            if !face.is_triangle {
                faces.push(RasterFace::with_texture(base_idx, base_idx + 2, base_idx + 3, face.texture_id));
            }
        }

        (vertices, faces)
    }
}

/// Editor layout configuration (saved with level)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorLayoutConfig {
    /// Main horizontal split ratio (left panels | center+right)
    pub main_split: f32,
    /// Right split ratio (center viewport | right panels)
    pub right_split: f32,
    /// Left vertical split ratio (2D grid | room properties)
    pub left_split: f32,
    /// Right vertical split ratio (texture palette | properties)
    pub right_panel_split: f32,
}

impl Default for EditorLayoutConfig {
    fn default() -> Self {
        Self {
            main_split: 0.25,
            right_split: 0.75,
            left_split: 0.6,
            right_panel_split: 0.6,
        }
    }
}

/// The entire level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Level {
    pub rooms: Vec<Room>,
    /// Editor layout configuration (optional, uses default if missing)
    #[serde(default)]
    pub editor_layout: EditorLayoutConfig,
}

impl Level {
    pub fn new() -> Self {
        Self {
            rooms: Vec::new(),
            editor_layout: EditorLayoutConfig::default(),
        }
    }

    /// Add a room and return its index
    pub fn add_room(&mut self, room: Room) -> usize {
        let id = self.rooms.len();
        self.rooms.push(room);
        id
    }

    /// Find which room contains a point
    pub fn find_room_at(&self, point: Vec3) -> Option<usize> {
        for (i, room) in self.rooms.iter().enumerate() {
            if room.contains_point(point) {
                return Some(i);
            }
        }
        None
    }

    /// Find room with hint (check hint first for faster lookup)
    pub fn find_room_at_with_hint(&self, point: Vec3, hint: Option<usize>) -> Option<usize> {
        // Check hint first
        if let Some(hint_id) = hint {
            if let Some(room) = self.rooms.get(hint_id) {
                if room.contains_point(point) {
                    return Some(hint_id);
                }
            }
        }

        // Fall back to linear search
        self.find_room_at(point)
    }
}

/// Create an empty level with a single starter room (floor only)
/// Uses TRLE sector size (1024 units) for proper grid alignment
pub fn create_empty_level() -> Level {
    let mut level = Level::new();

    // Create a single starter room with one sector floor
    let mut room0 = Room::new(0, Vec3::ZERO);

    // Floor vertices - single 1024×1024 TRLE sector at y = 0
    let f0 = room0.add_vertex(0.0, 0.0, 0.0);
    let f1 = room0.add_vertex(1024.0, 0.0, 0.0);
    let f2 = room0.add_vertex(1024.0, 0.0, 1024.0);
    let f3 = room0.add_vertex(0.0, 0.0, 1024.0);

    // Floor face
    room0.add_quad(f0, f1, f2, f3, 0);

    room0.recalculate_bounds();
    level.rooms.push(room0);

    level
}

/// Create a simple test level with two connected rooms
/// Uses TRLE sector sizes (1024 units per sector)
pub fn create_test_level() -> Level {
    let mut level = Level::new();

    // Room 0: Single sector room (1024×1024, height 1024 = 4 clicks)
    let mut room0 = Room::new(0, Vec3::ZERO);

    // Floor vertices (y = 0)
    let f0 = room0.add_vertex(0.0, 0.0, 0.0);
    let f1 = room0.add_vertex(1024.0, 0.0, 0.0);
    let f2 = room0.add_vertex(1024.0, 0.0, 1024.0);
    let f3 = room0.add_vertex(0.0, 0.0, 1024.0);

    // Ceiling vertices (y = 1024 = 4 clicks high)
    let c0 = room0.add_vertex(0.0, 1024.0, 0.0);
    let c1 = room0.add_vertex(1024.0, 1024.0, 0.0);
    let c2 = room0.add_vertex(1024.0, 1024.0, 1024.0);
    let c3 = room0.add_vertex(0.0, 1024.0, 1024.0);

    // Floor
    room0.add_quad(f0, f1, f2, f3, 0);

    // Ceiling
    room0.add_quad(c3, c2, c1, c0, 0);

    // Four walls
    // Wall at Z=0 (-Z side)
    room0.add_quad(f0, c0, c1, f1, 0);
    // Wall at X=0 (-X side)
    room0.add_quad(f3, c3, c0, f0, 0);
    // Wall at X=1024 (+X side)
    room0.add_quad(f1, c1, c2, f2, 0);
    // Wall at Z=1024 (+Z side)
    room0.add_quad(f2, c2, c3, f3, 0);

    room0.recalculate_bounds();
    level.add_room(room0);

    level
}
