//! Native GUI viewer using egui
//!
//! 3D visualization with mouse orbit controls and SpaceMouse support

use eframe::egui;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::walk::walk_base12;
use crate::converters::math::MathGenerator;

/// SpaceMouse axis configuration
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct SpaceMouseConfig {
    // Axis mapping: which physical axis controls which function
    // 0=tx, 1=ty, 2=tz, 3=rx, 4=ry, 5=rz
    pan_x_axis: usize,
    pan_y_axis: usize,
    zoom_axis: usize,
    rot_x_axis: usize,
    rot_y_axis: usize,
    rot_z_axis: usize,
    // Invert flags
    invert: [bool; 6],
    // Sensitivity
    sensitivity: f32,
}

impl Default for SpaceMouseConfig {
    fn default() -> Self {
        Self {
            pan_x_axis: 0,   // tx -> pan X
            pan_y_axis: 1,   // ty -> pan Y
            zoom_axis: 2,    // tz -> zoom
            rot_x_axis: 4,   // ry -> rotate X (pitch)
            rot_y_axis: 3,   // rx -> rotate Y (yaw)
            rot_z_axis: 5,   // rz -> rotate Z (roll, unused)
            invert: [false; 6],
            sensitivity: 1.0,
        }
    }
}

impl SpaceMouseConfig {
    fn load() -> Self {
        let path = PathBuf::from("spacemouse.yaml");
        if path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Ok(config) = serde_yaml::from_str(&contents) {
                    info!("Loaded SpaceMouse config from spacemouse.yaml");
                    return config;
                }
            }
        }
        Self::default()
    }

    fn save(&self) {
        let path = PathBuf::from("spacemouse.yaml");
        if let Ok(yaml) = serde_yaml::to_string(self) {
            if std::fs::write(&path, yaml).is_ok() {
                info!("Saved SpaceMouse config to spacemouse.yaml");
            }
        }
    }
}

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
    base12: Vec<u8>,
    points: Vec<[f32; 3]>,
    color: [f32; 3],
    visible: bool,
    is_math: bool,
    converter: String,
}

/// SpaceMouse input state (raw values from device)
struct SpaceMouseState {
    axes: [f32; 6],  // tx, ty, tz, rx, ry, rz
}

struct WalkerApp {
    config: Config,
    walks: BTreeMap<String, WalkData>,
    selected_mapping: String,
    max_points: usize,
    // Camera state
    camera_distance: f32,
    camera_angle_x: f32,  // Pitch (up/down)
    camera_angle_y: f32,  // Yaw (left/right)
    camera_target: [f32; 2],  // Pan offset
    // UI state
    show_grid: bool,
    point_size: f32,
    auto_rotate: bool,
    // SpaceMouse
    spacemouse: Option<Arc<Mutex<SpaceMouseState>>>,
    spacemouse_config: SpaceMouseConfig,
    spacemouse_config_open: bool,
    spacemouse_thread: Option<std::thread::JoinHandle<()>>,
}

impl WalkerApp {
    fn new(cc: &eframe::CreationContext<'_>, config: Config) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        // Try to initialize SpaceMouse
        let spacemouse = init_spacemouse();
        let spacemouse_state = spacemouse.as_ref().map(|s| s.clone());

        Self {
            config,
            walks: BTreeMap::new(),
            selected_mapping: "Identity".to_string(),
            max_points: 5000,
            camera_distance: 1.0,
            camera_angle_x: 0.0,
            camera_angle_y: 0.0,
            camera_target: [0.0, 0.0],
            show_grid: true,
            point_size: 2.0,
            auto_rotate: false,
            spacemouse: spacemouse_state,
            spacemouse_config: SpaceMouseConfig::load(),
            spacemouse_config_open: false,
            spacemouse_thread: None,
        }
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
            match self.load_cached_base12("dna", id, &source.url) {
                Some(data) => {
                    info!("Loaded {} base12 digits from cache for {}", data.len(), id);
                    data
                }
                None => {
                    warn!("DNA data not cached for {}", id);
                    return;
                }
            }
        } else if source.converter == "finance" {
            match self.load_cached_base12("finance", id, &source.url) {
                Some(data) => {
                    info!("Loaded {} base12 digits from cache for {}", data.len(), id);
                    data
                }
                None => {
                    warn!("Finance data not cached for {}", id);
                    return;
                }
            }
        } else if source.converter == "audio" {
            match self.load_cached_audio(id) {
                Some(data) => {
                    info!("Loaded {} base12 digits from cache for {}", data.len(), id);
                    data
                }
                None => {
                    warn!("Audio data not cached for {}", id);
                    return;
                }
            }
        } else if source.converter == "cosmos" {
            match self.load_cached_cosmos(id) {
                Some(data) => {
                    info!("Loaded {} base12 digits from cache for {}", data.len(), id);
                    data
                }
                None => {
                    warn!("Cosmos data not cached for {}", id);
                    return;
                }
            }
        } else {
            warn!("Converter not implemented: {}", source.converter);
            return;
        };

        let mapping = self.config.get_mapping(&self.selected_mapping);
        let points = walk_base12(&base12, &mapping, self.max_points);
        info!("Generated {} walk points for {}", points.len(), id);

        let hash = id.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
        let hue = (hash % 360) as f32 / 360.0;
        let color = hsv_to_rgb(hue, 0.8, 0.9);

        let is_math = source.converter.starts_with("math.");
        self.walks.insert(id.to_string(), WalkData {
            name: source.name.clone(),
            base12,
            points,
            color,
            visible: true,
            is_math,
            converter: source.converter.clone(),
        });

        info!("Walk {} loaded successfully", id);
    }

    fn remove_walk(&mut self, id: &str) {
        self.walks.remove(id);
    }

    fn load_cached_base12(&self, category: &str, id: &str, url: &str) -> Option<Vec<u8>> {
        let file_id = url.split('/').last().unwrap_or(id);
        let decoded = file_id.replace("%5E", "^");
        let filename = format!("{}.json",
            decoded.replace("^", "").replace(".", "_").replace("-", "_"));
        let path = std::path::Path::new("data").join(category).join(&filename);

        if !path.exists() { return None; }

        let content = std::fs::read_to_string(&path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&content).ok()?;
        let base12_array = json.get("base12")?.as_array()?;
        Some(base12_array.iter().filter_map(|v| v.as_u64().map(|n| n as u8)).collect())
    }

    fn load_cached_audio(&self, id: &str) -> Option<Vec<u8>> {
        let path = std::path::Path::new("data").join("audio").join(format!("{}.json", id));
        if !path.exists() { return None; }
        let content = std::fs::read_to_string(&path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&content).ok()?;
        let base12_array = json.get("base12")?.as_array()?;
        Some(base12_array.iter().filter_map(|v| v.as_u64().map(|n| n as u8)).collect())
    }

    fn load_cached_cosmos(&self, id: &str) -> Option<Vec<u8>> {
        let path = std::path::Path::new("data").join("cosmos").join(format!("{}.json", id));
        if !path.exists() { return None; }
        let content = std::fs::read_to_string(&path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&content).ok()?;
        let base12_array = json.get("base12")?.as_array()?;
        Some(base12_array.iter().filter_map(|v| v.as_u64().map(|n| n as u8)).collect())
    }

    fn has_data_available(&self, source: &crate::config::Source) -> bool {
        if source.converter.starts_with("math.") { return true; }
        match source.converter.as_str() {
            "dna" => {
                let file_id = source.url.split('/').last().unwrap_or(&source.id);
                let filename = format!("{}.json", file_id.replace(".", "_"));
                std::path::Path::new("data").join("dna").join(&filename).exists()
            }
            "finance" => {
                let file_id = source.url.split('/').last().unwrap_or(&source.id);
                let decoded = file_id.replace("%5E", "^");
                let filename = format!("{}.json", decoded.replace("^", "").replace(".", "_").replace("-", "_"));
                std::path::Path::new("data").join("finance").join(&filename).exists()
            }
            "audio" => std::path::Path::new("data").join("audio").join(format!("{}.json", source.id)).exists(),
            "cosmos" => std::path::Path::new("data").join("cosmos").join(format!("{}.json", source.id)).exists(),
            _ => false
        }
    }

    fn recompute_all_walks(&mut self) {
        let mapping = self.config.get_mapping(&self.selected_mapping);
        for (id, walk) in self.walks.iter_mut() {
            if walk.is_math {
                if let Some(gen) = MathGenerator::from_converter_string(&walk.converter) {
                    walk.base12 = gen.generate(self.max_points);
                }
            }
            walk.points = walk_base12(&walk.base12, &mapping, self.max_points);
            debug!("Recomputed {} points for {}", walk.points.len(), id);
        }
    }

    fn clear_all_walks(&mut self) {
        self.walks.clear();
    }

    fn center_view(&mut self) {
        self.camera_angle_x = 0.0;
        self.camera_angle_y = 0.0;
        self.camera_distance = 1.0;
        self.camera_target = [0.0, 0.0];
        self.reset_view = true;  // Flag to reset plot bounds
    }

    fn project_point(&self, p: [f32; 3]) -> [f64; 2] {
        let cos_x = self.camera_angle_x.cos();
        let sin_x = self.camera_angle_x.sin();
        let cos_y = self.camera_angle_y.cos();
        let sin_y = self.camera_angle_y.sin();

        // Rotate around Y axis (yaw)
        let x1 = p[0] * cos_y + p[2] * sin_y;
        let z1 = -p[0] * sin_y + p[2] * cos_y;

        // Rotate around X axis (pitch)
        let y1 = p[1] * cos_x - z1 * sin_x;

        // Apply pan only (zoom handled by plot bounds)
        [
            (x1 + self.camera_target[0]) as f64,
            (y1 + self.camera_target[1]) as f64,
        ]
    }
}

impl eframe::App for WalkerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());

        // Request continuous repaint for smooth interaction
        ctx.request_repaint();

        // Handle SpaceMouse input with configurable axis mapping
        if let Some(ref sm) = self.spacemouse {
            if let Ok(state) = sm.lock() {
                let cfg = &self.spacemouse_config;
                let sens = cfg.sensitivity;

                // Get axis values with invert applied
                let get_axis = |idx: usize| -> f32 {
                    let val = state.axes[idx];
                    if cfg.invert[idx] { -val } else { val }
                };

                // Apply rotation
                self.camera_angle_y += get_axis(cfg.rot_y_axis) * 0.001 * sens;
                self.camera_angle_x += get_axis(cfg.rot_x_axis) * 0.001 * sens;
                // Apply zoom
                self.camera_distance += get_axis(cfg.zoom_axis) * 0.0001 * sens;
                self.camera_distance = self.camera_distance.clamp(0.1, 10.0);
                // Apply pan
                self.camera_target[0] += get_axis(cfg.pan_x_axis) * 0.01 * sens;
                self.camera_target[1] += get_axis(cfg.pan_y_axis) * 0.01 * sens;
            }
        }

        // Auto-rotate
        if self.auto_rotate {
            self.camera_angle_y += 0.005;
        }

        // Left panel - walk selection
        egui::SidePanel::left("walks_panel").min_width(250.0).show(ctx, |ui| {
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
            if self.selected_mapping != old_mapping {
                self.recompute_all_walks();
            }

            let old_max_points = self.max_points;
            ui.add(egui::Slider::new(&mut self.max_points, 100..=10000).text("Max points"));
            if self.max_points != old_max_points {
                self.recompute_all_walks();
            }

            ui.horizontal(|ui| {
                if ui.button("Deselect All").clicked() {
                    self.clear_all_walks();
                }
                ui.label(format!("{} loaded", self.walks.len()));
            });

            ui.separator();

            // Source list
            let mut by_category: BTreeMap<String, Vec<_>> = BTreeMap::new();
            for source in &self.config.sources {
                by_category.entry(source.category.clone()).or_default().push(source.clone());
            }

            let mut to_load: Vec<String> = Vec::new();
            let mut to_remove: Vec<String> = Vec::new();

            egui::ScrollArea::vertical().show(ui, |ui| {
                for (category, sources) in by_category.iter() {
                    let cat_name = self.config.categories.get(category).map(|s| s.as_str()).unwrap_or(category.as_str());
                    ui.collapsing(cat_name, |ui| {
                        for source in sources {
                            let is_loaded = self.walks.contains_key(&source.id);
                            let is_available = self.has_data_available(source);
                            let mut checked = is_loaded;

                            let color = if let Some(walk) = self.walks.get(&source.id) {
                                egui::Color32::from_rgb(
                                    (walk.color[0] * 255.0) as u8,
                                    (walk.color[1] * 255.0) as u8,
                                    (walk.color[2] * 255.0) as u8,
                                )
                            } else if is_available {
                                egui::Color32::GRAY
                            } else {
                                egui::Color32::DARK_GRAY
                            };

                            ui.horizontal(|ui| {
                                ui.colored_label(color, "●");
                                if is_available {
                                    if ui.checkbox(&mut checked, &source.name).changed() {
                                        if checked { to_load.push(source.id.clone()); }
                                        else { to_remove.push(source.id.clone()); }
                                    }
                                } else {
                                    ui.add_enabled(false, egui::Checkbox::new(&mut checked, &source.name))
                                        .on_disabled_hover_text("Not downloaded yet");
                                }
                            });
                        }
                    });
                }
            });

            for id in to_load { self.load_walk(&id); }
            for id in to_remove { self.remove_walk(&id); }
        });

        // Bottom panel - controls
        egui::TopBottomPanel::bottom("controls_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.show_grid, "Grid");
                ui.checkbox(&mut self.auto_rotate, "Auto-rotate");

                ui.separator();
                ui.label("Rotate:");
                ui.add(egui::DragValue::new(&mut self.camera_angle_x).speed(0.02).prefix("X:"));
                ui.add(egui::DragValue::new(&mut self.camera_angle_y).speed(0.02).prefix("Y:"));

                ui.separator();
                if ui.button("Center").clicked() {
                    self.center_view();
                }

                if self.spacemouse.is_some() {
                    ui.separator();
                    if ui.button("SpaceMouse Config").clicked() {
                        self.spacemouse_config_open = !self.spacemouse_config_open;
                    }
                }
            });

            // SpaceMouse config panel (expandable)
            if self.spacemouse_config_open && self.spacemouse.is_some() {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Sensitivity:");
                    ui.add(egui::Slider::new(&mut self.spacemouse_config.sensitivity, 0.1..=3.0));
                });

                ui.horizontal(|ui| {
                    ui.label("Axis mapping:");
                });

                let axis_names = ["TX", "TY", "TZ", "RX", "RY", "RZ"];
                let function_names = ["Pan X", "Pan Y", "Zoom", "Rot X", "Rot Y", "Rot Z"];

                ui.horizontal(|ui| {
                    // Pan X axis
                    ui.label("Pan X:");
                    egui::ComboBox::from_id_salt("pan_x")
                        .selected_text(axis_names[self.spacemouse_config.pan_x_axis])
                        .width(50.0)
                        .show_ui(ui, |ui| {
                            for (i, name) in axis_names.iter().enumerate() {
                                ui.selectable_value(&mut self.spacemouse_config.pan_x_axis, i, *name);
                            }
                        });

                    // Pan Y axis
                    ui.label("Pan Y:");
                    egui::ComboBox::from_id_salt("pan_y")
                        .selected_text(axis_names[self.spacemouse_config.pan_y_axis])
                        .width(50.0)
                        .show_ui(ui, |ui| {
                            for (i, name) in axis_names.iter().enumerate() {
                                ui.selectable_value(&mut self.spacemouse_config.pan_y_axis, i, *name);
                            }
                        });

                    // Zoom axis
                    ui.label("Zoom:");
                    egui::ComboBox::from_id_salt("zoom")
                        .selected_text(axis_names[self.spacemouse_config.zoom_axis])
                        .width(50.0)
                        .show_ui(ui, |ui| {
                            for (i, name) in axis_names.iter().enumerate() {
                                ui.selectable_value(&mut self.spacemouse_config.zoom_axis, i, *name);
                            }
                        });
                });

                ui.horizontal(|ui| {
                    // Rot X axis
                    ui.label("Rot X:");
                    egui::ComboBox::from_id_salt("rot_x")
                        .selected_text(axis_names[self.spacemouse_config.rot_x_axis])
                        .width(50.0)
                        .show_ui(ui, |ui| {
                            for (i, name) in axis_names.iter().enumerate() {
                                ui.selectable_value(&mut self.spacemouse_config.rot_x_axis, i, *name);
                            }
                        });

                    // Rot Y axis
                    ui.label("Rot Y:");
                    egui::ComboBox::from_id_salt("rot_y")
                        .selected_text(axis_names[self.spacemouse_config.rot_y_axis])
                        .width(50.0)
                        .show_ui(ui, |ui| {
                            for (i, name) in axis_names.iter().enumerate() {
                                ui.selectable_value(&mut self.spacemouse_config.rot_y_axis, i, *name);
                            }
                        });

                    // Rot Z axis (unused but configurable)
                    ui.label("Rot Z:");
                    egui::ComboBox::from_id_salt("rot_z")
                        .selected_text(axis_names[self.spacemouse_config.rot_z_axis])
                        .width(50.0)
                        .show_ui(ui, |ui| {
                            for (i, name) in axis_names.iter().enumerate() {
                                ui.selectable_value(&mut self.spacemouse_config.rot_z_axis, i, *name);
                            }
                        });
                });

                ui.horizontal(|ui| {
                    ui.label("Invert:");
                    for (i, name) in axis_names.iter().enumerate() {
                        ui.checkbox(&mut self.spacemouse_config.invert[i], *name);
                    }
                });

                ui.horizontal(|ui| {
                    if ui.button("Save Config").clicked() {
                        self.spacemouse_config.save();
                    }
                    if ui.button("Reset Defaults").clicked() {
                        self.spacemouse_config = SpaceMouseConfig::default();
                    }
                });
            }
        });

        // Central panel - 3D view
        egui::CentralPanel::default().show(ctx, |ui| {
            // Mouse controls help
            ui.horizontal(|ui| {
                ui.label(format!("{} walks | ", self.walks.len()));
                ui.label("Right-drag: rotate | Middle-drag: pan | Scroll: zoom");
            });

            // Handle keyboard and scroll input
            ctx.input(|i| {
                // Arrow keys for rotation
                if i.key_down(egui::Key::ArrowLeft) { self.camera_angle_y -= 0.03; }
                if i.key_down(egui::Key::ArrowRight) { self.camera_angle_y += 0.03; }
                if i.key_down(egui::Key::ArrowUp) { self.camera_angle_x -= 0.03; }
                if i.key_down(egui::Key::ArrowDown) { self.camera_angle_x += 0.03; }
                // +/- for zoom
                if i.key_down(egui::Key::Minus) {
                    self.camera_distance *= 1.02;
                }
                if i.key_down(egui::Key::Plus) {
                    self.camera_distance *= 0.98;
                }
                // Home to center
                if i.key_pressed(egui::Key::Home) { self.center_view(); }
                // Scroll for zoom
                if i.raw_scroll_delta.y != 0.0 {
                    self.camera_distance *= 1.0 - i.raw_scroll_delta.y * 0.002;
                }
                // Mouse drag rotation (right button) and pan (middle button)
                if i.pointer.secondary_down() {
                    let delta = i.pointer.delta();
                    self.camera_angle_y += delta.x * 0.005;
                    self.camera_angle_x += delta.y * 0.005;
                }
                if i.pointer.middle_down() {
                    let delta = i.pointer.delta();
                    self.camera_target[0] -= delta.x * 0.5;
                    self.camera_target[1] += delta.y * 0.5;
                }
            });

            // Clamp values
            self.camera_angle_x = self.camera_angle_x.clamp(-1.5, 1.5);
            self.camera_distance = self.camera_distance.clamp(0.1, 10.0);

            // Calculate plot bounds based on zoom level
            let view_range = 100.0 * self.camera_distance as f64;

            // Create axis hints for consistent tick spacing (every 10 units)
            let x_axis = egui_plot::AxisHints::new_x()
                .label("X")
                .formatter(|val, _range| format!("{:.0}", val.value));
            let y_axis = egui_plot::AxisHints::new_y()
                .label("Y")
                .formatter(|val, _range| format!("{:.0}", val.value));

            // Build plot with explicit bounds for zoom control
            let plot = egui_plot::Plot::new("walk_plot")
                .data_aspect(1.0)
                .allow_drag(true)   // Pan with mouse drag
                .allow_zoom(true)   // Allow scroll zoom too
                .allow_scroll(true)
                .show_axes(true)
                .show_grid(self.show_grid)
                .custom_x_axes(vec![x_axis])
                .custom_y_axes(vec![y_axis])
                .include_x(-view_range)
                .include_x(view_range)
                .include_y(-view_range)
                .include_y(view_range);

            plot.show(ui, |plot_ui| {
                for (_id, walk) in &self.walks {
                    if !walk.visible || walk.points.is_empty() {
                        continue;
                    }

                    let points_2d: Vec<[f64; 2]> = walk.points.iter()
                        .map(|&p| self.project_point(p))
                        .collect();

                    let color = egui::Color32::from_rgb(
                        (walk.color[0] * 255.0) as u8,
                        (walk.color[1] * 255.0) as u8,
                        (walk.color[2] * 255.0) as u8,
                    );

                    let line = egui_plot::Line::new(egui_plot::PlotPoints::from_iter(
                        points_2d.iter().map(|p| [p[0], p[1]])
                    ))
                    .color(color)
                    .width(self.point_size)
                    .name(&walk.name);

                    plot_ui.line(line);
                }
            });
        });
    }
}

/// Initialize SpaceMouse using hidapi
fn init_spacemouse() -> Option<Arc<Mutex<SpaceMouseState>>> {
    // 3Dconnexion vendor IDs (newer devices use 0x256f, older use 0x046d via Logitech)
    const VENDOR_3DCONNEXION_NEW: u16 = 0x256f;
    const VENDOR_3DCONNEXION_OLD: u16 = 0x046d;
    // Common SpaceMouse product IDs
    const SPACEMOUSE_WIRELESS_NEW: u16 = 0xc62e;  // Current SpaceMouse Wireless
    const SPACEMOUSE_WIRELESS_OLD: u16 = 0xc62f;
    const SPACEMOUSE_COMPACT: u16 = 0xc635;
    const SPACEMOUSE_PRO: u16 = 0xc62b;
    const SPACEMOUSE_PRO_WIRELESS: u16 = 0xc632;

    let state = Arc::new(Mutex::new(SpaceMouseState {
        axes: [0.0; 6],  // tx, ty, tz, rx, ry, rz
    }));

    let state_clone = state.clone();

    // Try to open SpaceMouse in a background thread
    std::thread::spawn(move || {
        let api = match hidapi::HidApi::new() {
            Ok(api) => api,
            Err(e) => {
                info!("HID API init failed (no SpaceMouse support): {}", e);
                return;
            }
        };

        // Try different vendor/product ID combinations
        let device = api.open(VENDOR_3DCONNEXION_NEW, SPACEMOUSE_WIRELESS_NEW)  // Current SpaceMouse Wireless
            .or_else(|_| api.open(VENDOR_3DCONNEXION_NEW, SPACEMOUSE_COMPACT))
            .or_else(|_| api.open(VENDOR_3DCONNEXION_NEW, SPACEMOUSE_PRO))
            .or_else(|_| api.open(VENDOR_3DCONNEXION_NEW, SPACEMOUSE_PRO_WIRELESS))
            .or_else(|_| api.open(VENDOR_3DCONNEXION_OLD, SPACEMOUSE_WIRELESS_OLD))
            .or_else(|_| api.open(VENDOR_3DCONNEXION_OLD, SPACEMOUSE_COMPACT))
            .or_else(|_| api.open(VENDOR_3DCONNEXION_OLD, SPACEMOUSE_PRO));

        let device = match device {
            Ok(d) => {
                info!("SpaceMouse connected!");
                d
            }
            Err(_) => {
                info!("No SpaceMouse found");
                return;
            }
        };

        let mut buf = [0u8; 13];
        loop {
            match device.read_timeout(&mut buf, 100) {
                Ok(len) if len > 0 => {
                    if let Ok(mut state) = state_clone.lock() {
                        // Parse SpaceMouse packet
                        // Report ID 1: Translation (X, Y, Z) -> axes[0,1,2]
                        // Report ID 2: Rotation (Rx, Ry, Rz) -> axes[3,4,5]
                        match buf[0] {
                            1 if len >= 7 => {
                                state.axes[0] = i16::from_le_bytes([buf[1], buf[2]]) as f32;
                                state.axes[1] = i16::from_le_bytes([buf[3], buf[4]]) as f32;
                                state.axes[2] = i16::from_le_bytes([buf[5], buf[6]]) as f32;
                            }
                            2 if len >= 7 => {
                                state.axes[3] = i16::from_le_bytes([buf[1], buf[2]]) as f32;
                                state.axes[4] = i16::from_le_bytes([buf[3], buf[4]]) as f32;
                                state.axes[5] = i16::from_le_bytes([buf[5], buf[6]]) as f32;
                            }
                            _ => {}
                        }
                    }
                }
                Ok(_) => {
                    // Timeout or no data, reset values
                    if let Ok(mut state) = state_clone.lock() {
                        state.axes = [0.0; 6];
                    }
                }
                Err(_) => break, // Device disconnected
            }
        }
    });

    Some(state)
}

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
