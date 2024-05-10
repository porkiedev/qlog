//
// The map widget. This is intended to be used as a base widget for other things such as pskreporter maps, callsign maps, etc
//

use std::{collections::HashMap, ops::Neg};

use egui::{emath::TSTransform, Color32, ColorImage, Mesh, Rect, TextureHandle, Vec2, Widget};
use geo_types::Point;
use log::debug;


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

        ui.label(format!("Center tile: {:?}", center_tile_coords));
        ui.label(format!("Map rect: {:?}", map_rect.translate(Vec2::new(256.0, 256.0))));

        if ui.button("Bound test").clicked() {
            let tile_rect_center = map_rect.center();
            let tile_rect = Rect::from_center_size(tile_rect_center, Vec2::new(256.0, 256.0));
            let tile_rect = tile_rect.translate(Vec2::new(256.0*4.2, 0.0));

            debug!("Is tile rect visible: {}", map_rect.intersects(tile_rect));

            let mut center_tile_offset = (self.transform.translation + map_center);
            center_tile_offset.x %= 256.0;
            center_tile_offset.y %= 256.0;
            debug!("Center tile offset: {:?}", center_tile_offset);

            // map_painter.add(tile_rect);
            map_painter.rect_filled(tile_rect, 0.0, Color32::RED);
        }

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
    input_tile: TileId,
    tile_manager: &mut TileManager,
    tile_meshes: &mut HashMap<TileId, TextureHandle>
) {

    // tile_manager.get_tile_image(tile_id)
    // let tile = tile_manager.get_tile(starting_tile);
    // let projected_tile = starting_tile.

    let tile_pos = input_tile.pixels();

    // If the tile mesh is already cached
    if let Some(mesh) = tile_meshes.get(&input_tile) {
        // mesh.transform(transform)
    } else {
        let texture_handle = ctx.load_texture(format!("{:?}", input_tile), tile_manager.get_tile_image(&input_tile), egui::TextureOptions::LINEAR);
        // let mesh = Mesh::with_texture(texture_handle.id());
        tile_meshes.insert(input_tile, texture_handle);
    }

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
#[derive(Debug, PartialEq, Eq, Hash)]
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

    fn north(&self) -> Option<Self> {
        Some(Self {
            x: self.x,
            y: self.y.checked_sub(1)?,
            zoom: self.zoom
        })
    }

    fn east(&self) -> Option<Self> {
        Some(Self {
            x: self.x + 1,
            y: self.y,
            zoom: self.zoom
        })
    }

    fn south(&self) -> Option<Self> {
        Some(Self {
            x: self.x,
            y: self.y + 1,
            zoom: self.zoom
        })
    }

    fn west(&self) -> Option<Self> {
        Some(Self {
            x: self.x.checked_sub(1)?,
            y: self.y,
            zoom: self.zoom
        })
    }

}
