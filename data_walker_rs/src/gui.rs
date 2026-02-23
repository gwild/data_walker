//! Native GUI viewer using egui + three-d
//!
//! Pure Rust 3D visualization - no JavaScript

use eframe::egui;
use std::collections::BTreeMap;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::walk::walk_base12;
use crate::converters::math::MathGenerator;

/// Run the native GUI viewer
pub fn run_viewer(config: Config) -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("Data Walker"),
        ..Default::default()
    };

    eframe::run_native(
        "Data Walker",
        options,
        Box::new(|cc| Ok(Box::new(WalkerApp::new(cc, config)))),
    ).map_err(|e| anyhow::anyhow!("GUI error: {}", e))
}

struct WalkData {
    name: String,
    base12: Vec<u8>,  // Store base12 for recomputing with different mappings
    points: Vec<[f32; 3]>,
    color: [f32; 3],
    visible: bool,
}

struct WalkerApp {
    config: Config,
    walks: BTreeMap<String, WalkData>,
    selected_mapping: String,
    max_points: usize,
    camera_distance: f32,
    camera_angle_x: f32,
    camera_angle_y: f32,
    show_grid: bool,
    point_size: f32,
}

impl WalkerApp {
    fn new(cc: &eframe::CreationContext<'_>, config: Config) -> Self {
        // Dark mode
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        let mut app = Self {
            config,
            walks: BTreeMap::new(),
            selected_mapping: "Identity".to_string(),
            max_points: 5000,
            camera_distance: 200.0,
            camera_angle_x: 0.3,
            camera_angle_y: 0.3,
            show_grid: true,
            point_size: 2.0,
        };

        // Auto-load pi on startup to verify loading works
        info!("Auto-loading pi walk on startup");
        app.load_walk("pi");
        info!("Auto-load complete, walks count: {}", app.walks.len());

        app
    }

    fn load_walk(&mut self, id: &str) {
        info!("load_walk called for: {}", id);

        if self.walks.contains_key(id) {
            debug!("Walk {} already loaded, skipping", id);
            return;
        }

        let source = match self.config.get_source(id) {
            Some(s) => s.clone(),
            None => {
                warn!("Source not found for id: {}", id);
                return;
            }
        };

        debug!("Found source: {} with converter: {}", source.name, source.converter);

        // Generate or load base12 data
        let base12 = if source.converter.starts_with("math.") {
            match MathGenerator::from_converter_string(&source.converter) {
                Some(gen) => {
                    let data = gen.generate(self.max_points);
                    info!("Generated {} base12 digits for {}", data.len(), id);
                    data
                }
                None => {
                    warn!("Failed to parse math converter: {}", source.converter);
                    return;
                }
            }
        } else if source.converter == "dna" {
            // Load from cached file
            match self.load_cached_base12("dna", id, &source.url) {
                Some(data) => {
                    info!("Loaded {} base12 digits from cache for {}", data.len(), id);
                    data
                }
                None => {
                    warn!("DNA data not cached for {}, run 'download --all' first", id);
                    return;
                }
            }
        } else if source.converter == "finance" {
            // Load from cached file
            match self.load_cached_base12("finance", id, &source.url) {
                Some(data) => {
                    info!("Loaded {} base12 digits from cache for {}", data.len(), id);
                    data
                }
                None => {
                    warn!("Finance data not cached for {}, run 'download --all' first", id);
                    return;
                }
            }
        } else if source.converter == "audio" {
            // Load from cached file (audio data stored by id)
            match self.load_cached_audio(id) {
                Some(data) => {
                    info!("Loaded {} base12 digits from cache for {}", data.len(), id);
                    data
                }
                None => {
                    warn!("Audio data not cached for {}, run 'download --all' first", id);
                    return;
                }
            }
        } else {
            warn!("Converter not implemented: {}", source.converter);
            return;
        };

        // Compute walk points
        let mapping = self.config.get_mapping(&self.selected_mapping);
        let points = walk_base12(&base12, &mapping, self.max_points);
        info!("Generated {} walk points for {}", points.len(), id);

        // Generate color from hash
        let hash = id.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
        let hue = (hash % 360) as f32 / 360.0;
        let color = hsv_to_rgb(hue, 0.8, 0.9);

        self.walks.insert(id.to_string(), WalkData {
            name: source.name.clone(),
            base12,
            points,
            color,
            visible: true,
        });

        info!("Walk {} loaded successfully, total walks: {}", id, self.walks.len());
    }

    fn remove_walk(&mut self, id: &str) {
        self.walks.remove(id);
    }

    fn load_cached_base12(&self, category: &str, id: &str, url: &str) -> Option<Vec<u8>> {
        // Extract identifier from URL
        let file_id = url.split('/').last().unwrap_or(id);

        // Decode and clean up the filename:
        // - %5E -> ^ (URL encoding for caret)
        // - ^ -> "" (remove caret for indices like ^GSPC)
        // - . -> _ (version numbers like NC_045512.2)
        // - - -> _ (symbols like BTC-USD)
        let decoded = file_id.replace("%5E", "^");
        let filename = format!("{}.json",
            decoded.replace("^", "").replace(".", "_").replace("-", "_"));
        let path = std::path::Path::new("data").join(category).join(&filename);

        debug!("Looking for cached data at: {:?}", path);

        if !path.exists() {
            return None;
        }

        // Read and parse JSON
        let content = std::fs::read_to_string(&path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&content).ok()?;

        // Extract base12 array
        let base12_array = json.get("base12")?.as_array()?;
        let base12: Vec<u8> = base12_array
            .iter()
            .filter_map(|v| v.as_u64().map(|n| n as u8))
            .collect();

        Some(base12)
    }

    fn load_cached_audio(&self, id: &str) -> Option<Vec<u8>> {
        // Audio files are stored by ID directly
        let path = std::path::Path::new("data").join("audio").join(format!("{}.json", id));

        debug!("Looking for cached audio at: {:?}", path);

        if !path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(&path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&content).ok()?;

        let base12_array = json.get("base12")?.as_array()?;
        let base12: Vec<u8> = base12_array
            .iter()
            .filter_map(|v| v.as_u64().map(|n| n as u8))
            .collect();

        Some(base12)
    }

    /// Check if a source has data available (either computed or cached)
    fn has_data_available(&self, source: &crate::config::Source) -> bool {
        // Math sources are always available (computed on demand)
        if source.converter.starts_with("math.") {
            return true;
        }

        // For downloaded sources, check if cache file exists
        match source.converter.as_str() {
            "dna" => {
                let file_id = source.url.split('/').last().unwrap_or(&source.id);
                let filename = format!("{}.json", file_id.replace(".", "_"));
                let path = std::path::Path::new("data").join("dna").join(&filename);
                path.exists()
            }
            "finance" => {
                let file_id = source.url.split('/').last().unwrap_or(&source.id);
                let decoded = file_id.replace("%5E", "^");
                let filename = format!("{}.json",
                    decoded.replace("^", "").replace(".", "_").replace("-", "_"));
                let path = std::path::Path::new("data").join("finance").join(&filename);
                path.exists()
            }
            "audio" => {
                let path = std::path::Path::new("data").join("audio").join(format!("{}.json", source.id));
                path.exists()
            }
            "cosmos" => {
                // Not implemented yet
                false
            }
            _ => false
        }
    }

    fn recompute_all_walks(&mut self) {
        let mapping = self.config.get_mapping(&self.selected_mapping);
        info!("Recomputing {} walks with mapping '{}'", self.walks.len(), self.selected_mapping);

        for (id, walk) in self.walks.iter_mut() {
            let points = walk_base12(&walk.base12, &mapping, self.max_points);
            debug!("Recomputed {} points for {}", points.len(), id);
            walk.points = points;
        }
    }
}

impl eframe::App for WalkerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Force dark mode every frame (persistence doesn't work reliably)
        ctx.set_visuals(egui::Visuals::dark());

        // Log first update (use static to only log once)
        use std::sync::atomic::{AtomicBool, Ordering};
        static FIRST_UPDATE: AtomicBool = AtomicBool::new(true);
        if FIRST_UPDATE.swap(false, Ordering::SeqCst) {
            info!("GUI update() called - first frame");
        }

        // Left panel - walk selection
        egui::SidePanel::left("walks_panel")
            .min_width(250.0)
            .show(ctx, |ui| {
                ui.heading("Data Walks");
                ui.separator();

                // Mapping selector
                let old_mapping = self.selected_mapping.clone();
                ui.horizontal(|ui| {
                    ui.label("Mapping:");
                    egui::ComboBox::from_id_salt("mapping")
                        .selected_text(&self.selected_mapping)
                        .show_ui(ui, |ui| {
                            for name in self.config.mappings.keys() {
                                ui.selectable_value(&mut self.selected_mapping, name.clone(), name);
                            }
                        });
                });
                // Recompute walks if mapping changed
                if self.selected_mapping != old_mapping {
                    self.recompute_all_walks();
                }

                let old_max_points = self.max_points;
                ui.add(egui::Slider::new(&mut self.max_points, 100..=10000).text("Max points"));
                if self.max_points != old_max_points {
                    self.recompute_all_walks();
                }
                ui.separator();

                // Group sources by category (BTreeMap for stable order)
                let mut by_category: BTreeMap<String, Vec<_>> = BTreeMap::new();
                for source in &self.config.sources {
                    by_category
                        .entry(source.category.clone())
                        .or_default()
                        .push(source.clone());
                }

                // Collect actions to perform after UI rendering
                let mut to_load: Vec<String> = Vec::new();
                let mut to_remove: Vec<String> = Vec::new();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (category, sources) in by_category.iter() {
                        let cat_name = self.config.categories.get(category)
                            .map(|s| s.as_str())
                            .unwrap_or(category.as_str());
                        ui.collapsing(cat_name, |ui| {
                            for source in sources {
                                let is_loaded = self.walks.contains_key(&source.id);
                                let is_available = self.has_data_available(source);
                                let mut checked = is_loaded;

                                // Color indicator
                                let color = if let Some(walk) = self.walks.get(&source.id) {
                                    egui::Color32::from_rgb(
                                        (walk.color[0] * 255.0) as u8,
                                        (walk.color[1] * 255.0) as u8,
                                        (walk.color[2] * 255.0) as u8,
                                    )
                                } else if is_available {
                                    egui::Color32::GRAY
                                } else {
                                    egui::Color32::DARK_GRAY // Disabled items
                                };

                                ui.horizontal(|ui| {
                                    ui.colored_label(color, "●");
                                    if is_available {
                                        // Math and DNA sources are enabled
                                        if ui.checkbox(&mut checked, &source.name).changed() {
                                            debug!("Checkbox changed for {}: checked={}", source.id, checked);
                                            if checked {
                                                to_load.push(source.id.clone());
                                            } else {
                                                to_remove.push(source.id.clone());
                                            }
                                        }
                                    } else {
                                        // Other sources show as disabled with tooltip
                                        ui.add_enabled(false, egui::Checkbox::new(&mut checked, &source.name))
                                            .on_disabled_hover_text("Download not implemented yet");
                                    }
                                });
                            }
                        });
                    }
                });

                // Apply deferred actions
                if !to_load.is_empty() {
                    info!("Loading {} walks: {:?}", to_load.len(), to_load);
                }
                for id in to_load {
                    self.load_walk(&id);
                }
                for id in to_remove {
                    self.remove_walk(&id);
                }
            });

        // Bottom panel - controls
        egui::TopBottomPanel::bottom("controls_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.show_grid, "Grid");
                ui.add(egui::Slider::new(&mut self.point_size, 0.5..=10.0).text("Point size"));
                ui.add(egui::Slider::new(&mut self.camera_distance, 10.0..=1000.0).text("Distance"));
            });
        });

        // Central panel - 3D view
        egui::CentralPanel::default().show(ctx, |ui| {
            // Show walk count for debugging
            ui.label(format!("Loaded walks: {}", self.walks.len()));

            let plot = egui_plot::Plot::new("walk_plot")
                .data_aspect(1.0)
                .allow_drag(true)
                .allow_zoom(true)
                .allow_scroll(true)
                .show_axes(true)
                .show_grid(self.show_grid);

            plot.show(ui, |plot_ui| {
                for (_id, walk) in &self.walks {
                    if !walk.visible || walk.points.is_empty() {
                        continue;
                    }

                    // Project 3D to 2D (simple orthographic for now)
                    let points_2d: Vec<[f64; 2]> = walk.points.iter()
                        .map(|p| {
                            let x = p[0] as f64;
                            let y = p[1] as f64;
                            let z = p[2] as f64;
                            // Simple rotation
                            let rx = x * self.camera_angle_y.cos() as f64 - z * self.camera_angle_y.sin() as f64;
                            let ry = y * self.camera_angle_x.cos() as f64 - z * self.camera_angle_x.sin() as f64;
                            [rx, ry]
                        })
                        .collect();

                    let color = egui::Color32::from_rgb(
                        (walk.color[0] * 255.0) as u8,
                        (walk.color[1] * 255.0) as u8,
                        (walk.color[2] * 255.0) as u8,
                    );

                    // Draw as line
                    let line = egui_plot::Line::new(egui_plot::PlotPoints::from_iter(
                        points_2d.iter().map(|p| [p[0], p[1]])
                    ))
                    .color(color)
                    .name(&walk.name);

                    plot_ui.line(line);
                }
            });
        });

        // Only repaint when UI changes (not continuously)
    }
}

/// Convert HSV to RGB
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = match (h * 6.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    [r + m, g + m, b + m]
}
