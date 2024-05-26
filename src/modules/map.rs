//
// The map widget. This is intended to be used as a base widget for other things such as pskreporter maps, callsign maps, etc
//

use std::{collections::HashMap, f64::consts::PI, io::Cursor, time::Instant};
use anyhow::Result;
use egui::{Color32, Context, Rect, TextureHandle, TextureId, Ui, Vec2};
use geo::{Contains, Coord, Intersects};
use image::{GenericImage, ImageBuffer, ImageDecoder};
use lazy_static::lazy_static;
use log::{debug, error};
use poll_promise::Promise;
use rand::Rng;
use reqwest::Response;
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use thiserror::Error;
use tokio::runtime::Handle;
use crate::GuiConfig;
use super::gui::generate_random_id;


/// The maximum number of visible tiles. This is used to initialize hashmaps and vecs to improve frame time consistency (this is very overkill, lol)
const MAX_TILES: usize = 128;

const NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");
lazy_static! {
    // We use a custom useragent to identify our application
    /// The client used to sent requests to the tile APIs
    static ref CLIENT: reqwest::Client = reqwest::Client::builder().user_agent(format!("{NAME}/{VERSION} OSS for Amateur Radio Operators")).build().unwrap();
}


/// A map widget. This aims to be a high-performance zoomable map with support for multiple different tile providers.
/// 
/// NOTE: Initialization of the MapWidget is a little unusual. The MapWidget requires access to the [egui::Context] and [tokio::runtime::Handle],
///       which means it can't be initialized with [Default::default()] like most widgets.
///       This typically requires you to wrap the map widget into an `Option<Self>` and initialize it as soon as a frame is rendered
///       so we can get access to the egui context and the tokio runtime.
pub struct MapWidget {
    map_rect_id: egui::Id,
    /// The tile in the center of the map
    center_tile: TileId,
    /// The relative offset for the center tile in pixels
    relative_offset: Vec2,
    /// The zoom level of the map
    zoom: f32,
    /// The tilemanager system is responsible for caching and fetching any tiles that the map widget requires
    tile_manager: TileManager,
    /// The overlay manager is responsible for lazily computing an overlay for the map. This is used to draw objects on the map with good performance
    overlay_manager: MapOverlayManager,
    /// The center of the map. `center_tile` is still used for movement since it's cheaper and simpler, but it isn't very precise,
    /// so we store the center location here and re-center the map on zoom events.
    center_loc: Coord<f64>,
}
impl MapWidget {

    pub fn new(ctx: &Context, config: &mut GuiConfig) -> Self {
        let tile_manager = TileManager::new(ctx, config.runtime.handle());
        let overlay_manager = MapOverlayManager::new(ctx);

        Self {
            map_rect_id: generate_random_id(),
            center_tile: Default::default(),
            relative_offset: Default::default(),
            zoom: Default::default(),
            tile_manager,
            overlay_manager,
            center_loc: Coord::zero()
        }
    }

    /// Returns the location of the center of the map
    fn get_center_location(&self) -> Coord {
        let _span = tracy_client::span!("get_center_location");

        // Calculate the on-screen size of a tile
        let tile_size = {
            // Calculate the scaling value
            let scale_zoom = (self.zoom % 1.0) + 1.0;
            256.0 * scale_zoom as f64
        };

        // Get the width of the entire world map
        let map_size = tile_size * max_tiles(self.center_tile.zoom as u32) as f64;

        // Calculate the longitude
        let longitude = {
            // Get the tile size by dividing the offset by the tile size
            let mut center_x_pixels = self.relative_offset.x as f64 / tile_size;
            // Add the tile X coordinate
            center_x_pixels += (self.center_tile.x + 1) as f64;
            // Multiply by the tile size to get the total number of pixels in context of the world map
            center_x_pixels *= tile_size;
            // Subtract half of the tile size to compensate for some center tile offset trickery
            center_x_pixels -= tile_size / 2.0;
            
            // Calculate the longitude
            (360.0 * (center_x_pixels / map_size)) - 180.0
        };

        // Calculate the latitude
        let latitude = {
            // Get the tile size by dividing the offset by the tile size
            let mut center_y_pixels = self.relative_offset.y as f64 / tile_size;
            // Add the tile Y coordinate (+1 to account for the zero-indexing)
            center_y_pixels += (self.center_tile.y + 1) as f64;
            // Multiply by the tile size to get the total number of pixels in context of the world map
            center_y_pixels *= tile_size;
            // Subtract half of the tile size to compensate for some center tile offset trickery
            center_y_pixels -= tile_size / 2.0;

            // Calculate the latitude
            let y = convert_range(center_y_pixels, [0.0, map_size], [PI, -PI]);
            gudermannian(y)
        };

        geo::coord! { x: longitude, y: latitude }

    }

    /// Sets the map center to the provided location
    fn set_center_location(&mut self, location: Coord) {

        // Update the center location
        self.center_loc = location;

        // Calculate the tile size
        let tile_size = {
            // Calculate the scaling value
            let scale_zoom = (self.zoom % 1.0) + 1.0;
            256.0 * scale_zoom as f64
        };

        // Get the width of the entire world map at our current zoom level in tiles
        let map_max_tiles = max_tiles(self.center_tile.zoom as u32) as f64;

        // ===== LATITUDE ===== //
        // Calculate our latitude position in pixels on the world map
        let y = inverse_gudermannian(location.y);
        let mut y_pixels = convert_range(y, [PI, -PI], [0.0, map_max_tiles * tile_size]);
        // Calculate the number of tiles in the Y axis
        let y_tiles = (y_pixels / tile_size) as u32;
        // Get the remaining pixels and apply an offsset of half the tile size
        y_pixels %= tile_size;
        y_pixels -= tile_size * 0.5;

        // ===== LONGITUDE ===== //
        // Calculate the ratio of our longitude in the world map
        let x_ratio = (location.x + 180.0) / 360.0;
        // Calculate our pixel position on the world map
        let mut x_pixels = ((map_max_tiles * x_ratio) * tile_size).floor();
        // Calculate the number of tiles in the X axis
        let x_tiles = (x_pixels / tile_size) as u32;
        // Get the remaining pixels and apply an offset of half the tile size
        x_pixels %= tile_size;
        x_pixels -= tile_size * 0.5;

        // Update the map position
        self.center_tile.x = x_tiles;
        self.center_tile.y = y_tiles;
        self.relative_offset.x = x_pixels as f32;
        self.relative_offset.y = y_pixels as f32;

    }

    // TODO: Optimize this
    /// Returns a geographic rect containing the visibile area on the map
    /// 
    /// This is useful for point culling the overlay (i.e. only draw the points that are currently visible)
    fn get_visible_geo_rect(&self, map_rect: &Rect) -> geo::Rect {
        let _span = tracy_client::span!("get_visible_geo_rect");
        // let map_rect = map_rect.translate(-map_rect.left_top().to_vec2());

        // Calculate the on-screen size of a tile
        let tile_size = {
            // Calculate the scaling value
            let scale_zoom = (self.zoom % 1.0) + 1.0;
            256.0 * scale_zoom as f64
        };

        // Get the width of the entire world map in pixels at our current tile size
        let map_size = tile_size * max_tiles(self.center_tile.zoom as u32) as f64;

        let mut min = {

            // Calculate the longitude
            let longitude = {
                // Get the tile size by dividing the offset by the tile size
                let mut center_x_pixels = self.relative_offset.x as f64 / tile_size;
                // Add the tile X coordinate
                center_x_pixels += (self.center_tile.x + 1) as f64;
                // Multiply by the tile size to get the total number of pixels in context of the world map
                center_x_pixels *= tile_size;
                // Subtract half of the tile size to compensate for some center tile offset trickery
                center_x_pixels -= tile_size / 2.0;

                center_x_pixels -= map_rect.width() as f64 / 2.0;
                
                // Calculate the longitude
                (360.0 * (center_x_pixels / map_size)) - 180.0
            };

            // Calculate the latitude
            let latitude = {
                // Get the tile size by dividing the offset by the tile size
                let mut center_y_pixels = self.relative_offset.y as f64 / tile_size;
                // Add the tile Y coordinate (+1 to account for the zero-indexing)
                center_y_pixels += (self.center_tile.y + 1) as f64;
                // Multiply by the tile size to get the total number of pixels in context of the world map
                center_y_pixels *= tile_size;
                // Subtract half of the tile size to compensate for some center tile offset trickery
                center_y_pixels -= tile_size / 2.0;

                center_y_pixels -= map_rect.height() as f64 / 2.0;

                // Calculate the latitude
                let y = convert_range(center_y_pixels, [0.0, map_size], [PI, -PI]);
                gudermannian(y)
            };

            geo::coord!{ x: longitude, y: latitude}
        };
        // min.x = min.x.clamp(-180.0, 180.0);
        // min.y = min.y.clamp(-90.0, 90.0);

        let mut max = {

            // Calculate the longitude
            let longitude = {
                // Get the tile size by dividing the offset by the tile size
                let mut center_x_pixels = self.relative_offset.x as f64 / tile_size;
                // Add the tile X coordinate
                center_x_pixels += (self.center_tile.x + 1) as f64;
                // Multiply by the tile size to get the total number of pixels in context of the world map
                center_x_pixels *= tile_size;
                // Subtract half of the tile size to compensate for some center tile offset trickery
                center_x_pixels -= tile_size / 2.0;

                center_x_pixels += map_rect.width() as f64 / 2.0;
                
                // Calculate the longitude
                (360.0 * (center_x_pixels / map_size)) - 180.0
            };

            // Calculate the latitude
            let latitude = {
                // Get the tile size by dividing the offset by the tile size
                let mut center_y_pixels = self.relative_offset.y as f64 / tile_size;
                // Add the tile Y coordinate (+1 to account for the zero-indexing)
                center_y_pixels += (self.center_tile.y + 1) as f64;
                // Multiply by the tile size to get the total number of pixels in context of the world map
                center_y_pixels *= tile_size;
                // Subtract half of the tile size to compensate for some center tile offset trickery
                center_y_pixels -= tile_size / 2.0;

                center_y_pixels += map_rect.height() as f64 / 2.0;

                // Calculate the latitude
                let y = convert_range(center_y_pixels, [0.0, map_size], [PI, -PI]);
                gudermannian(y)
            };

            geo::coord!{ x: longitude, y: latitude}
        };
        // max.x = max.x.clamp(-180.0, 180.0);
        // max.y = max.y.clamp(-90.0, 90.0);

        // let visible_rect = geo::Rect::new(min, max);
        // debug!("Visible area: {visible_rect:?}");

        geo::Rect::new(min, max)
    }

    /// Render the UI layout. This doesn't implement `egui::Widget` because we also need mutable access to the `GuiConfig`
    pub fn ui(&mut self, ui: &mut Ui, config: &mut GuiConfig) -> egui::Response {

        // Allocate the ract for the entire map and add senses to it
        let (id, map_rect) = ui.allocate_space(ui.available_size());
        let response = ui.interact(map_rect, id, egui::Sense::click_and_drag());

        // Allocate a painter that only clips anything outside the map rect
        let map_painter = ui.painter_at(map_rect);

        // Calculate the tile size at the current zoom level
        let scale_zoom = (self.zoom % 1.0) + 1.0;
        let corrected_tile_size = 256.0 * scale_zoom;

        // Create the starting (center) tile
        let offset = Vec2::new(corrected_tile_size * 0.5, corrected_tile_size * 0.5);
        let center_tile_rect = Rect::from_min_size(map_rect.center() - offset - self.relative_offset, Vec2::new(corrected_tile_size, corrected_tile_size));

        // Create a hashmap that will contain the visible tiles and their corresponding rects
        let mut tiles = HashMap::with_capacity(MAX_TILES);
        // Find visible tiles using the breadth/4-way flood fill algorithm
        fill_tiles_breadth(map_rect, (self.center_tile, center_tile_rect), &mut tiles);

        // Tick the tile manager (i.e. load tiles and cleanup the cache)
        self.tile_manager.tick();

        // Iterate through each visible tile and render it
        for (tile_id, tile_rect) in tiles {

            // Get the texture id of the tile image
            let tile_tex = self.tile_manager.get_tile(&tile_id, &config.map_tile_provider);

            // Draw the tile
            map_painter.image(
                tile_tex,
                tile_rect,
                Rect::from_min_max(egui::Pos2::new(0.0, 0.0), egui::Pos2::new(1.0, 1.0)),
                Color32::WHITE
            );

        }


        // TODO: in-progress; draw map overlay

        // Get the geo rect
        let geo_rect = self.get_visible_geo_rect(&map_rect);
        // Update the map overlay
        self.overlay_manager.update_overlay(map_rect, geo_rect);

        // Draw the map overlay
        map_painter.image(
            self.overlay_manager.get_overlay(),
            map_rect,
            Rect::from_min_max(egui::Pos2::new(0.0, 0.0), egui::Pos2::new(1.0, 1.0)),
            Color32::WHITE
        );

        // Allocate a response for the map_rect. This allows us to sense for user input
        let map_response = ui.interact(map_rect, self.map_rect_id, egui::Sense::hover());

        // Display some text when the user hovers over a marker
        let mut debug_text = String::new();
        if let Some(mut hover_pos) = map_response.hover_pos() {
            // Get the first marker (if there is one)
            if let Some(point) = self.overlay_manager.hovered_objects_iter(geo_rect, map_rect, hover_pos).next() {
                debug_text.push_str(&format!("Marker at {:?}", point.location));
                
                egui::containers::show_tooltip_at_pointer(ui.ctx(), self.map_rect_id.with("_tooltip"), |ui| {
                    ui.label(format!("Location: {:.3?}", point.location));
                });

            }
        }

        // The map was dragged so update the center position
        if response.dragged() {

            self.relative_offset -= response.drag_delta();

            let half_tile_size = corrected_tile_size / 2.0;

            // Move north
            if self.relative_offset.y < -half_tile_size {
                if let Some(new_tile) = self.center_tile.north() {
                    self.center_tile = new_tile;
                    self.relative_offset.y = half_tile_size;
                } else {
                    self.relative_offset.y = -half_tile_size;
                }
            }
            // Move east
            if self.relative_offset.x > half_tile_size {
                if let Some(new_tile) = self.center_tile.east() {
                    self.center_tile = new_tile;
                    self.relative_offset.x = -half_tile_size;
                } else {
                    self.relative_offset.x = half_tile_size;
                }
            }
            // Move south
            if self.relative_offset.y > half_tile_size {
                if let Some(new_tile) = self.center_tile.south() {
                    self.center_tile = new_tile;
                    self.relative_offset.y = -half_tile_size;
                } else {
                    self.relative_offset.y = half_tile_size;
                }
            }
            // Move west
            if self.relative_offset.x < -half_tile_size {
                if let Some(new_tile) = self.center_tile.west() {
                    self.center_tile = new_tile;
                    self.relative_offset.x = half_tile_size;
                } else {
                    self.relative_offset.x = -half_tile_size;
                }
            }

            self.center_loc = self.get_center_location();

        }

        // Reset the map zoom and center when double clicked
        if response.double_clicked() {
            self.relative_offset = Vec2::new(0.0, 0.0);
            self.center_tile.zoom = 0;
            self.zoom = 0.0;
            self.set_center_location(Coord::zero());
        }

        // Hover and Zoom logic
        if let Some(_hover_pos) = response.hover_pos() {

            // Get the zoom delta (how much the user zoomed)
            let zoom_delta = ui.ctx().input(|i| i.zoom_delta());
            
            // The user zoomed in/out
            if zoom_delta != 1.0 {

                // // Store the current location so we can center on it again later
                // let loc = self.get_center_location();

                // Add the zoom delta to the zoom value
                self.zoom += (zoom_delta - 1.0) * 0.5;
                // Clamp the zoom to the 0-20 tile zoom range
                self.zoom = self.zoom.clamp(0.0, 20.0);

                // Update the tile zoom level
                // NOTE: The type conversion to u8 automatically floors the value so we don't have to do it manually
                self.center_tile.zoom = self.zoom as u8;

                // Set the center location again
                self.set_center_location(self.center_loc);

            }

        }

        // Allocate an overlay UI over the map. This is useful showing text on top of the map
        ui.allocate_ui_at_rect(map_rect, |ui| {

            // Debug info
            let debug_color = Color32::from_rgb(219, 65, 5);
            let loc = self.get_center_location();
            ui.colored_label(debug_color, format!("Current center location: {loc:?}"));
            // ui.colored_label(debug_color, format!("Position: {:?}", self.relative_offset));
            // ui.colored_label(debug_color, format!("Current center location: {loc:?}"));
            // ui.colored_label(debug_color, format!("Zoom: {}", self.zoom));
            // ui.colored_label(debug_color, format!("Relative offset: {:?}", self.relative_offset));
            // ui.colored_label(debug_color, format!("Corrected tile size: {:?}", corrected_tile_size));
            ui.colored_label(debug_color, debug_text);

            let crosshair_rect = Rect::from_center_size(map_rect.center(), Vec2::new(5.0, 5.0));
            map_painter.rect_filled(crosshair_rect, 0.0, Color32::RED);

            // let ctx = ui.ctx().clone();
            // ctx.texture_ui(ui);

            // If we are using the OpenStreetMap tile provider, add license attribution in the bottom-right of the map
            if let TileProvider::OpenStreetMap = config.map_tile_provider {
                // License attribution for OpenStreetMap in the bottom right corner of the map
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
        
                    // Create the openstreetmap hyperlink (Thanks OSM for being awesome :) )
                    ui.hyperlink_to("OpenStreetMap", "https://www.openstreetmap.org");
        
                    // Paint a background behind the hyperlink
                    map_painter.rect_filled(
                        ui.min_rect(),
                        0.0,
                        Color32::from_black_alpha(64)
                    );
        
                });
            }

        });

        response
    }
}
impl std::fmt::Debug for MapWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapWidget").field("Position", &self.relative_offset).field("Zoom", &self.zoom).finish()
    }
}


/// A struct that manages the map overlay. When given points on the map, this lazily draws the objects onto a transparent overlay, which is later drawn over the map itself.
/// 
/// This was created so we don't re-draw every point on the map every frame. This way, the points are only redrawn when the map changes
struct MapOverlayManager {
    /// A handle to the egui context. This is used for upload the overlay image to the GPU
    ctx: Context,
    /// Markers that should be drawn on the map
    markers: Vec<MapMarker>,
    /// A handle to the overlay image texture
    overlay: TextureHandle,
    /// The latest geo rect. This is used as a reference so we know if the map has changed and if we need to redraw the overlay
    geo_rect: geo::Rect<f64>,
    cached_color_image: egui::ColorImage
}
impl MapOverlayManager {

    fn new(ctx: &Context) -> Self {

        let cached_color_image = egui::ColorImage::new([256, 256], Color32::TRANSPARENT);

        // Upload the loading/error image to the GPU
        let overlay_texture = ctx.load_texture(
            "MapOverlayManager_Overlay",
            cached_color_image.clone(),
            egui::TextureOptions::LINEAR
        );

        let mut markers = Vec::with_capacity(500);
        let mut rng = rand::thread_rng();
        for _ in 0..500 {
            // objects.push((geo::coord! { x: rng.gen_range(-100.0..-90.0), y: rng.gen_range(-40.0..-30.0) }, true));
            let m = MapMarker { location: geo::coord! { x: rng.gen_range(-180.0..180.0), y: rng.gen_range(-85.0..85.0) } };
            markers.push(m);
            // markers.push((geo::coord! { x: rng.gen_range(-180.0..180.0), y: rng.gen_range(-85.0..85.0) }, true));
        }

        Self {
            ctx: ctx.clone(),
            markers,
            overlay: overlay_texture,
            geo_rect: geo::Rect::new(Coord::zero(), Coord::zero()),
            cached_color_image
        }
    }

    /// When provided with a geo rect, map rect, and a cursor hover position,
    /// this will return a iterator over the marker(s) that the cursor is hovering over.
    fn hovered_objects_iter(&self, geo_rect: geo::Rect<f64>, map_rect: egui::Rect, mut hover_pos: egui::Pos2) -> impl Iterator<Item=&MapMarker> {
        
        // Make the hover pos relative to the map rect instead of the whole window (i.e. 0px/0px is the top left of the map rect)
        hover_pos -= map_rect.left_top().to_vec2();

        // Get the width and height of the map rect
        let width = map_rect.width() as usize;
        let height = map_rect.height() as usize;

        // Get the min and max lon/lat values of the geo rect
        let (geo_min_x, geo_max_x) = (geo_rect.min().x, geo_rect.max().x);
        let (geo_min_y, geo_max_y) = (inverse_gudermannian(geo_rect.min().y), inverse_gudermannian(geo_rect.max().y));

        self.markers.iter()
        .filter(move |marker| geo_rect.intersects(&marker.location))
        .filter(move |marker| {
            // Calculate the x and y coordinates for the marker
            let x = convert_range(marker.location.x, [geo_min_x, geo_max_x], [0.0, width as f64]) as f32;
            let y = convert_range(inverse_gudermannian(marker.location.y), [geo_min_y, geo_max_y], [height as f64, 0.0]) as f32;

            // Create the marker rect
            let point_rect = egui::Rect::from_center_size(egui::Pos2 { x, y }, Vec2 { x: 8.0, y: 8.0 });

            // Check if the cursor is hovering over the marker rect
            point_rect.contains(hover_pos)
        })
        
    }

    fn update_overlay(&mut self, map_rect: egui::Rect, geo_rect: geo::Rect<f64>) {
        let _span = tracy_client::span!("Update overlay");

        // The geographically visible area hasn't changed; return early since there's no reason to redraw the overlay
        if self.geo_rect == geo_rect {
            return;
        }
        // The geographically visible area changed; update the geo rect
        else {
            self.geo_rect = geo_rect;
        }

        // Get the width and height of the map rect
        let width = map_rect.width() as usize;
        let height = map_rect.height() as usize;

        // Update the image size and resize the pixels vec to contain the number of pixels in our overlay
        self.cached_color_image.size = [width, height];
        self.cached_color_image.pixels.resize(width*height, Color32::TRANSPARENT);

        // Reset the image to transparent pixels
        // This is unsafe but it was the most performant way to reuse the same pixels vec without reallocating memory for a new image
        unsafe { std::ptr::write_bytes(self.cached_color_image.pixels.as_mut_ptr(), 0, self.cached_color_image.pixels.len()) };

        // Create the overlay image buffer using the cached image pixels
        // We call unwrap here because the only way this should fail is if the pixel buffer isn't big enough, but we resize it every time, so it's guaranteed to be the right size
        let mut image_buf: ImageBuffer<image::Rgba<u8>, &mut [u8]> = ImageBuffer::from_raw(width as u32, height as u32, self.cached_color_image.as_raw_mut()).unwrap();

        // Get the min and max lon/lat values of the geo rect
        let (geo_min_x, geo_max_x) = (geo_rect.min().x, geo_rect.max().x);
        let (geo_min_y, geo_max_y) = (inverse_gudermannian(geo_rect.min().y), inverse_gudermannian(geo_rect.max().y));

        // Iterate through the visible markers
        for point in self.markers.iter().filter(|c| geo_rect.intersects(&c.location)) {

            // Calculate the x and y coordinates for the marker
            let x = convert_range(point.location.x, [geo_min_x, geo_max_x], [0.0, width as f64]) as f32;
            let y = convert_range(inverse_gudermannian(point.location.y), [geo_min_y, geo_max_y], [height as f64, 0.0]) as f32;

            // Create the marker rect
            let point_rect = imageproc::rect::Rect::at((x - 4.0) as i32, (y - 4.0) as i32)
                .of_size(8, 8);

            // Draw the hollow rect
            imageproc::drawing::draw_hollow_rect_mut(
                &mut image_buf,
                point_rect,
                image::Rgba([255, 0, 0, 128])
            );

        }
        
        // Update the map overlay with our new image
        self.overlay.set(
            self.cached_color_image.clone(),
            egui::TextureOptions::LINEAR
        );

    }

    fn get_overlay(&self) -> TextureId {
        self.overlay.id()
    }

}


/// Breadth flood fill tiling algorithm.
/// 
/// Given a starting tile and a rect that resembles the visible area (i.e. the map),
/// this function will span out in all directions, storing all visible tiles in the provided `tiles` HashMap.
fn fill_tiles_breadth(
    map_rect: Rect,
    input_tile: (TileId, Rect),
    tiles: &mut HashMap<TileId, Rect>
) {

    // Add the input tile to the tiles hashmap if it's visible
    if map_rect.intersects(input_tile.1) && input_tile.0.is_in_range() {
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


/// The TileManager is responsible for querying the tile provider for tiles, caching tiles, and uploading tile textures onto the GPU
pub struct TileManager {
    /// A handle to the egui context. This is used for upload images (tiles) to the GPU
    ctx: Context,
    /// A handle to the tokio runtime
    handle: Handle,
    /// Our pending tile load tasks in the background
    tasks: HashMap<TileId, Promise<Result<TextureHandle>>>,
    /// The image used as a placeholder while the tile is loading, or if an error occured while loading the tile
    loading_texture: TextureHandle,
    /// Our cached tiles. These can either be successfully cached tiles or tiles that failed to load.
    /// 
    /// NOTE: The reason we cache tiles that failed to load is so we don't query the tile provider API again every frame (i.e. so we rate limit ourselves)
    tile_cache: HashMap<TileId, CachedTile>
}
impl TileManager {
    /// This is how long a tile is allowed to remain in the cache unused
    const CACHE_LIFETIME: u64 = 5;
    /// This is how often we should retry loading a tile
    const RETRY_TIME: u64 = 3;
    fn new(ctx: &Context, handle: &Handle) -> Self {

        // Upload the loading/error image to the GPU
        let loading_texture = ctx.load_texture(
            "TileManager_Loading",
            egui::ColorImage::new([256, 256], Color32::TRANSPARENT),
            egui::TextureOptions::LINEAR
        );

        Self {
            ctx: ctx.clone(),
            handle: handle.clone(),
            tasks: Default::default(),
            loading_texture,
            tile_cache: Default::default()
        }
    }

    /// Checks if any tiles have finished loading and removes expired tiles from the cache.
    /// 
    /// Call this each frame.
    fn tick(&mut self) {

        // Get the current instant
        let now = Instant::now();

        // Remove expired tiles from the cache
        self.tile_cache.retain(|_k, v| {
            match v {
                // The cached tile has expired
                CachedTile::Cached { handle: _, last_used } => now.duration_since(*last_used).as_secs() < Self::CACHE_LIFETIME,
                // The failed tile load cooldown has been met
                CachedTile::Failed { failed_at } => now.duration_since(*failed_at).as_secs() < Self::RETRY_TIME
            }
        });

        // Extract the finished tile load tasks
        let finished_tasks = self.tasks.extract_if(|_k, v| v.poll().is_ready()).map(|(k, v)| (k, v.block_and_take()));

        // Iterate through the finished tasks
        for (tile_id, tile_result) in finished_tasks {
            match tile_result {
                // The tile successfully loaded; put it in the cache
                Ok(handle) => {
                    self.tile_cache.insert(tile_id, CachedTile::Cached { handle, last_used: now });
                },
                // The tile failed to load; put the fail into the cache. This is done to add a retry cooldown
                Err(err) => {
                    error!("Failed to load tile: {err}");
                    self.tile_cache.insert(tile_id, CachedTile::Failed { failed_at: now });
                }
            }
        }

    }

    fn get_tile(&mut self, tile_id: &TileId, tile_provider: &TileProvider) -> TextureId {

        // Get the current instant
        let now = Instant::now();

        // The tile exists in the cache; if it was a successful load, return the tile texture, otherwise if we failed to load the tile, return the error texture
        if let Some(cached_tile) = self.tile_cache.get_mut(tile_id) {

            // If the tile was successfully loaded, update its last used time and return its texture,
            // otherwise return the texture for the tile load error
            // We cache failed tiles so we don't slam an API with requests when a tile load fails.
            // The failed tile will be removed from the cache by Self::tick() once the cooldown timer has ended, at which point you can retry the query.
            match cached_tile {
                CachedTile::Cached { handle, last_used } => {
                    *last_used = now;
                    handle.id()
                },
                CachedTile::Failed { failed_at: _ } => self.loading_texture.id()
            }

        }
        // The tile is still loading; return the loading texture
        else if self.tasks.contains_key(tile_id) {

            // Return the loading texture
            self.loading_texture.id()

        }
        // The tile is not in the cache or loading; add it to the load queue and return the loading texture
        else {

            // Enter the async runtime
            let _enter_guard = self.handle.enter();

            // Spawn a task to load the tile
            let promise = Promise::spawn_async(Self::get_tile_image_from_server(self.ctx.clone(), *tile_id, tile_provider.clone()));
            self.tasks.insert(*tile_id, promise);

            // Return the loading texture
            self.loading_texture.id()

        }

    }

    async fn get_tile_image_from_server(ctx: Context, tile_id: TileId, tile_provider: TileProvider) -> Result<TextureHandle> {

        // Query the tile server using the provided tile provider
        // TODO: Continue + License attribution
        let response = tile_provider.get_tile(&tile_id).await?;

        // If the API gave us an error, return it
        if response.status().is_client_error() || response.status().is_server_error() {
            // Format the error into a tile provider error with the response code and response text
            let err = Error::TileProvider(response.status(), response.text().await.map_err(Error::Request)?);
            return Err(err)?;
        }

        let response = response.bytes().await
            .map_err(Error::Request)?;

        // Create the image decoder
        let img = image::codecs::png::PngDecoder::new(Cursor::new(response))
            .map_err(Error::ImageDecoding)?;

        // Decode and read the image pixels into a 256x256x3 byte vector
        let mut pixel_data = vec![0; img.total_bytes() as usize];
        img.read_image(&mut pixel_data)
            .map_err(Error::ImageDecoding)?;
        
        // Upload the tile image to the GPU
        let tile_texture = ctx.load_texture(
            format!("TileManager_z{}_x{}_y{}", tile_id.zoom, tile_id.x, tile_id.y),
            egui::ColorImage::from_rgb([256, 256], &pixel_data),
            egui::TextureOptions::LINEAR
        );

        Ok(tile_texture)
    }
}
impl std::fmt::Debug for TileManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TileManager").field("ctx", &self.ctx).finish()
    }
}

/// The ID of a map tile
#[derive(Debug, Default, PartialEq, Clone, Copy, Eq, Hash)]
struct TileId {
    /// The X/Longitude coordinate of the tile
    x: u32,
    /// The Y/Latitude coordinate of the tile
    y: u32,
    /// The zoom level of the tile
    zoom: u8
}
impl TileId {

    /// Does this TileID correspond to an actual map tile? (i.e. is this tile in bounds of earth)
    /// 
    /// Returns false if the tile is *outside of the range of the world*
    fn is_in_range(&self) -> bool {
        // Get the maximum number of tiles in one axis
        let max_tiles = max_tiles(self.zoom as u32);

        // Return false if the tile is outside of the world range
        !(self.x >= max_tiles || self.y >= max_tiles)
    }

    /// Returns the tile north of self, if it exists
    fn north(&self) -> Option<Self> {
        let s = Self {
            x: self.x,
            y: self.y.checked_sub(1)?,
            zoom: self.zoom
        };

        s.is_in_range().then_some(s)
    }

    /// Returns the tile east of self, if it exists
    fn east(&self) -> Option<Self> {
        let s = Self {
            x: self.x + 1,
            y: self.y,
            zoom: self.zoom
        };

        s.is_in_range().then_some(s)
    }

    /// Returns the tile south of self, if it exists
    fn south(&self) -> Option<Self> {
        let s = Self {
            x: self.x,
            y: self.y + 1,
            zoom: self.zoom
        };

        s.is_in_range().then_some(s)
    }

    /// Returns the tile west of self, if it exists
    fn west(&self) -> Option<Self> {
        let s = Self {
            x: self.x.checked_sub(1)?,
            y: self.y,
            zoom: self.zoom
        };

        s.is_in_range().then_some(s)
    }

}


/// A map error
#[derive(Debug, Error)]
enum Error {
    #[error("Failed execute request: {0}")]
    Request(reqwest::Error),
    #[error("Failed to tile from the tile provider ({0}): {1}")]
    TileProvider(reqwest::StatusCode, String),
    #[error("Failed to decode the tile image: {0}")]
    ImageDecoding(image::ImageError)
}

/// The supported tile providers. These are APIs that can be used to fetch tiles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TileProvider {
    /// The OpenStreetMap API.
    OpenStreetMap,
    /// The MapBox API. This is a paid API and requires an API key. Additionally, you must specify a style owner and style name.
    /// This is the style that will be used when querying the API.
    /// 
    /// Some basic styles (owner/name):
    /// - mapbox/dark-v11
    /// - mapbox/light-v11
    /// - mapbox/navigation-night-v1
    /// - mapbox/navigation-day-v1
    MapBox { access_token: String, style_owner: String, style: String },
    /// The CartoCDN API. You must specify a basemap style name.
    /// This is the style that will be used when querying the API
    /// 
    /// Some basic styles:
    /// - dark_all
    /// - dark_only_labels
    /// - dark_nolabels
    /// - light_all
    /// - light_only_labels
    /// - light_nolabels
    /// - rastertiles/voyager
    /// 
    CartoCDN { access_token: String, style: String }
}
impl TileProvider {
    async fn get_tile(&self, tile_id: &TileId) -> Result<Response> {
        let response = match self {
            TileProvider::OpenStreetMap => {
                let url = format!("https://tile.openstreetmap.org/{}/{}/{}.png", tile_id.zoom, tile_id.x, tile_id.y);
                CLIENT.get(url).send().await.map_err(Error::Request)?
            },
            TileProvider::MapBox { access_token, style_owner, style } => {
                let url = format!("https://api.mapbox.com/styles/v1/{style_owner}/{style}/tiles/256/{}/{}/{}", tile_id.zoom, tile_id.x, tile_id.y);
                CLIENT.get(url).query(&[("access_token", &access_token)]).send().await.map_err(Error::Request)?
            },
            TileProvider::CartoCDN { access_token, style } => {
                let url = format!("https://basemaps.cartocdn.com/{style}/{}/{}/{}.png", tile_id.zoom, tile_id.x, tile_id.y);
                CLIENT.get(url).bearer_auth(access_token).send().await.map_err(Error::Request)?
            }
        };

        Ok(response)
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

/// A tile in the tile manager hashmap. This is used to keep track of tiles that are cached or failed to load. 
enum CachedTile {
    /// The tile was successfully loaded and is in the cache
    /// 
    /// This contains a handle to the texture that was allocated on the GPU along with the instant at which it was last accessed
    Cached { handle: TextureHandle, last_used: Instant },
    /// The tile failed to load, but it's in the cache to act as a retry cooldown timer
    /// 
    /// This contains the instant at which the load request failed
    Failed { failed_at: Instant }
}

/// A marker on the map
#[derive(Debug)]
pub struct MapMarker {
    /// The location of this marker
    location: Coord<f64>
}


/// Returns the maximum number of tiles in either the X or Y axis on the map at the provided zoom level.
/// 
/// NOTE: Because the map is square, the X and Y axis share the same max value, so all you have to do it provide a zoom value.
fn max_tiles(zoom: u32) -> u32 {
    let n_tiles = 4_u64.pow(zoom) as f64;
    n_tiles.sqrt() as u32
}

/// Converts a value from one range into a different value in another range
fn convert_range(val: f64, r1: [f64; 2], r2: [f64; 2]) -> f64 {
    (val - r1[0])
        * (r2[1] - r2[0])
        / (r1[1] - r1[0])
        + r2[0]
}

/// The gudermannian function. Used to convert Y pixels into a Latitude value
fn gudermannian(value: f64) -> f64 {
    value.sinh().atan() * (180.0 / PI)
}

/// The inverse gudermannian function. Used to convert a Latitude value into Y pixels
fn inverse_gudermannian(value: f64) -> f64 {
    let sign = value.signum();
    let sin = f64::sin(value * (PI / 180.0) * sign);
    sign * (f64::ln((1.0 + sin) / (1.0 - sin)) / 2.0)
}
