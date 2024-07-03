//
// Contains code belonging to the band allocations chart tab
//


use egui::{Align2, Color32, FontId, Pos2, Stroke, Vec2, Vec2b, Widget};
use egui_extras::Column;
use serde::{Deserialize, Serialize};
use crate::modules::{gui::{self, frequency_formatter, frequency_formatter_no_unit}, types::convert_range};


/// The frequency allocations chart tab
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BandAllocationsTab {
    /// The selected band allocations
    selected_band_allocations: BandAllocations
}
impl gui::Tab for BandAllocationsTab {
    fn id(&self) -> egui::Id {
        egui::Id::new("band_allocations_tab")
    }

    fn title(&mut self) -> egui::WidgetText {
        "Band Allocations".into()
    }

    fn ui(&mut self, config: &mut crate::GuiConfig, ui: &mut egui::Ui) {

        let band = Band {
            name: "20M",
            start: 14_000_000,
            end: 14_350_000,
            chunks: vec![
                BandChunk {
                    start: 14_025_000,
                    end: 14_150_000,
                    color: Color32::RED
                },
                BandChunk {
                    start: 14_225_000,
                    end: 14_350_000,
                    color: Color32::GREEN
                }
            ],
            markers: vec![
                BandMarker {
                    freq: 14_074_000,
                    text: "FT8"
                },
                BandMarker {
                    freq: 14_070_000,
                    text: "Digital"
                }
            ]
        };

        // Show the band allocations chart
        BandAllocationWidget { band: &band }.ui(ui);

        // Create a table to show the band's frequencies of interest
        egui_extras::TableBuilder::new(ui)
        .column(Column::auto().at_least(86.0).clip(true).resizable(true)) // Frequency column
        .column(Column::remainder().at_least(94.0).clip(true).resizable(true)) // Description column
        .striped(true)
        .header(20.0, |mut header| {
            // Frequency column
            header.col(|ui| {
                ui.heading("Frequency");
            });
            // Description column
            header.col(|ui| {
                ui.heading("Description");
            });
        }).body(|mut body| {
            // Show a row for each marker
            body.rows(18.0, band.markers.len(), |mut row| {

                // Get the marker
                // This is safe because the rows method knows the length of the markers vec
                let marker = &band.markers[row.index()];
                
                // Frequency column
                row.col(|ui| {
                    ui.label(frequency_formatter_no_unit(marker.freq as f64));
                });

                // Description column
                row.col(|ui| {
                    ui.label(marker.text);
                });

            });
        });

    }
}
impl Default for BandAllocationsTab {
    fn default() -> Self {
        Self {
            selected_band_allocations: BandAllocations::UnitedStates
        }
    }
}

/// The widget for the band allocations chart
struct BandAllocationWidget<'a> {
    band: &'a Band
}
impl BandAllocationWidget<'_> {
    /// The height of the band bar
    const BAND_HEIGHT: f32 = 20.0;
    /// The spacing between labels
    const LABEL_SPACING: f32 = 2.0;
}
impl Widget for BandAllocationWidget<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        
        // Allocate an id and rect for the widget. This occupies the entire available space and is resized later to the minimum required size
        let (id, mut rect) = ui.allocate_space(ui.available_size());
        // Allocate a response for the widget
        let response = ui.interact(rect, id, egui::Sense::hover());
        // Allocate a painter
        let painter = ui.painter();

        // The available size for the widget
        let size = rect.size();
        // The start position for the first band. This is modified by each band so that each band drawn below the previous band
        let mut start_pos = rect.left_top();
        // Get the font id for heading text
        let heading_font = egui::TextStyle::Heading.resolve(ui.style());
        // Get the color for heading text
        let heading_color = ui.style().visuals.strong_text_color();
        // Get the font id for regular text
        let body_font = egui::TextStyle::Body.resolve(ui.style());
        // The the color for regular text
        let body_color = ui.style().visuals.text_color();

        // ===== Render the band ===== //
        // A vec containing rects for all the rendered objects. Used for collision detection and final widget sizing (we have to tell the GUI how big the widget is)
        let mut rects = Vec::new();

        // Draw the text for the band name
        let r = painter.text(
            start_pos,
            Align2::LEFT_TOP,
            self.band.name,
            heading_font.clone(),
            heading_color
        );
        // Apply the height offset to the start position
        start_pos.y += r.height();
        // Add the rect to the rects vec
        rects.push(r);

        // Draw the text for the start frequency
        let r = painter.text(
            start_pos,
            Align2::LEFT_TOP,
            frequency_formatter(self.band.start as f64, 0..=0),
            body_font.clone(),
            body_color
        );
        // Add the rect to the rects vec
        rects.push(r);

        // Draw the text for the end frequency
        let r = painter.text(
            start_pos + Vec2::new(size.x, 0.0),
            Align2::RIGHT_TOP,
            frequency_formatter(self.band.end as f64, 0..=0),
            body_font.clone(),
            body_color
        );
        // Apply the height offset to the start position
        start_pos.y += r.height();
        // Add the rect to the rects vec
        rects.push(r);

        // Draw the horizontal line from the start to the end of the band
        painter.hline(
            start_pos.x..=start_pos.x + size.x,
            start_pos.y + (Self::BAND_HEIGHT / 2.0),
            Stroke::new(2.0, body_color)
        );

        // Draw the vertical line at the start of the band
        painter.vline(
            start_pos.x,
            start_pos.y..=start_pos.y + Self::BAND_HEIGHT,
            Stroke::new(2.0, body_color)
        );

        // Draw the vertical line at the end of the band
        painter.vline(
            start_pos.x + size.x,
            start_pos.y..=start_pos.y + Self::BAND_HEIGHT,
            Stroke::new(2.0, body_color)
        );

        // Add the band to the rects vec. We subtract 2 from the height otherwise the text thinks it's colliding with the band when it isn't
        rects.push(egui::Rect::from_min_size(
            start_pos + Vec2::new(0.0, 1.0),
            Vec2::new(size.x, 18.0)
        ));

        // ===== Render the band allocation chunks ===== //

        // Iterate over the band chunks and render them
        for chunk in &self.band.chunks {

            // Calculate the start and end X coordinates for the chunk
            let start_x = convert_range(
                chunk.start as f32,
                [self.band.start as f32, self.band.end as f32],
                [start_pos.x + 2.0, start_pos.x + size.x - 2.0]
            );
            let end_x = convert_range(
                chunk.end as f32,
                [self.band.start as f32, self.band.end as f32],
                [start_pos.x + 2.0, start_pos.x + size.x - 2.0]
            );

            // Paint a partially transparent line for the chunk
            painter.hline(
                start_x..=end_x,
                start_pos.y + 10.0,
                Stroke::new(17.5, chunk.color.gamma_multiply(0.25))
            );

            // Create the start label if the start frequency is not the same as the band start frequency
            if chunk.start != self.band.start {

                // Create the text layout for the chunk start label
                let text_layout = painter.layout_no_wrap(
                    frequency_formatter_no_unit(chunk.start as f64),
                    body_font.clone(),
                    body_color
                );

                // Calculate the start coordinate for the start label
                let text_rect_pos = Pos2::new(
                    start_x - text_layout.rect.width() / 2.0,
                    start_pos.y - text_layout.rect.height()
                );

                // Create the rectangle that contains the text
                let mut text_rect = egui::Rect::from_min_size(text_rect_pos, text_layout.size());

                // Check to ensure that the text is visible (i.e. not off the screen)
                if !rect.contains_rect(text_rect) {
                    // Calculate the right edge of the text rect
                    let right = rect.left() + text_rect.width();
                    // Set the left and right edges of the text rect
                    text_rect.set_left(rect.left());
                    text_rect.set_right(right);
                }

                // Check for collisions with other text
                if rects.iter().any(|r| r.intersects(text_rect)) {

                    // Apply the initial offset to the text rect
                    text_rect = text_rect.translate(Vec2::new(0.0, 20.0 + text_rect.height()));

                    // Keep applying offsets until there are no collisions
                    while rects.iter().any(|r| r.intersects(text_rect)) {

                        // Apply the offset to the text rect
                        text_rect = text_rect.translate(Vec2::new(0.0, text_rect.height() + Self::LABEL_SPACING));

                    }

                }

                // Paint the text
                painter.galley(
                    text_rect.left_top(),
                    text_layout,
                    Color32::GOLD
                );

                // Push the text rect to the vec of text rects
                rects.push(text_rect);

            }

            // Create the end label if the end frequency is not the same as the band end frequency
            if chunk.end != self.band.end {

                // Create the text layout for the chunk end label
                let text_layout = painter.layout_no_wrap(
                    frequency_formatter_no_unit(chunk.end as f64),
                    body_font.clone(),
                    body_color
                );

                // Calculate the start coordinate for the end label
                let text_rect_pos = Pos2::new(
                    end_x - text_layout.rect.width() / 2.0,
                    start_pos.y - text_layout.rect.height()
                );

                // Create the rectangle that contains the text
                let mut text_rect = egui::Rect::from_min_size(text_rect_pos, text_layout.size());

                // Check to ensure that the text is visible (i.e. not off the screen)
                if !rect.contains_rect(text_rect) {
                    // Calculate the left edge of the text rect
                    let left = rect.right() - text_rect.width();
                    // Set the left and right edges of the text rect
                    text_rect.set_left(left);
                    text_rect.set_right(rect.right());
                }

                // Check for collisions with other text
                if rects.iter().any(|r| r.intersects(text_rect)) {

                    // Apply the initial offset to the text rect
                    text_rect = text_rect.translate(Vec2::new(0.0, 20.0 + text_rect.height()));

                    // Keep applying offsets until there are no collisions
                    while rects.iter().any(|r| r.intersects(text_rect)) {

                        // Apply the offset to the text rect
                        text_rect = text_rect.translate(Vec2::new(0.0, text_rect.height() + Self::LABEL_SPACING));

                    }

                }

                // Paint the text
                painter.galley(
                    text_rect.left_top(),
                    text_layout,
                    Color32::GOLD
                );

                // Push the text rect to the vec of text rects
                rects.push(text_rect);

            }

        }

        // Determine the max Y value of the rects. This is used so we can tell the GUI how big the widget is (and consequently, where to put the cursor)
        let mut max_y = 0.0;
        for rect in &rects {
            if rect.bottom() > max_y {
                max_y = rect.bottom();
            }
        }

        // Update the bottom of the rect to the max Y value and advance the UI cursor to the end of the rect
        rect.set_bottom(max_y);
        ui.advance_cursor_after_rect(rect);

        response
    }
}

struct Band {
    /// The name of the band
    name: &'static str,
    /// The start frequency of the band
    start: u64,
    /// The end frequency of the band
    end: u64,
    /// Allocated chunks of the band
    chunks: Vec<BandChunk>,
    /// The markers for the band
    markers: Vec<BandMarker>
}
struct BandChunk {
    /// The start frequency of the chunk
    start: u64,
    /// The end frequency of the chunk
    end: u64,
    /// The color of the chunk
    color: Color32
}
struct BandMarker {
    /// The frequency of the marker
    freq: u64,
    /// The description of the marker
    text: &'static str,
}


#[derive(Debug, Serialize, Deserialize)]
enum BandAllocations {
    UnitedStates,
    OtherPlace
}
impl BandAllocations {
    fn band_allocations_iter(&self) -> impl Iterator<Item = BandAllocation> {

        match self {
            BandAllocations::UnitedStates => [
                BandAllocation { name: "20M", frange: 14_000_000..=14_350_000 },
                BandAllocation { name: "40M", frange: 7_000_000..=7_300_000 },
                BandAllocation { name: "80M", frange: 3_500_000..=4_000_000 }
            ].into_iter(),
            BandAllocations::OtherPlace => todo!()
        }

    }
}

/// The license class
#[derive(Debug, Clone, Copy)]
enum LicenseClass {
    Extra,
    General,
    Technician,
    Novice
}

struct BandAllocation {
    /// The name of the band
    name: &'static str,
    /// The full frequency range of the band
    frange: std::ops::RangeInclusive<u64>,
}
