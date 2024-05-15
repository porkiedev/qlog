//
// The map widget. This is intended to be used as a base widget for other things such as pskreporter maps, callsign maps, etc
//

use std::{collections::HashMap, ops::Neg};

use egui::{emath::TSTransform, Color32, ColorImage, Mesh, Rect, TextureHandle, Vec2, Widget};
use geo_types::Point;
use log::debug;
use rand::Rng;


const BLANK_IMAGE_BYTES: &[u8; 564] = include_bytes!("../../blank-255-tile.png");


#[derive(Default)]
struct MapPosition(Point);
impl MapPosition {

}


#[derive(Default)]
pub struct MapWidget {
    transform: TSTransform,
    /// The tilemanager system is responsible for caching and fetching any tiles that the map widget requires
    tile_manager: TileManager,
    center: MapPosition,
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

        // Get the tile coordinates of the center tile. This serves as our starting point.
        // From here, we branch out, rendering a tile if it fits within the map rectangle
        let map_center = map_rect.center().to_vec2() - map_rect.left_top().to_vec2();
        let center_tile_coords = (self.transform.translation + map_center) / 256.0;
        let center_tile_coords = (center_tile_coords.x as u32, center_tile_coords.y as u32);

        // Get the tile coordinates of the first (top left) tile
        let first_tile_coords = self.transform.translation / 256.0;
        let first_tile_coords = (first_tile_coords.x as u32, first_tile_coords.y as u32);

        ui.label(format!("Top-left (first) tile: {:?}", first_tile_coords));

        if ui.button("Test rect scaling").clicked() {
            let r = Rect::from_min_size(map_rect.left_top(), Vec2::new(256.0, 256.0));
            debug!("Rect before: {:?}", r.size());
            debug!("Rect now: {:?}", (r * 1.1).size());
        }

        let mut tiles = Default::default();
        let start_tile = TileId { x: first_tile_coords.0, y: first_tile_coords.1, zoom: 0 };
        
        let mut tile_offset = self.transform.translation;

        if tile_offset.x.is_sign_positive() {
            tile_offset.x %= 256.0;
        }
        if tile_offset.y.is_sign_positive() {
            tile_offset.y %= 256.0;
        }

        let start_tile_rect = Rect::from_min_size(map_rect.left_top() - tile_offset, Vec2::new(256.0, 256.0));

        fill_tiles(ui.ctx(), map_rect, (start_tile, start_tile_rect), &mut tiles);

        ui.label(format!("Received {} tiles", tiles.len()));
        
        for tile in tiles {
            if let Some(tile_rect) = tile.1 {
                let rand_num: u8 = rand::thread_rng().gen();
                map_painter.rect_filled(tile_rect, 0.0, Color32::from_black_alpha(rand_num));
                // map_painter.rect_filled(tile_rect, 0.0, Color32::RED);
            }
        }



        // let mut tile_offset = self.transform.translation;
        // // tile_offset.x %= 256.0;
        // // tile_offset.y %= 256.0;
        
        // let (tile_x, tile_y) = (1_f32, 1_f32);
        // let mut tile_rect = Rect::from_min_size(map_rect.left_top(), Vec2::new(256.0 * tile_x, 256.0 * tile_y));
        // tile_rect = tile_rect.translate(-self.transform.translation);

        // // debug!("Is tile rect visible: {}", map_rect.intersects(tile_rect));
        // // debug!("Tile offset: {:?}", tile_offset);
        // ui.label(format!("Is tile rect visible: {}", map_rect.intersects(tile_rect)));
        // ui.label(format!("Tile offset: {:?}", tile_offset));

        // map_painter.rect_filled(tile_rect, 0.0, Color32::RED);


        // let mut meshes = Default::default();
        // fill_tiles(
        //     ui.ctx(),
        //     map_painter.clip_rect(),
        //     TileId { x: 0, y: 0, zoom: 0 },
        //     &mut self.tile_manager,
        //     &mut meshes
        // );

        // let ctx = ui.ctx().clone();
        // ctx.texture_ui(ui);

        // for (tile_id, tile_mesh) in &meshes {

        //     // let mesh = Mesh::with_texture(tile_mesh.id());
        //     // map_painter.add(mesh);

        //     let tl = map_rect.left_top() + Vec2::new(128.0, 128.0);
        //     let wanted_rect = Rect::from_center_size(tl, Vec2::new(256.0, 256.0));
        //     // let wanted_rect = Rect::from_two_pos(tl - Vec2::new(32.0, 32.0), tl + Vec2::new(32.0, 32.0));

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
            self.transform.translation -= response.drag_delta();
        }

        // The map was double clicked so reset the position
        if response.double_clicked() {
            debug!("Resetting map position");
            self.transform = TSTransform::default();
        }

        let transform = TSTransform::from_translation(ui.min_rect().left_top().to_vec2()) * self.transform;

        if let Some(hover_pos) = response.hover_pos() {

            // Get the zoom delta (if any)
            let zoom_delta = ui.ctx().input(|i| i.zoom_delta());
            // Zoom in/out
            self.transform = self.transform * TSTransform::from_scaling(zoom_delta);

            // Debug info
            ui.add_space(-64.0);
            ui.label(format!("Hovering at {} {}", hover_pos.x, hover_pos.y));
            ui.label(format!("Transform: {:?}", self.transform));
        }

        response
    }
}
impl std::fmt::Debug for MapWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapWidget").field("transform", &self.transform).finish()
    }
}


fn fill_tiles(
    ctx: &egui::Context,
    map_rect: Rect,
    input_tile: (TileId, Rect),
    // tile_manager: &mut TileManager,
    tiles: &mut HashMap<TileId, Option<Rect>>
    // tile_meshes: &mut HashMap<TileId, TextureHandle>
) {

    // tile_manager.get_tile_image(tile_id)
    // let tile = tile_manager.get_tile(starting_tile);
    // let projected_tile = starting_tile.

    // let tile_pos = input_tile.pixels();

    // Insert the input tile into the map
    if input_tile.0.is_in_range() && map_rect.intersects(input_tile.1) {
        tiles.insert(input_tile.0, Some(input_tile.1));
    }
    // if map_rect.intersects(input_tile.1) {
    //     tiles.insert(input_tile.0, Some(input_tile.1));
    // }

    let next_tile_rect = input_tile.1;
    if let Some(next_tile_id) = input_tile.0.north() {
        if tiles.get(&next_tile_id).is_none() {

            let next_tile_rect = next_tile_rect.translate(Vec2::new(0.0, -256.0));
            if map_rect.intersects(next_tile_rect) {
                fill_tiles(ctx, map_rect, (next_tile_id, next_tile_rect), tiles);
            }
            
        }
    }
    if let Some(next_tile_id) = input_tile.0.east() {
        if tiles.get(&next_tile_id).is_none() {

            let next_tile_rect = next_tile_rect.translate(Vec2::new(256.0, 0.0));
            if map_rect.intersects(next_tile_rect) {
                fill_tiles(ctx, map_rect, (next_tile_id, next_tile_rect), tiles);
            }
            
        }
    }
    if let Some(next_tile_id) = input_tile.0.south() {
        if tiles.get(&next_tile_id).is_none() {

            let next_tile_rect = next_tile_rect.translate(Vec2::new(0.0, 256.0));
            if map_rect.intersects(next_tile_rect) {
                fill_tiles(ctx, map_rect, (next_tile_id, next_tile_rect), tiles);
            }
            
        }
    }
    if let Some(next_tile_id) = input_tile.0.west() {
        if tiles.get(&next_tile_id).is_none() {

            let next_tile_rect = next_tile_rect.translate(Vec2::new(-256.0, 0.0));
            if map_rect.intersects(next_tile_rect) {
                fill_tiles(ctx, map_rect, (next_tile_id, next_tile_rect), tiles);
            }
            
        }
    }

    // let next_tile_rect = input_tile.1;
    // if let Some(next_tile_id) = input_tile.0.north() {
    //     let next_tile_rect = next_tile_rect.translate(Vec2::new(0.0, -256.0));
    //     if map_rect.intersects(next_tile_rect) {
    //         fill_tiles(ctx, map_rect, (next_tile_id, next_tile_rect), tiles);
    //     }
    // }
    // else if let Some(next_tile_id) = input_tile.0.east() {
    //     let next_tile_rect = next_tile_rect.translate(Vec2::new(256.0, 0.0));
    //     if map_rect.intersects(next_tile_rect) {
    //         fill_tiles(ctx, map_rect, (next_tile_id, next_tile_rect), tiles);
    //     }
    // }
    // else if let Some(next_tile_id) = input_tile.0.south() {
    //     let next_tile_rect = next_tile_rect.translate(Vec2::new(0.0, 256.0));
    //     if map_rect.intersects(next_tile_rect) {
    //         fill_tiles(ctx, map_rect, (next_tile_id, next_tile_rect), tiles);
    //     }
    // }
    // else if let Some(next_tile_id) = input_tile.0.west() {
    //     let next_tile_rect = next_tile_rect.translate(Vec2::new(-256.0, 0.0));
    //     if map_rect.intersects(next_tile_rect) {
    //         fill_tiles(ctx, map_rect, (next_tile_id, next_tile_rect), tiles);
    //     }
    // }

    // // If the tile mesh is already cached
    // if let Some(mesh) = tile_meshes.get(&input_tile) {
    //     // mesh.transform(transform)
    // } else {
    //     let texture_handle = ctx.load_texture(format!("{:?}", input_tile), tile_manager.get_tile_image(&input_tile), egui::TextureOptions::LINEAR);
    //     // let mesh = Mesh::with_texture(texture_handle.id());
    //     tile_meshes.insert(input_tile, texture_handle);
    // }

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
    const TILE_SIZE: u32 = 256;

    /// Returns the coordinates of the top-left corner of the tile in pixels
    fn pixels(&self) -> egui::Pos2 {
        let x = (self.x * Self::TILE_SIZE) as f32;
        let y = (self.y * Self::TILE_SIZE) as f32;
        egui::Pos2 { x, y }
    }

    /// Does this TileID correspond to an actual map tile? (i.e. is this tile in bounds of earth)
    /// 
    /// Returns false if the tile is *outside of the range of the world*
    fn is_in_range(&self) -> bool {
        // Get the maximum number of tiles in either direction
        // TODO: This number needs to change with the zoom level
        let max_tiles = 4 / 2;

        // Return false if the tile is outside of the world range
        !(self.x > max_tiles || self.y > max_tiles)
    }

    fn north(&self) -> Option<Self> {
        let s = Self {
            x: self.x,
            y: self.y.checked_sub(1)?,
            zoom: self.zoom
        };

        if s.is_in_range() {
            Some(s)
        } else {
            None
        }
    }

    fn east(&self) -> Option<Self> {
        let s = Self {
            x: self.x + 1,
            y: self.y,
            zoom: self.zoom
        };

        if s.is_in_range() {
            Some(s)
        } else {
            None
        }
    }

    fn south(&self) -> Option<Self> {
        let s = Self {
            x: self.x,
            y: self.y + 1,
            zoom: self.zoom
        };

        if s.is_in_range() {
            Some(s)
        } else {
            None
        }
    }

    fn west(&self) -> Option<Self> {
        let s = Self {
            x: self.x.checked_sub(1)?,
            y: self.y,
            zoom: self.zoom
        };

        if s.is_in_range() {
            Some(s)
        } else {
            None
        }
    }

}
