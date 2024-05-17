//
// The map widget. This is intended to be used as a base widget for other things such as pskreporter maps, callsign maps, etc
//

use std::{collections::HashMap, ops::Neg};

use egui::{emath::TSTransform, Color32, ColorImage, Mesh, Rect, TextureHandle, Vec2, Widget};
use geo_types::Point;
use log::{debug, error};
use rand::Rng;
use strum::IntoEnumIterator;


/// The maximum number of visible tiles. This is used to initialize hashmaps and vecs to improve frame time consistency (this is very overkill, lol)
const MAX_TILES: usize = 128;
const BLANK_IMAGE_BYTES: &[u8; 564] = include_bytes!("../../blank-255-tile.png");


/// A location on the map
#[derive(Default)]
struct MapLocation {
    lat: f64,
    lon: f64
}
impl MapLocation {

}


#[derive(Default)]
pub struct MapWidget {
    center_tile: TileId,
    relative_position: Vec2,
    zoom: f32,
    /// The tilemanager system is responsible for caching and fetching any tiles that the map widget requires
    tile_manager: TileManager,
    /// Where the map is centered at
    center: MapLocation,
    texture_handle: Option<egui::TextureHandle>
}
impl MapWidget {
    fn load_texture(&mut self, ctx: &egui::Context) {

        // let pixels: Vec<u8> = vec![255; 256*256*3];
        // let color_image = egui::ColorImage::from_rgb([256, 256], &pixels);
        let color_image = egui::ColorImage::example();

        let texture_handle = ctx.load_texture("test-texture", color_image, egui::TextureOptions::LINEAR);
        self.texture_handle = Some(texture_handle);

    }
}
impl Widget for &mut MapWidget {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        
        // Test load texture button
        if ui.button("Load texture").clicked() {
            self.load_texture(ui.ctx());
        }

        // Allocate the ract for the entire map and add senses to it
        let (id, mut map_rect) = ui.allocate_space(ui.available_size());
        let response = ui.interact(map_rect, id, egui::Sense::click_and_drag());

        // Allocate a painter that only clips anything outside the map rect
        let map_painter = ui.painter_at(map_rect);

        let corrected_tile_size = 256.0 * (self.zoom + 1.0);

        // Get the tile coordinates of the center tile. This serves as our starting point.
        // From here, we branch out, rendering a tile if it fits within the map rectangle
        let map_center = map_rect.center().to_vec2() - map_rect.left_top().to_vec2();
        let center_tile_coords = (self.relative_position + map_center) / corrected_tile_size;
        let center_tile_coords = center_tile_coords.floor();
        let mut center_tile_coords = (center_tile_coords.x as u32, center_tile_coords.y as u32);

        let start_tile = self.center_tile;

        // map_center
        // let start_tile_rect = Rect::from_min_size(map_rect.left_top() - tile_offset, Vec2::new(corrected_tile_size, corrected_tile_size));
        let start_tile_rect = Rect::from_center_size(map_rect.center() - self.relative_position, Vec2::new(corrected_tile_size, corrected_tile_size));

        let mut tiles = HashMap::with_capacity(MAX_TILES);
        // fill_tiles(ui.ctx(), map_rect, (start_tile, start_tile_rect), &mut tiles);
        {
            let _span = tracy_client::span!("Fill tiles");
            fill_tiles_breadth(ui.ctx(), map_rect, (start_tile, start_tile_rect), &mut tiles);
        }

        ui.label(format!("Rendering {} tiles", tiles.len()));
        ui.label(format!("Center tile: {:?}", start_tile));
        ui.label(format!("Center coords with offset: {:?}", map_rect.center() - self.relative_position));

        // map_painter.rect_filled(start_tile_rect, 0.0, Color32::from_black_alpha(125));

        for tile in tiles {
            // if let Some(tile_rect) = tile.1 {
                // let rand_num: u8 = rand::thread_rng().gen();
                if tile.0.x == 4 && tile.0.y == 4 {
                    map_painter.rect_filled(tile.1, 0.0, Color32::RED);
                } else {
                    map_painter.rect_filled(tile.1, 0.0, Color32::from_white_alpha(((tile.0.x + tile.0.y) as u8).wrapping_mul(10)));
                }
                // map_painter.rect_filled(tile_rect, 0.0, Color32::RED);
            // }
        }

        // {
        //     let center_x = (map_center + tile_offset).x;
        //     let max_x = start_tile.max_tiles() as f32 * corrected_tile_size;
        //     let max_flattened_lat = 360.0;
            
        //     let lat = (360.0 * (center_x / max_x)) - 180.0;

        //     debug!("Center latitude (x) is {lat}\n{center_x}/{max_x}");

        // }

        // let ctx = ui.ctx().clone();
        // ctx.texture_ui(ui);

        // for (tile_id, tile_mesh) in &meshes {

        //     // let mesh = Mesh::with_texture(tile_mesh.id());
        //     // map_painter.add(mesh);

        //     egui::paint_texture_at(
        //         &map_painter,
        //         wanted_rect,
        //         &egui::ImageOptions::default(),
        //         &egui::load::SizedTexture::from_handle(tile_mesh)
        //     )

        //     // map_painter.image(
        //     //     texture_id,
        //     //     rect,
        //     //     uv,
        //     //     tint
        //     // )

        //     // egui::Image::from_texture(egui::load::SizedTexture::from_handle(tile_mesh)).fit_to_exact_size(egui::vec2(256.0, 256.0)).maintain_aspect_ratio(false)
        //     // .paint_at(ui, map_rect);
        // }

        // ui.painter().add(egui::Mesh::with_texture(tex_handle.id()));

        // // If the tile is visible, render it
        // if ui.is_rect_visible(main_rect) {
        //     if let Some(tex_handle) = &self.texture_handle {

        //         egui::paint_texture_at(
        //             ui.painter(),
        //             main_rect,
        //             &egui::ImageOptions::default(),
        //             &egui::load::SizedTexture::from_handle(tex_handle)
        //         )

        //     }
        // }

        // if let Some(tex_handle) = &self.texture_handle {
        //     let sized_tex = egui::load::SizedTexture::from_handle(tex_handle);
        //     // ui.image(sized_tex);
        //     egui::Image::from_texture(sized_tex)
        //     .fit_to_exact_size(egui::vec2(128.0, 128.0))
        //     .maintain_aspect_ratio(false)
        //     .ui(ui);
        // }


        // The map was dragged so update the center position
        if response.dragged() {
            self.relative_position -= response.drag_delta();

            if self.relative_position.y < -corrected_tile_size {
                debug!("Moving north");
                if let Some(new_tile) = self.center_tile.north() {
                    self.center_tile = new_tile;
                    self.relative_position.y %= corrected_tile_size;
                } else {
                    self.relative_position.y = -corrected_tile_size;
                }
            }

            if self.relative_position.x > corrected_tile_size {
                debug!("Moving east");
                if let Some(new_tile) = self.center_tile.east() {
                    self.center_tile = new_tile;
                    self.relative_position.x %= corrected_tile_size;
                } else {
                    self.relative_position.x = corrected_tile_size;
                }
            }

            if self.relative_position.y > corrected_tile_size {
                debug!("Moving south");
                if let Some(new_tile) = self.center_tile.south() {
                    self.center_tile = new_tile;
                    self.relative_position.y %= corrected_tile_size;
                } else {
                    self.relative_position.y = corrected_tile_size;
                }
            }

            if self.relative_position.x < -corrected_tile_size {
                debug!("Moving west");
                if let Some(new_tile) = self.center_tile.west() {
                    self.center_tile = new_tile;
                    self.relative_position.x %= corrected_tile_size;
                } else {
                    self.relative_position.x = -corrected_tile_size;
                }
            }

        }

        // The map was double clicked so reset the position
        if response.double_clicked() {
            debug!("Resetting map position");
            self.relative_position = Vec2::new(0.0, 0.0);
            self.zoom = 0.0;
        }

        if let Some(hover_pos) = response.hover_pos() {

            // Get the zoom delta (if any)
            let zoom_delta = ui.ctx().input(|i| i.zoom_delta());
            // Zoom in/out
            self.zoom += (zoom_delta - 1.0) * 0.5;
            if zoom_delta != 1.0 {
                let new_tile_size = 256.0 * (self.zoom + 1.0);

                // self.position *= corrected_tile_size / new_tile_size;

                // let x = corrected_tile_size / new_tile_size * first_tile_coords.0 as f32;
                // let y = corrected_tile_size / new_tile_size * first_tile_coords.1 as f32;
                // self.position += Vec2::new(x, y);

                let m = (start_tile.max_tiles() as f32 * corrected_tile_size) / (start_tile.max_tiles() as f32 * new_tile_size);
                self.relative_position *= m;
            }

            // Debug info
            ui.add_space(-98.0);
            ui.label(format!("Hovering at {} {}", hover_pos.x, hover_pos.y));
            ui.label(format!("Position: {:?}", self.relative_position));
            ui.label(format!("Zoom: {}", self.zoom));
        }

        response
    }
}
impl std::fmt::Debug for MapWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapWidget").field("Position", &self.relative_position).field("Zoom", &self.zoom).finish()
    }
}


/// Breadth flood fill tiling algorithm.
/// 
/// Given a starting tile and a rect that resembles the visible area (i.e. the map),
/// this function will span out in all directions, storing all visible tiles in the provided `tiles` HashMap.
fn fill_tiles_breadth(
    ctx: &egui::Context,
    map_rect: Rect,
    input_tile: (TileId, Rect),
    tiles: &mut HashMap<TileId, Rect>
) {

    // Add the input tile to the tiles hashmap if it's visible
    if map_rect.intersects(input_tile.1) {
        tiles.insert(input_tile.0, input_tile.1);
    } else {
        return;
    }

    // Get the width/height (1:1 aspect ratio) of the input tile
    let tile_size = input_tile.1.width();

    // Create the edge tile hashmap
    let mut edge_tiles = tiles.clone();

    // Continue loading tiles until we're done, or until we hit a soft max of 128 tiles
    // Note: I say 'soft max' because there may still be some remaining tiles to load.
    // The total number of tiles can be slightly more than 128.
    while !edge_tiles.is_empty() {

        // Iterate through the edge tiles
        for (current_tile, current_tile_rect) in std::mem::take(&mut edge_tiles) {

            // If we hit our maximum number of tiles, break out
            if tiles.len() == MAX_TILES {
                break;
            }
            
            // Insert the current tile into the tiles hashmap
            tiles.insert(current_tile, current_tile_rect);

            // Try to load one tile in every direction
            // This zips the resulting TileId up with a TileDirection, and filters any tiles that are out of bounds
            for (next_tile_id, next_tile_direction) in [
                current_tile.north(),
                current_tile.east(),
                current_tile.south(),
                current_tile.west()
            ].into_iter()
            .zip(TileDirection::iter())
            .filter_map(|(a, b)| Some((a?, b))) {

                // Skip this tile if it has already been loaded into the hashmap
                if tiles.contains_key(&next_tile_id) || edge_tiles.contains_key(&next_tile_id) {
                    continue;
                }
                
                // Where should we translate/move the rect for the next tile?
                let next_tile_translation = match next_tile_direction {
                    TileDirection::North => Vec2::new(0.0, -tile_size),
                    TileDirection::East => Vec2::new(tile_size, 0.0),
                    TileDirection::South => Vec2::new(0.0, tile_size),
                    TileDirection::West => Vec2::new(-tile_size, 0.0)
                };

                // Translate/move the rect for the new tile
                let next_tile_rect = current_tile_rect.translate(next_tile_translation);

                // If the tile would be visible on the map, push it to the edge_tiles hashmap
                if map_rect.intersects(next_tile_rect) {
                    edge_tiles.insert(next_tile_id, next_tile_rect);
                }

            }

        }

    }

}


/// The direction of the next Tile
#[derive(strum_macros::EnumIter)]
enum TileDirection {
    North,
    East,
    South,
    West
}


#[derive(Debug, Default)]
struct TileManager;
impl TileManager {
    /// Returns the pixels of a tile.
    /// 
    /// NOTE: This is just the image. You are still responsible for allocating this texture, and caching that texture until it's no longer needed.
    fn get_tile_image(&mut self, tile_id: &TileId) -> ColorImage {

        let pixels: Vec<u8> = vec![255; 256*256*3];
        ColorImage::from_rgb([256, 256], &pixels)
        // egui::ColorImage::example()

    }
}

/// The ID of a map tile
#[derive(Debug, PartialEq, Clone, Copy, Eq, Hash)]
struct TileId {
    x: u32,
    y: u32,
    zoom: u8
}
impl TileId {

    /// Returns the coordinates of the top-left corner of the tile in pixels
    fn pixels(&self, tile_size: u32) -> egui::Pos2 {
        let x = (self.x * tile_size) as f32;
        let y = (self.y * tile_size) as f32;
        egui::Pos2 { x, y }
    }

    /// Returns the maximum number of tiles in one direction at the current zoom level.
    /// 
    /// The map is a square so this value is the same for the X and Y dimension, hence why only one value is returned.
    fn max_tiles(&self) -> u32 {
        let n_tiles = 4_u32.pow(self.zoom as u32) as f32;
        n_tiles.sqrt() as u32
    }

    /// Does this TileID correspond to an actual map tile? (i.e. is this tile in bounds of earth)
    /// 
    /// Returns false if the tile is *outside of the range of the world*
    fn is_in_range(&self) -> bool {
        // Get the maximum number of tiles in either direction
        // TODO: This number needs to change with the zoom level
        let max_tiles = self.max_tiles();

        // Return false if the tile is outside of the world range
        !(self.x >= max_tiles || self.y >= max_tiles)
    }

    fn north(&self) -> Option<Self> {
        let s = Self {
            x: self.x,
            y: self.y.checked_sub(1)?,
            zoom: self.zoom
        };

        s.is_in_range().then_some(s)
    }

    fn east(&self) -> Option<Self> {
        let s = Self {
            x: self.x + 1,
            y: self.y,
            zoom: self.zoom
        };

        s.is_in_range().then_some(s)
    }

    fn south(&self) -> Option<Self> {
        let s = Self {
            x: self.x,
            y: self.y + 1,
            zoom: self.zoom
        };

        s.is_in_range().then_some(s)
    }

    fn west(&self) -> Option<Self> {
        let s = Self {
            x: self.x.checked_sub(1)?,
            y: self.y,
            zoom: self.zoom
        };

        s.is_in_range().then_some(s)
    }

}
impl Default for TileId {
    fn default() -> Self {
        Self {
            x: Default::default(),
            y: Default::default(),
            zoom: 6
        }
    }
}
