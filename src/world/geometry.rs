//! Core geometry types for TR1-style levels
//!
//! Sector-based geometry system inspired by TRLE.
//! Rooms contain a 2D grid of sectors, each with floor, ceiling, and walls.

use serde::{Serialize, Deserialize};
use crate::rasterizer::{Vec3, Vec2, Vertex, Face as RasterFace, BlendMode, Color};

/// TRLE sector size in world units
pub const SECTOR_SIZE: f32 = 1024.0;

/// Texture reference by pack and name
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextureRef {
    /// Texture pack name (e.g., "SAMPLE")
    pub pack: String,
    /// Texture name without extension (e.g., "floor_01")
    pub name: String,
}

impl TextureRef {
    pub fn new(pack: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            pack: pack.into(),
            name: name.into(),
        }
    }

    /// Create a None reference (uses fallback checkerboard)
    pub fn none() -> Self {
        Self {
            pack: String::new(),
            name: String::new(),
        }
    }

    /// Check if this is a valid reference
    pub fn is_valid(&self) -> bool {
        !self.pack.is_empty() && !self.name.is_empty()
    }
}

impl Default for TextureRef {
    fn default() -> Self {
        Self::none()
    }
}

fn default_true() -> bool { true }
fn default_neutral_color() -> Color { Color::NEUTRAL }
fn default_neutral_colors_4() -> [Color; 4] { [Color::NEUTRAL; 4] }

/// A horizontal face (floor or ceiling)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HorizontalFace {
    /// Corner heights [NW, NE, SE, SW] - allows sloped surfaces
    /// NW = (-X, -Z), NE = (+X, -Z), SE = (+X, +Z), SW = (-X, +Z)
    pub heights: [f32; 4],
    /// Texture reference
    pub texture: TextureRef,
    /// Custom UV coordinates (None = use default 0,0 to 1,1)
    #[serde(default)]
    pub uv: Option<[Vec2; 4]>,
    /// Is this surface walkable? (for collision/AI)
    #[serde(default = "default_true")]
    pub walkable: bool,
    /// Transparency/blend mode
    #[serde(default)]
    pub blend_mode: BlendMode,
    /// PS1-style vertex colors for texture modulation [NW, NE, SE, SW]
    /// 128 = neutral (no tint), <128 = darken, >128 = brighten
    /// Per-vertex colors enable Gouraud-style color gradients across the face
    #[serde(default = "default_neutral_colors_4")]
    pub colors: [Color; 4],
}

impl HorizontalFace {
    /// Create a flat horizontal face at the given height
    pub fn flat(height: f32, texture: TextureRef) -> Self {
        Self {
            heights: [height, height, height, height],
            texture,
            uv: None,
            walkable: true,
            blend_mode: BlendMode::Opaque,
            colors: [Color::NEUTRAL; 4],
        }
    }

    /// Create a sloped horizontal face
    pub fn sloped(heights: [f32; 4], texture: TextureRef) -> Self {
        Self {
            heights,
            texture,
            uv: None,
            walkable: true,
            blend_mode: BlendMode::Opaque,
            colors: [Color::NEUTRAL; 4],
        }
    }

    /// Set all vertex colors to the same value (uniform tint)
    pub fn set_uniform_color(&mut self, color: Color) {
        self.colors = [color; 4];
    }

    /// Check if all vertex colors are the same
    pub fn has_uniform_color(&self) -> bool {
        self.colors[0].r == self.colors[1].r && self.colors[0].r == self.colors[2].r && self.colors[0].r == self.colors[3].r &&
        self.colors[0].g == self.colors[1].g && self.colors[0].g == self.colors[2].g && self.colors[0].g == self.colors[3].g &&
        self.colors[0].b == self.colors[1].b && self.colors[0].b == self.colors[2].b && self.colors[0].b == self.colors[3].b
    }

    /// Get average height of the face
    pub fn avg_height(&self) -> f32 {
        (self.heights[0] + self.heights[1] + self.heights[2] + self.heights[3]) / 4.0
    }

    /// Check if the face is flat (all corners at same height)
    pub fn is_flat(&self) -> bool {
        let h = self.heights[0];
        self.heights.iter().all(|&corner| (corner - h).abs() < 0.001)
    }

    /// Get heights at a specific edge (left_corner, right_corner) when looking from inside the sector
    /// Returns (left_height, right_height) for the edge in that direction
    pub fn edge_heights(&self, dir: Direction) -> (f32, f32) {
        // Heights are [NW, NE, SE, SW] = [0, 1, 2, 3]
        // NW = (-X, -Z), NE = (+X, -Z), SE = (+X, +Z), SW = (-X, +Z)
        match dir {
            Direction::North => (self.heights[0], self.heights[1]), // NW, NE (looking at -Z edge)
            Direction::East => (self.heights[1], self.heights[2]),  // NE, SE (looking at +X edge)
            Direction::South => (self.heights[3], self.heights[2]), // SW, SE (looking at +Z edge)
            Direction::West => (self.heights[0], self.heights[3]),  // NW, SW (looking at -X edge)
        }
    }

    /// Get max height at a specific edge
    pub fn edge_max(&self, dir: Direction) -> f32 {
        let (h1, h2) = self.edge_heights(dir);
        h1.max(h2)
    }

    /// Get min height at a specific edge
    pub fn edge_min(&self, dir: Direction) -> f32 {
        let (h1, h2) = self.edge_heights(dir);
        h1.min(h2)
    }
}

/// A vertical face (wall) on a sector edge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerticalFace {
    /// Corner heights: [bottom-left, bottom-right, top-right, top-left]
    pub heights: [f32; 4],
    /// Texture reference
    pub texture: TextureRef,
    /// Custom UV coordinates (None = use default)
    #[serde(default)]
    pub uv: Option<[Vec2; 4]>,
    /// Is this a solid wall for collision?
    #[serde(default = "default_true")]
    pub solid: bool,
    /// Transparency/blend mode
    #[serde(default)]
    pub blend_mode: BlendMode,
    /// PS1-style vertex colors for texture modulation [bottom-left, bottom-right, top-right, top-left]
    /// 128 = neutral (no tint), <128 = darken, >128 = brighten
    /// Per-vertex colors enable Gouraud-style color gradients across the wall
    #[serde(default = "default_neutral_colors_4")]
    pub colors: [Color; 4],
}

impl VerticalFace {
    /// Create a wall from bottom to top (all corners at same heights)
    pub fn new(y_bottom: f32, y_top: f32, texture: TextureRef) -> Self {
        Self {
            heights: [y_bottom, y_bottom, y_top, y_top],
            texture,
            uv: None,
            solid: true,
            blend_mode: BlendMode::Opaque,
            colors: [Color::NEUTRAL; 4],
        }
    }

    /// Set all vertex colors to the same value (uniform tint)
    pub fn set_uniform_color(&mut self, color: Color) {
        self.colors = [color; 4];
    }

    /// Check if all vertex colors are the same
    pub fn has_uniform_color(&self) -> bool {
        self.colors[0].r == self.colors[1].r && self.colors[0].r == self.colors[2].r && self.colors[0].r == self.colors[3].r &&
        self.colors[0].g == self.colors[1].g && self.colors[0].g == self.colors[2].g && self.colors[0].g == self.colors[3].g &&
        self.colors[0].b == self.colors[1].b && self.colors[0].b == self.colors[2].b && self.colors[0].b == self.colors[3].b
    }

    /// Get the average height of this wall
    pub fn height(&self) -> f32 {
        let bottom = (self.heights[0] + self.heights[1]) / 2.0;
        let top = (self.heights[2] + self.heights[3]) / 2.0;
        top - bottom
    }

    /// Get the bottom Y (average of bottom corners)
    pub fn y_bottom(&self) -> f32 {
        (self.heights[0] + self.heights[1]) / 2.0
    }

    /// Get the top Y (average of top corners)
    pub fn y_top(&self) -> f32 {
        (self.heights[2] + self.heights[3]) / 2.0
    }

    /// Check if wall has uniform heights (all bottom same, all top same)
    pub fn is_flat(&self) -> bool {
        let bottom_same = (self.heights[0] - self.heights[1]).abs() < 0.001;
        let top_same = (self.heights[2] - self.heights[3]).abs() < 0.001;
        bottom_same && top_same
    }
}

/// A single sector in the room grid
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Sector {
    /// Floor face (None = no floor / pit)
    pub floor: Option<HorizontalFace>,
    /// Ceiling face (None = no ceiling / open sky)
    pub ceiling: Option<HorizontalFace>,
    /// Walls on north edge (-Z) - can have multiple stacked
    #[serde(default)]
    pub walls_north: Vec<VerticalFace>,
    /// Walls on east edge (+X)
    #[serde(default)]
    pub walls_east: Vec<VerticalFace>,
    /// Walls on south edge (+Z)
    #[serde(default)]
    pub walls_south: Vec<VerticalFace>,
    /// Walls on west edge (-X)
    #[serde(default)]
    pub walls_west: Vec<VerticalFace>,
}

impl Sector {
    /// Create an empty sector (no floor, ceiling, or walls)
    pub fn empty() -> Self {
        Self::default()
    }

    /// Create a sector with just a floor
    pub fn with_floor(height: f32, texture: TextureRef) -> Self {
        Self {
            floor: Some(HorizontalFace::flat(height, texture)),
            ..Default::default()
        }
    }

    /// Create a sector with floor and ceiling
    pub fn with_floor_and_ceiling(floor_height: f32, ceiling_height: f32, texture: TextureRef) -> Self {
        Self {
            floor: Some(HorizontalFace::flat(floor_height, texture.clone())),
            ceiling: Some(HorizontalFace::flat(ceiling_height, texture)),
            ..Default::default()
        }
    }

    /// Check if this sector has any geometry
    pub fn has_geometry(&self) -> bool {
        self.floor.is_some()
            || self.ceiling.is_some()
            || !self.walls_north.is_empty()
            || !self.walls_east.is_empty()
            || !self.walls_south.is_empty()
            || !self.walls_west.is_empty()
    }

    /// Get all walls on a given edge
    pub fn walls(&self, direction: Direction) -> &Vec<VerticalFace> {
        match direction {
            Direction::North => &self.walls_north,
            Direction::East => &self.walls_east,
            Direction::South => &self.walls_south,
            Direction::West => &self.walls_west,
        }
    }

    /// Get mutable walls on a given edge
    pub fn walls_mut(&mut self, direction: Direction) -> &mut Vec<VerticalFace> {
        match direction {
            Direction::North => &mut self.walls_north,
            Direction::East => &mut self.walls_east,
            Direction::South => &mut self.walls_south,
            Direction::West => &mut self.walls_west,
        }
    }
}

/// Cardinal direction for sector edges
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Direction {
    North,  // -Z
    East,   // +X
    South,  // +Z
    West,   // -X
}

impl Direction {
    /// Get the opposite direction
    pub fn opposite(self) -> Self {
        match self {
            Direction::North => Direction::South,
            Direction::East => Direction::West,
            Direction::South => Direction::North,
            Direction::West => Direction::East,
        }
    }

    /// Get offset in grid coordinates
    pub fn offset(self) -> (i32, i32) {
        match self {
            Direction::North => (0, -1),
            Direction::East => (1, 0),
            Direction::South => (0, 1),
            Direction::West => (-1, 0),
        }
    }
}

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

/// A room in the level - contains a 2D grid of sectors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    /// Unique room identifier
    pub id: usize,
    /// Room position in world space (origin of sector grid)
    pub position: Vec3,
    /// Grid width (number of sectors in X direction)
    pub width: usize,
    /// Grid depth (number of sectors in Z direction)
    pub depth: usize,
    /// 2D array of sectors [x][z], None = no sector at this position
    pub sectors: Vec<Vec<Option<Sector>>>,
    /// Portals to adjacent rooms
    #[serde(default)]
    pub portals: Vec<Portal>,
    /// Bounding box (room-relative) - computed from sectors, not serialized
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
    /// Create a new empty room with the given grid size
    pub fn new(id: usize, position: Vec3, width: usize, depth: usize) -> Self {
        // Initialize 2D grid with None
        let sectors = (0..width)
            .map(|_| (0..depth).map(|_| None).collect())
            .collect();

        Self {
            id,
            position,
            width,
            depth,
            sectors,
            portals: Vec::new(),
            bounds: Aabb::default(),
            ambient: 0.5,
        }
    }

    /// Get sector at grid position (returns None if out of bounds or empty)
    pub fn get_sector(&self, x: usize, z: usize) -> Option<&Sector> {
        self.sectors.get(x)?.get(z)?.as_ref()
    }

    /// Get mutable sector at grid position
    pub fn get_sector_mut(&mut self, x: usize, z: usize) -> Option<&mut Sector> {
        self.sectors.get_mut(x)?.get_mut(z)?.as_mut()
    }

    /// Set sector at grid position (creates if doesn't exist)
    pub fn set_sector(&mut self, x: usize, z: usize, sector: Sector) {
        if x < self.width && z < self.depth {
            self.sectors[x][z] = Some(sector);
        }
    }

    /// Remove sector at grid position
    pub fn remove_sector(&mut self, x: usize, z: usize) {
        if x < self.width && z < self.depth {
            self.sectors[x][z] = None;
        }
    }

    /// Ensure sector exists at position, creating empty one if needed
    pub fn ensure_sector(&mut self, x: usize, z: usize) -> &mut Sector {
        if x < self.width && z < self.depth {
            if self.sectors[x][z].is_none() {
                self.sectors[x][z] = Some(Sector::empty());
            }
            self.sectors[x][z].as_mut().unwrap()
        } else {
            panic!("Sector position ({}, {}) out of bounds", x, z);
        }
    }

    /// Set floor at grid position
    pub fn set_floor(&mut self, x: usize, z: usize, height: f32, texture: TextureRef) {
        let sector = self.ensure_sector(x, z);
        sector.floor = Some(HorizontalFace::flat(height, texture));
    }

    /// Set ceiling at grid position
    pub fn set_ceiling(&mut self, x: usize, z: usize, height: f32, texture: TextureRef) {
        let sector = self.ensure_sector(x, z);
        sector.ceiling = Some(HorizontalFace::flat(height, texture));
    }

    /// Add a wall on a sector edge
    pub fn add_wall(&mut self, x: usize, z: usize, direction: Direction, y_bottom: f32, y_top: f32, texture: TextureRef) {
        let sector = self.ensure_sector(x, z);
        sector.walls_mut(direction).push(VerticalFace::new(y_bottom, y_top, texture));
    }

    /// Add a portal to another room
    pub fn add_portal(&mut self, target_room: usize, vertices: [Vec3; 4], normal: Vec3) {
        self.portals.push(Portal::new(target_room, vertices, normal));
    }

    /// Convert world position to grid coordinates
    pub fn world_to_grid(&self, world_x: f32, world_z: f32) -> Option<(usize, usize)> {
        let local_x = world_x - self.position.x;
        let local_z = world_z - self.position.z;

        if local_x < 0.0 || local_z < 0.0 {
            return None;
        }

        let grid_x = (local_x / SECTOR_SIZE) as usize;
        let grid_z = (local_z / SECTOR_SIZE) as usize;

        if grid_x < self.width && grid_z < self.depth {
            Some((grid_x, grid_z))
        } else {
            None
        }
    }

    /// Convert grid coordinates to world position (returns corner of sector)
    pub fn grid_to_world(&self, x: usize, z: usize) -> Vec3 {
        Vec3::new(
            self.position.x + (x as f32) * SECTOR_SIZE,
            self.position.y,
            self.position.z + (z as f32) * SECTOR_SIZE,
        )
    }

    /// Recalculate bounds from sectors (call after loading from file)
    pub fn recalculate_bounds(&mut self) {
        self.bounds = Aabb::new(
            Vec3::new(f32::MAX, f32::MAX, f32::MAX),
            Vec3::new(f32::MIN, f32::MIN, f32::MIN),
        );

        for x in 0..self.width {
            for z in 0..self.depth {
                if let Some(sector) = &self.sectors[x][z] {
                    let base_x = (x as f32) * SECTOR_SIZE;
                    let base_z = (z as f32) * SECTOR_SIZE;

                    // Expand bounds for floor corners
                    if let Some(floor) = &sector.floor {
                        for (i, &h) in floor.heights.iter().enumerate() {
                            let (dx, dz) = match i {
                                0 => (0.0, 0.0),           // NW
                                1 => (SECTOR_SIZE, 0.0),   // NE
                                2 => (SECTOR_SIZE, SECTOR_SIZE), // SE
                                3 => (0.0, SECTOR_SIZE),   // SW
                                _ => unreachable!(),
                            };
                            self.bounds.expand(Vec3::new(base_x + dx, h, base_z + dz));
                        }
                    }

                    // Expand bounds for ceiling corners
                    if let Some(ceiling) = &sector.ceiling {
                        for (i, &h) in ceiling.heights.iter().enumerate() {
                            let (dx, dz) = match i {
                                0 => (0.0, 0.0),
                                1 => (SECTOR_SIZE, 0.0),
                                2 => (SECTOR_SIZE, SECTOR_SIZE),
                                3 => (0.0, SECTOR_SIZE),
                                _ => unreachable!(),
                            };
                            self.bounds.expand(Vec3::new(base_x + dx, h, base_z + dz));
                        }
                    }

                    // Expand bounds for wall corners (walls can extend beyond floor/ceiling)
                    for wall in &sector.walls_north {
                        for &h in &wall.heights {
                            self.bounds.expand(Vec3::new(base_x, h, base_z));
                        }
                    }
                    for wall in &sector.walls_east {
                        for &h in &wall.heights {
                            self.bounds.expand(Vec3::new(base_x + SECTOR_SIZE, h, base_z));
                        }
                    }
                    for wall in &sector.walls_south {
                        for &h in &wall.heights {
                            self.bounds.expand(Vec3::new(base_x, h, base_z + SECTOR_SIZE));
                        }
                    }
                    for wall in &sector.walls_west {
                        for &h in &wall.heights {
                            self.bounds.expand(Vec3::new(base_x, h, base_z));
                        }
                    }
                }
            }
        }
    }

    /// Trim empty rows and columns from the edges of the room grid.
    /// Adjusts room position to keep sectors in the same world position.
    pub fn trim_empty_edges(&mut self) {
        if self.sectors.is_empty() || self.width == 0 || self.depth == 0 {
            return;
        }

        // Find first non-empty column (from left)
        let mut first_col = 0;
        while first_col < self.width {
            let col_has_sector = (0..self.depth).any(|z| self.sectors[first_col][z].is_some());
            if col_has_sector {
                break;
            }
            first_col += 1;
        }

        // Find last non-empty column (from right)
        let mut last_col = self.width;
        while last_col > first_col {
            let col_has_sector = (0..self.depth).any(|z| self.sectors[last_col - 1][z].is_some());
            if col_has_sector {
                break;
            }
            last_col -= 1;
        }

        // Find first non-empty row (from front)
        let mut first_row = 0;
        while first_row < self.depth {
            let row_has_sector = (first_col..last_col).any(|x| self.sectors[x][first_row].is_some());
            if row_has_sector {
                break;
            }
            first_row += 1;
        }

        // Find last non-empty row (from back)
        let mut last_row = self.depth;
        while last_row > first_row {
            let row_has_sector = (first_col..last_col).any(|x| self.sectors[x][last_row - 1].is_some());
            if row_has_sector {
                break;
            }
            last_row -= 1;
        }

        // If grid is completely empty, leave at least 1x1
        if first_col >= last_col || first_row >= last_row {
            self.width = 1;
            self.depth = 1;
            self.sectors = vec![vec![None]];
            return;
        }

        // Apply trimming if needed
        if first_col > 0 || first_row > 0 || last_col < self.width || last_row < self.depth {
            // Adjust room position for removed columns/rows at the start
            self.position.x += (first_col as f32) * SECTOR_SIZE;
            self.position.z += (first_row as f32) * SECTOR_SIZE;

            // Extract the trimmed portion
            let new_width = last_col - first_col;
            let new_depth = last_row - first_row;
            let mut new_sectors = Vec::with_capacity(new_width);

            for x in first_col..last_col {
                let mut col = Vec::with_capacity(new_depth);
                for z in first_row..last_row {
                    col.push(self.sectors[x][z].take());
                }
                new_sectors.push(col);
            }

            self.sectors = new_sectors;
            self.width = new_width;
            self.depth = new_depth;
        }
    }

    /// Check if a world-space point is inside this room's bounds
    pub fn contains_point(&self, point: Vec3) -> bool {
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

    /// Iterate over all sectors with their grid coordinates
    pub fn iter_sectors(&self) -> impl Iterator<Item = (usize, usize, &Sector)> {
        self.sectors.iter().enumerate().flat_map(|(x, col)| {
            col.iter().enumerate().filter_map(move |(z, sector)| {
                sector.as_ref().map(|s| (x, z, s))
            })
        })
    }

    /// Convert room geometry to rasterizer format (vertices + faces)
    /// Returns world-space vertices ready for rendering
    pub fn to_render_data_with_textures<F>(&self, resolve_texture: F) -> (Vec<Vertex>, Vec<RasterFace>)
    where
        F: Fn(&TextureRef) -> Option<usize>,
    {
        let mut vertices = Vec::new();
        let mut faces = Vec::new();

        for (grid_x, grid_z, sector) in self.iter_sectors() {
            let base_x = self.position.x + (grid_x as f32) * SECTOR_SIZE;
            let base_z = self.position.z + (grid_z as f32) * SECTOR_SIZE;

            // Render floor
            if let Some(floor) = &sector.floor {
                self.add_horizontal_face_to_render_data(
                    &mut vertices,
                    &mut faces,
                    floor,
                    base_x,
                    base_z,
                    true, // is_floor
                    &resolve_texture,
                );
            }

            // Render ceiling
            if let Some(ceiling) = &sector.ceiling {
                self.add_horizontal_face_to_render_data(
                    &mut vertices,
                    &mut faces,
                    ceiling,
                    base_x,
                    base_z,
                    false, // is_ceiling
                    &resolve_texture,
                );
            }

            // Render walls on each edge
            for wall in &sector.walls_north {
                self.add_wall_to_render_data(&mut vertices, &mut faces, wall, base_x, base_z, Direction::North, &resolve_texture);
            }
            for wall in &sector.walls_east {
                self.add_wall_to_render_data(&mut vertices, &mut faces, wall, base_x, base_z, Direction::East, &resolve_texture);
            }
            for wall in &sector.walls_south {
                self.add_wall_to_render_data(&mut vertices, &mut faces, wall, base_x, base_z, Direction::South, &resolve_texture);
            }
            for wall in &sector.walls_west {
                self.add_wall_to_render_data(&mut vertices, &mut faces, wall, base_x, base_z, Direction::West, &resolve_texture);
            }
        }

        (vertices, faces)
    }

    /// Helper to add a horizontal face (floor or ceiling) to render data
    fn add_horizontal_face_to_render_data<F>(
        &self,
        vertices: &mut Vec<Vertex>,
        faces: &mut Vec<RasterFace>,
        face: &HorizontalFace,
        base_x: f32,
        base_z: f32,
        is_floor: bool,
        resolve_texture: &F,
    )
    where
        F: Fn(&TextureRef) -> Option<usize>,
    {
        let base_idx = vertices.len();

        // Corner positions: NW, NE, SE, SW
        let corners = [
            Vec3::new(base_x, face.heights[0], base_z),                         // NW
            Vec3::new(base_x + SECTOR_SIZE, face.heights[1], base_z),           // NE
            Vec3::new(base_x + SECTOR_SIZE, face.heights[2], base_z + SECTOR_SIZE), // SE
            Vec3::new(base_x, face.heights[3], base_z + SECTOR_SIZE),           // SW
        ];

        // Calculate normal from cross product
        // For floor (facing up): use edge2 x edge1 to get +Y normal
        // For ceiling (facing down): use edge1 x edge2 to get -Y normal
        let edge1 = corners[1] - corners[0]; // NW -> NE (along +X)
        let edge2 = corners[3] - corners[0]; // NW -> SW (along +Z)
        let normal = if is_floor {
            edge2.cross(edge1).normalize() // +Z x +X = +Y (up)
        } else {
            edge1.cross(edge2).normalize() // +X x +Z = -Y (down)
        };

        // Default UVs
        let uvs = face.uv.unwrap_or([
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ]);

        // Add vertices with per-vertex colors for PS1-style texture modulation
        for i in 0..4 {
            vertices.push(Vertex::with_color(corners[i], uvs[i], normal, face.colors[i]));
        }

        let texture_id = resolve_texture(&face.texture).unwrap_or(0);

        // Winding order: floor = CCW from above, ceiling = CW from above (so it faces down)
        if is_floor {
            faces.push(RasterFace::with_texture(base_idx, base_idx + 1, base_idx + 2, texture_id));
            faces.push(RasterFace::with_texture(base_idx, base_idx + 2, base_idx + 3, texture_id));
        } else {
            faces.push(RasterFace::with_texture(base_idx, base_idx + 3, base_idx + 2, texture_id));
            faces.push(RasterFace::with_texture(base_idx, base_idx + 2, base_idx + 1, texture_id));
        }
    }

    /// Helper to add a wall to render data
    fn add_wall_to_render_data<F>(
        &self,
        vertices: &mut Vec<Vertex>,
        faces: &mut Vec<RasterFace>,
        wall: &VerticalFace,
        base_x: f32,
        base_z: f32,
        direction: Direction,
        resolve_texture: &F,
    )
    where
        F: Fn(&TextureRef) -> Option<usize>,
    {
        let base_idx = vertices.len();

        // Wall corners based on direction
        // Each wall has 4 corners: bottom-left, bottom-right, top-right, top-left (from inside room)
        // wall.heights = [bottom-left, bottom-right, top-right, top-left]
        let (corners, normal) = match direction {
            Direction::North => {
                // Wall at -Z edge, facing +Z (into room)
                let corners = [
                    Vec3::new(base_x, wall.heights[0], base_z),                    // bottom-left
                    Vec3::new(base_x + SECTOR_SIZE, wall.heights[1], base_z),      // bottom-right
                    Vec3::new(base_x + SECTOR_SIZE, wall.heights[2], base_z),      // top-right
                    Vec3::new(base_x, wall.heights[3], base_z),                    // top-left
                ];
                (corners, Vec3::new(0.0, 0.0, 1.0))
            }
            Direction::East => {
                // Wall at +X edge, facing -X (into room)
                let corners = [
                    Vec3::new(base_x + SECTOR_SIZE, wall.heights[0], base_z),
                    Vec3::new(base_x + SECTOR_SIZE, wall.heights[1], base_z + SECTOR_SIZE),
                    Vec3::new(base_x + SECTOR_SIZE, wall.heights[2], base_z + SECTOR_SIZE),
                    Vec3::new(base_x + SECTOR_SIZE, wall.heights[3], base_z),
                ];
                (corners, Vec3::new(-1.0, 0.0, 0.0))
            }
            Direction::South => {
                // Wall at +Z edge, facing -Z (into room)
                let corners = [
                    Vec3::new(base_x + SECTOR_SIZE, wall.heights[0], base_z + SECTOR_SIZE),
                    Vec3::new(base_x, wall.heights[1], base_z + SECTOR_SIZE),
                    Vec3::new(base_x, wall.heights[2], base_z + SECTOR_SIZE),
                    Vec3::new(base_x + SECTOR_SIZE, wall.heights[3], base_z + SECTOR_SIZE),
                ];
                (corners, Vec3::new(0.0, 0.0, -1.0))
            }
            Direction::West => {
                // Wall at -X edge, facing +X (into room)
                let corners = [
                    Vec3::new(base_x, wall.heights[0], base_z + SECTOR_SIZE),
                    Vec3::new(base_x, wall.heights[1], base_z),
                    Vec3::new(base_x, wall.heights[2], base_z),
                    Vec3::new(base_x, wall.heights[3], base_z + SECTOR_SIZE),
                ];
                (corners, Vec3::new(1.0, 0.0, 0.0))
            }
        };

        // Default UVs for wall
        let uvs = wall.uv.unwrap_or([
            Vec2::new(0.0, 1.0),  // bottom-left
            Vec2::new(1.0, 1.0),  // bottom-right
            Vec2::new(1.0, 0.0),  // top-right
            Vec2::new(0.0, 0.0),  // top-left
        ]);

        // Add vertices with per-vertex colors for PS1-style texture modulation
        for i in 0..4 {
            vertices.push(Vertex::with_color(corners[i], uvs[i], normal, wall.colors[i]));
        }

        let texture_id = resolve_texture(&wall.texture).unwrap_or(0);

        // Two triangles for the quad (CCW winding when viewed from inside room)
        faces.push(RasterFace::with_texture(base_idx, base_idx + 2, base_idx + 1, texture_id));
        faces.push(RasterFace::with_texture(base_idx, base_idx + 3, base_idx + 2, texture_id));
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
    /// 2D grid view pan offset X (screen pixels)
    #[serde(default)]
    pub grid_offset_x: f32,
    /// 2D grid view pan offset Y (screen pixels)
    #[serde(default)]
    pub grid_offset_y: f32,
    /// 2D grid view zoom level (pixels per world unit)
    #[serde(default = "default_grid_zoom")]
    pub grid_zoom: f32,
    /// 3D orbit camera target X
    #[serde(default = "default_orbit_target_x")]
    pub orbit_target_x: f32,
    /// 3D orbit camera target Y
    #[serde(default = "default_orbit_target_y")]
    pub orbit_target_y: f32,
    /// 3D orbit camera target Z
    #[serde(default = "default_orbit_target_z")]
    pub orbit_target_z: f32,
    /// 3D orbit camera distance from target
    #[serde(default = "default_orbit_distance")]
    pub orbit_distance: f32,
    /// 3D orbit camera horizontal angle (radians)
    #[serde(default = "default_orbit_azimuth")]
    pub orbit_azimuth: f32,
    /// 3D orbit camera vertical angle (radians)
    #[serde(default = "default_orbit_elevation")]
    pub orbit_elevation: f32,
}

fn default_grid_zoom() -> f32 {
    0.1
}

fn default_orbit_target_x() -> f32 { 512.0 }
fn default_orbit_target_y() -> f32 { 512.0 }
fn default_orbit_target_z() -> f32 { 512.0 }
fn default_orbit_distance() -> f32 { 4000.0 }
fn default_orbit_azimuth() -> f32 { 0.8 }
fn default_orbit_elevation() -> f32 { 0.4 }

impl Default for EditorLayoutConfig {
    fn default() -> Self {
        Self {
            main_split: 0.25,
            right_split: 0.75,
            left_split: 0.6,
            right_panel_split: 0.6,
            grid_offset_x: 0.0,
            grid_offset_y: 0.0,
            grid_zoom: 0.1,
            orbit_target_x: 512.0,
            orbit_target_y: 512.0,
            orbit_target_z: 512.0,
            orbit_distance: 4000.0,
            orbit_azimuth: 0.8,
            orbit_elevation: 0.4,
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

    /// Recalculate all portals based on room adjacency
    /// Call this after room positions change, heights change, or walls are added/removed
    pub fn recalculate_portals(&mut self) {
        // Clear existing portals from all rooms
        for room in &mut self.rooms {
            room.portals.clear();
        }

        // For each pair of rooms, detect portals between them
        let num_rooms = self.rooms.len();
        for room_a_idx in 0..num_rooms {
            for room_b_idx in (room_a_idx + 1)..num_rooms {
                self.detect_portals_between(room_a_idx, room_b_idx);
            }
        }
    }

    /// Detect and create portals between two rooms based on adjacent sectors
    fn detect_portals_between(&mut self, room_a_idx: usize, room_b_idx: usize) {
        // We need to check if any sector edges in room A are adjacent to sector edges in room B
        // Two sectors are adjacent if they share an edge at the same world position

        // Get room data (positions and dimensions)
        let (pos_a, width_a, depth_a) = {
            let room = &self.rooms[room_a_idx];
            (room.position, room.width, room.depth)
        };
        let (pos_b, width_b, depth_b) = {
            let room = &self.rooms[room_b_idx];
            (room.position, room.width, room.depth)
        };

        // Check all directions for adjacency
        let directions = [Direction::North, Direction::East, Direction::South, Direction::West];

        for &dir in &directions {
            // For each sector in room A on its boundary facing direction `dir`
            // Check if there's a matching sector in room B on the opposite boundary

            for gx_a in 0..width_a {
                for gz_a in 0..depth_a {
                    // World position of this sector in room A
                    let world_x_a = pos_a.x + (gx_a as f32) * SECTOR_SIZE;
                    let world_z_a = pos_a.z + (gz_a as f32) * SECTOR_SIZE;

                    // World position of the adjacent sector (on the edge in direction `dir`)
                    let (adj_world_x, adj_world_z) = match dir {
                        Direction::North => (world_x_a, world_z_a - SECTOR_SIZE),
                        Direction::East => (world_x_a + SECTOR_SIZE, world_z_a),
                        Direction::South => (world_x_a, world_z_a + SECTOR_SIZE),
                        Direction::West => (world_x_a - SECTOR_SIZE, world_z_a),
                    };

                    // Check if this adjacent position falls within room B's grid
                    let local_x_b = adj_world_x - pos_b.x;
                    let local_z_b = adj_world_z - pos_b.z;

                    // Must be aligned to grid
                    if local_x_b < 0.0 || local_z_b < 0.0 {
                        continue;
                    }
                    if (local_x_b % SECTOR_SIZE).abs() > 0.1 || (local_z_b % SECTOR_SIZE).abs() > 0.1 {
                        continue;
                    }

                    let gx_b = (local_x_b / SECTOR_SIZE) as usize;
                    let gz_b = (local_z_b / SECTOR_SIZE) as usize;

                    if gx_b >= width_b || gz_b >= depth_b {
                        continue;
                    }

                    // Now check if both sectors exist and have no walls blocking
                    let sector_a = self.rooms[room_a_idx].get_sector(gx_a, gz_a);
                    let sector_b = self.rooms[room_b_idx].get_sector(gx_b, gz_b);

                    let (sector_a, sector_b) = match (sector_a, sector_b) {
                        (Some(a), Some(b)) => (a, b),
                        _ => continue, // One or both sectors don't exist
                    };

                    // Check for walls blocking the portal
                    let opposite_dir = dir.opposite();
                    if !sector_a.walls(dir).is_empty() || !sector_b.walls(opposite_dir).is_empty() {
                        continue; // Wall blocks the portal
                    }

                    // Calculate portal opening at each corner (trapezoidal portal for sloped surfaces)
                    // Get edge heights from both sectors: (left, right) when looking from inside
                    // The edge_heights function returns corners in world-space order for the shared edge,
                    // so both rooms' left/right corners line up (A.NW = B.SW, A.NE = B.SE for North/South)
                    let (floor_a_left, floor_a_right) = sector_a.floor.as_ref()
                        .map(|f| f.edge_heights(dir))
                        .unwrap_or((f32::NEG_INFINITY, f32::NEG_INFINITY));
                    let (floor_b_left, floor_b_right) = sector_b.floor.as_ref()
                        .map(|f| f.edge_heights(opposite_dir))
                        .unwrap_or((f32::NEG_INFINITY, f32::NEG_INFINITY));

                    let (ceil_a_left, ceil_a_right) = sector_a.ceiling.as_ref()
                        .map(|c| c.edge_heights(dir))
                        .unwrap_or((f32::INFINITY, f32::INFINITY));
                    let (ceil_b_left, ceil_b_right) = sector_b.ceiling.as_ref()
                        .map(|c| c.edge_heights(opposite_dir))
                        .unwrap_or((f32::INFINITY, f32::INFINITY));

                    // Portal bottom at each corner = max of both floors
                    // Portal top at each corner = min of both ceilings
                    let portal_bottom_left = floor_a_left.max(floor_b_left);
                    let portal_bottom_right = floor_a_right.max(floor_b_right);
                    let portal_top_left = ceil_a_left.min(ceil_b_left);
                    let portal_top_right = ceil_a_right.min(ceil_b_right);

                    // Skip if no vertical opening at either corner
                    if portal_bottom_left >= portal_top_left && portal_bottom_right >= portal_top_right {
                        continue;
                    }

                    // Create portal vertices (quad at the shared edge)
                    // Vertices are in world space, will be converted to room-relative when stored
                    // v0=bottom-left, v1=bottom-right, v2=top-right, v3=top-left
                    let (v0, v1, v2, v3, normal_a) = match dir {
                        Direction::North => {
                            // Edge at -Z side of sector A
                            let edge_z = world_z_a;
                            (
                                Vec3::new(world_x_a, portal_bottom_left, edge_z),              // bottom-left (NW corner)
                                Vec3::new(world_x_a + SECTOR_SIZE, portal_bottom_right, edge_z), // bottom-right (NE corner)
                                Vec3::new(world_x_a + SECTOR_SIZE, portal_top_right, edge_z),    // top-right
                                Vec3::new(world_x_a, portal_top_left, edge_z),                  // top-left
                                Vec3::new(0.0, 0.0, -1.0), // Normal points into room A (toward -Z)
                            )
                        }
                        Direction::East => {
                            // Edge at +X side of sector A
                            let edge_x = world_x_a + SECTOR_SIZE;
                            (
                                Vec3::new(edge_x, portal_bottom_left, world_z_a),              // bottom-left (NE corner)
                                Vec3::new(edge_x, portal_bottom_right, world_z_a + SECTOR_SIZE), // bottom-right (SE corner)
                                Vec3::new(edge_x, portal_top_right, world_z_a + SECTOR_SIZE),    // top-right
                                Vec3::new(edge_x, portal_top_left, world_z_a),                  // top-left
                                Vec3::new(1.0, 0.0, 0.0), // Normal points into room A (toward +X)
                            )
                        }
                        Direction::South => {
                            // Edge at +Z side of sector A
                            let edge_z = world_z_a + SECTOR_SIZE;
                            (
                                Vec3::new(world_x_a + SECTOR_SIZE, portal_bottom_left, edge_z), // bottom-left (SE corner)
                                Vec3::new(world_x_a, portal_bottom_right, edge_z),              // bottom-right (SW corner)
                                Vec3::new(world_x_a, portal_top_right, edge_z),                  // top-right
                                Vec3::new(world_x_a + SECTOR_SIZE, portal_top_left, edge_z),    // top-left
                                Vec3::new(0.0, 0.0, 1.0), // Normal points into room A (toward +Z)
                            )
                        }
                        Direction::West => {
                            // Edge at -X side of sector A
                            let edge_x = world_x_a;
                            (
                                Vec3::new(edge_x, portal_bottom_left, world_z_a + SECTOR_SIZE), // bottom-left (SW corner)
                                Vec3::new(edge_x, portal_bottom_right, world_z_a),              // bottom-right (NW corner)
                                Vec3::new(edge_x, portal_top_right, world_z_a),                  // top-right
                                Vec3::new(edge_x, portal_top_left, world_z_a + SECTOR_SIZE),    // top-left
                                Vec3::new(-1.0, 0.0, 0.0), // Normal points into room A (toward -X)
                            )
                        }
                    };

                    // Convert to room-relative coordinates and add portals to both rooms
                    // Portal in room A points to room B
                    let vertices_a = [
                        Vec3::new(v0.x - pos_a.x, v0.y - pos_a.y, v0.z - pos_a.z),
                        Vec3::new(v1.x - pos_a.x, v1.y - pos_a.y, v1.z - pos_a.z),
                        Vec3::new(v2.x - pos_a.x, v2.y - pos_a.y, v2.z - pos_a.z),
                        Vec3::new(v3.x - pos_a.x, v3.y - pos_a.y, v3.z - pos_a.z),
                    ];
                    self.rooms[room_a_idx].portals.push(Portal::new(room_b_idx, vertices_a, normal_a));

                    // Portal in room B points to room A (opposite normal)
                    let normal_b = Vec3::new(-normal_a.x, -normal_a.y, -normal_a.z);
                    let vertices_b = [
                        Vec3::new(v1.x - pos_b.x, v1.y - pos_b.y, v1.z - pos_b.z), // Swap order for opposite facing
                        Vec3::new(v0.x - pos_b.x, v0.y - pos_b.y, v0.z - pos_b.z),
                        Vec3::new(v3.x - pos_b.x, v3.y - pos_b.y, v3.z - pos_b.z),
                        Vec3::new(v2.x - pos_b.x, v2.y - pos_b.y, v2.z - pos_b.z),
                    ];
                    self.rooms[room_b_idx].portals.push(Portal::new(room_a_idx, vertices_b, normal_b));
                }
            }
        }
    }
}

/// Create an empty level with a single starter room (floor only)
/// Uses TRLE sector size (1024 units) for proper grid alignment
pub fn create_empty_level() -> Level {
    let mut level = Level::new();

    // Create a single starter room with one sector (1x1 grid)
    let mut room0 = Room::new(0, Vec3::ZERO, 1, 1);

    // Add floor at height 0
    let texture = TextureRef::new("retro-texture-pack", "FLOOR_1A");
    room0.set_floor(0, 0, 0.0, texture);

    room0.recalculate_bounds();
    level.rooms.push(room0);

    level
}

/// Create a simple test level with a fully enclosed room
/// Uses TRLE sector sizes (1024 units per sector)
pub fn create_test_level() -> Level {
    let mut level = Level::new();

    // Room 0: Single sector room (1024×1024, height 1024 = 4 clicks)
    let mut room0 = Room::new(0, Vec3::ZERO, 1, 1);

    // Floor at y=0, ceiling at y=1024
    let floor_tex = TextureRef::new("retro-texture-pack", "FLOOR_1A");
    let ceiling_tex = TextureRef::new("retro-texture-pack", "FLOOR_1A");
    let wall_tex = TextureRef::new("retro-texture-pack", "WALL_1A");

    room0.set_floor(0, 0, 0.0, floor_tex);
    room0.set_ceiling(0, 0, 1024.0, ceiling_tex);

    // Four walls around the single sector
    room0.add_wall(0, 0, Direction::North, 0.0, 1024.0, wall_tex.clone());
    room0.add_wall(0, 0, Direction::East, 0.0, 1024.0, wall_tex.clone());
    room0.add_wall(0, 0, Direction::South, 0.0, 1024.0, wall_tex.clone());
    room0.add_wall(0, 0, Direction::West, 0.0, 1024.0, wall_tex);

    room0.recalculate_bounds();
    level.add_room(room0);

    level
}
