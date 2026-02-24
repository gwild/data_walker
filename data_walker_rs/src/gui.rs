//! Native GUI viewer using three-d for real 3D rendering
//!
//! Proper 3D visualization with orbit camera and SpaceMouse support

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use tracing::info;
use three_d::*;
use three_d::egui;

use crate::config::Config;
use crate::walk::walk_base12;
use crate::converters::math::MathGenerator;

/// SpaceMouse axis configuration
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct SpaceMouseConfig {
    pub pan_x_axis: usize,
    pub pan_y_axis: usize,
    pub zoom_axis: usize,
    pub rot_x_axis: usize,
    pub rot_y_axis: usize,
    pub rot_z_axis: usize,
    pub invert: [bool; 6],
    pub sensitivity: f32,
}

impl Default for SpaceMouseConfig {
    fn default() -> Self {
        Self {
            pan_x_axis: 0,
            pan_y_axis: 1,
            zoom_axis: 2,
            rot_x_axis: 4,
            rot_y_axis: 3,
            rot_z_axis: 5,
            invert: [false; 6],
            sensitivity: 1.0,
        }
    }
}

impl SpaceMouseConfig {
    pub fn load() -> Self {
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

    pub fn save(&self) {
        let path = PathBuf::from("spacemouse.yaml");
        if let Ok(yaml) = serde_yaml::to_string(self) {
            if std::fs::write(&path, yaml).is_ok() {
                info!("Saved SpaceMouse config to spacemouse.yaml");
            }
        }
    }
}

/// SpaceMouse input state (raw values from device)
struct SpaceMouseState {
    axes: [f32; 6],  // tx, ty, tz, rx, ry, rz
}

struct WalkData {
    name: String,
    points: Vec<[f32; 3]>,
    color: [f32; 3],
    visible: bool,
}

/// Run the native 3D GUI viewer using three-d
pub fn run_viewer(config: Config) -> anyhow::Result<()> {
    // Create window
    let window = Window::new(WindowSettings {
        title: "Data Walker - 3D".to_string(),
        max_size: Some((1920, 1080)),
        ..Default::default()
    })?;

    let context = window.gl();

    // Camera setup - orbit control
    let target = vec3(0.0, 0.0, 0.0);
    let mut camera = Camera::new_perspective(
        window.viewport(),
        vec3(0.0, 50.0, 200.0),  // position
        target,                   // target
        vec3(0.0, 1.0, 0.0),     // up
        degrees(45.0),           // fov
        0.1,                     // near
        10000.0,                 // far
    );

    let mut orbit_control = OrbitControl::new(target, 1.0, 10000.0);

    // SpaceMouse
    let spacemouse = init_spacemouse();
    let spacemouse_config = SpaceMouseConfig::load();

    // State
    let mut walks: BTreeMap<String, WalkData> = BTreeMap::new();
    let mut selected_sources: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut selected_mapping = "Identity".to_string();
    let mut max_points: usize = 5000;
    let mut show_grid = true;
    let mut auto_rotate = false;
    let mut rotation_angle: f32 = 0.0;

    // GUI state
    let mut gui = GUI::new(&context);

    // Pre-build category list
    let mut by_category: BTreeMap<String, Vec<crate::config::Source>> = BTreeMap::new();
    for source in &config.sources {
        by_category.entry(source.category.clone()).or_default().push(source.clone());
    }

    // Main loop
    window.render_loop(move |mut frame_input| {
        // Handle SpaceMouse
        if let Some(ref sm) = spacemouse {
            if let Ok(state) = sm.lock() {
                let cfg = &spacemouse_config;
                let sens = cfg.sensitivity * 0.5;

                let get_axis = |idx: usize| -> f32 {
                    let val = state.axes[idx];
                    if cfg.invert[idx] { -val } else { val }
                };

                // Apply to camera position
                let rot_y = get_axis(cfg.rot_y_axis) * 0.001 * sens;
                let rot_x = get_axis(cfg.rot_x_axis) * 0.001 * sens;
                let zoom = get_axis(cfg.zoom_axis) * 0.01 * sens;

                if rot_y.abs() > 0.0001 || rot_x.abs() > 0.0001 || zoom.abs() > 0.0001 {
                    let pos = camera.position();
                    let target = camera.target();
                    let mut dir = pos - target;
                    let dist = dir.magnitude();

                    // Rotate around target
                    let yaw = Mat4::from_angle_y(radians(rot_y));
                    let right = camera.right_direction();
                    let pitch = Mat4::from_axis_angle(right, radians(rot_x));

                    dir = (yaw * pitch * vec4(dir.x, dir.y, dir.z, 0.0)).truncate();

                    // Apply zoom
                    let new_dist = (dist + zoom).max(1.0).min(5000.0);
                    dir = dir.normalize() * new_dist;

                    camera.set_view(target + dir, target, vec3(0.0, 1.0, 0.0));
                }
            }
        }

        // Auto-rotate
        if auto_rotate {
            rotation_angle += 0.01;
            let pos = camera.position();
            let target = camera.target();
            let dir = pos - target;
            let dist = dir.magnitude();
            let new_x = rotation_angle.cos() * dist;
            let new_z = rotation_angle.sin() * dist;
            camera.set_view(vec3(new_x, pos.y, new_z), target, vec3(0.0, 1.0, 0.0));
        }

        // Handle orbit control
        orbit_control.handle_events(&mut camera, &mut frame_input.events);
        camera.set_viewport(frame_input.viewport);

        // Build line geometry for visible walks
        let mut walk_lines: Vec<Gm<InstancedMesh, ColorMaterial>> = Vec::new();

        for (_id, walk) in &walks {
            if !walk.visible || walk.points.len() < 2 {
                continue;
            }

            let color = Srgba::new(
                (walk.color[0] * 255.0) as u8,
                (walk.color[1] * 255.0) as u8,
                (walk.color[2] * 255.0) as u8,
                255,
            );

            // Create line segments using thin cylinders
            let mut instances = Instances::default();
            instances.transformations = Vec::new();
            instances.colors = Some(Vec::new());

            for i in 0..walk.points.len() - 1 {
                let p1 = vec3(walk.points[i][0], walk.points[i][1], walk.points[i][2]);
                let p2 = vec3(walk.points[i + 1][0], walk.points[i + 1][1], walk.points[i + 1][2]);

                let center = (p1 + p2) * 0.5;
                let dir = p2 - p1;
                let length = dir.magnitude();

                if length > 0.001 {
                    // Create transform for cylinder
                    let up = vec3(0.0, 1.0, 0.0);
                    let rotation = if dir.normalize().dot(up).abs() > 0.999 {
                        Mat4::identity()
                    } else {
                        let axis = up.cross(dir.normalize()).normalize();
                        let angle = up.dot(dir.normalize()).acos();
                        Mat4::from_axis_angle(axis, radians(angle.to_degrees()))
                    };

                    let transform = Mat4::from_translation(center)
                        * rotation
                        * Mat4::from_nonuniform_scale(0.5, length * 0.5, 0.5);

                    instances.transformations.push(transform);
                    if let Some(ref mut colors) = instances.colors {
                        colors.push(color);
                    }
                }
            }

            if !instances.transformations.is_empty() {
                let cylinder = CpuMesh::cylinder(8);
                let instanced = Gm::new(
                    InstancedMesh::new(&context, &instances, &cylinder),
                    ColorMaterial::default(),
                );
                walk_lines.push(instanced);
            }
        }

        // Grid using thin cylinders
        let grid_objects: Vec<Gm<InstancedMesh, ColorMaterial>> = if show_grid {
            let grid_size = 200.0;
            let grid_step = 20.0;
            let grid_color = Srgba::new(60, 60, 80, 255);

            let mut instances = Instances::default();
            instances.transformations = Vec::new();
            instances.colors = Some(Vec::new());

            let mut i = -grid_size;
            while i <= grid_size {
                // X direction line
                let transform_x = Mat4::from_translation(vec3(0.0, 0.0, i))
                    * Mat4::from_angle_z(degrees(90.0))
                    * Mat4::from_nonuniform_scale(0.2, grid_size, 0.2);
                instances.transformations.push(transform_x);
                if let Some(ref mut colors) = instances.colors {
                    colors.push(grid_color);
                }

                // Z direction line
                let transform_z = Mat4::from_translation(vec3(i, 0.0, 0.0))
                    * Mat4::from_nonuniform_scale(0.2, grid_size, 0.2)
                    * Mat4::from_angle_x(degrees(90.0));
                instances.transformations.push(transform_z);
                if let Some(ref mut colors) = instances.colors {
                    colors.push(grid_color);
                }

                i += grid_step;
            }

            let cylinder = CpuMesh::cylinder(4);
            vec![Gm::new(
                InstancedMesh::new(&context, &instances, &cylinder),
                ColorMaterial::default(),
            )]
        } else {
            vec![]
        };

        // GUI panel
        gui.update(
            &mut frame_input.events,
            frame_input.accumulated_time,
            frame_input.viewport,
            frame_input.device_pixel_ratio,
            |egui_ctx| {
                egui::SidePanel::left("walks_panel").min_width(250.0).show(egui_ctx, |ui| {
                    ui.heading("Data Walks");
                    ui.separator();

                    // Mapping selector
                    ui.horizontal(|ui| {
                        ui.label("Mapping:");
                        egui::ComboBox::from_id_salt("mapping")
                            .selected_text(&selected_mapping)
                            .show_ui(ui, |ui| {
                                for name in config.mappings.keys() {
                                    ui.selectable_value(&mut selected_mapping, name.clone(), name);
                                }
                            });
                    });

                    ui.add(egui::Slider::new(&mut max_points, 100..=10000).text("Max points"));

                    ui.horizontal(|ui| {
                        if ui.button("Deselect All").clicked() {
                            selected_sources.clear();
                            walks.clear();
                        }
                        if ui.button("Center View").clicked() {
                            camera.set_view(
                                vec3(0.0, 50.0, 200.0),
                                vec3(0.0, 0.0, 0.0),
                                vec3(0.0, 1.0, 0.0),
                            );
                        }
                    });

                    ui.checkbox(&mut show_grid, "Show Grid");
                    ui.checkbox(&mut auto_rotate, "Auto-rotate");

                    ui.separator();

                    // Source list
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (category, sources) in &by_category {
                            let cat_name = config.categories.get(category).unwrap_or(category);
                            egui::CollapsingHeader::new(cat_name).show(ui, |ui| {
                                for source in sources {
                                    let mut checked = selected_sources.contains(&source.id);

                                    // Check if data is available
                                    let is_available = source.converter.starts_with("math.") ||
                                        check_data_exists(&source.id, &source.converter);

                                    if is_available {
                                        if ui.checkbox(&mut checked, &source.name).changed() {
                                            if checked {
                                                selected_sources.insert(source.id.clone());
                                                // Load walk
                                                if let Some(walk_data) = load_walk_data(&source, &config, max_points, &selected_mapping) {
                                                    walks.insert(source.id.clone(), walk_data);
                                                }
                                            } else {
                                                selected_sources.remove(&source.id);
                                                walks.remove(&source.id);
                                            }
                                        }
                                    } else {
                                        ui.add_enabled(false, egui::Checkbox::new(&mut checked, &source.name))
                                            .on_disabled_hover_text("Not downloaded yet");
                                    }
                                }
                            });
                        }
                    });
                });

                // Bottom panel
                egui::TopBottomPanel::bottom("status").show(egui_ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(format!("{} walks loaded", walks.len()));
                        ui.separator();
                        ui.label("Right-drag: orbit | Scroll: zoom | Middle-drag: pan");
                    });
                });
            },
        );

        // Clear and render
        frame_input.screen().clear(ClearState::color_and_depth(0.1, 0.1, 0.15, 1.0, 1.0));

        // Render grid
        for grid_obj in &grid_objects {
            grid_obj.render(&camera, &[]);
        }

        // Render walks
        for walk_obj in &walk_lines {
            walk_obj.render(&camera, &[]);
        }

        // Render GUI
        let _ = frame_input.screen().write(|| gui.render());

        FrameOutput::default()
    });

    Ok(())
}

fn check_data_exists(id: &str, converter: &str) -> bool {
    let path = match converter {
        "audio" => format!("data/audio/{}.json", id),
        "dna" => format!("data/dna/{}.json", id),
        "cosmos" => format!("data/cosmos/{}.json", id),
        "finance" => format!("data/finance/{}.json", id),
        _ => return false,
    };
    std::path::Path::new(&path).exists()
}

fn load_walk_data(
    source: &crate::config::Source,
    config: &Config,
    max_points: usize,
    mapping_name: &str,
) -> Option<WalkData> {
    let base12 = if source.converter.starts_with("math.") {
        MathGenerator::from_converter_string(&source.converter)?.generate(max_points)
    } else {
        // Load from cache
        let path = match source.converter.as_str() {
            "audio" => format!("data/audio/{}.json", source.id),
            "dna" => format!("data/dna/{}.json", source.id),
            "cosmos" => format!("data/cosmos/{}.json", source.id),
            "finance" => format!("data/finance/{}.json", source.id),
            _ => return None,
        };

        let contents = std::fs::read_to_string(&path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&contents).ok()?;
        let arr = json.get("base12")?.as_array()?;
        arr.iter().filter_map(|v| v.as_u64().map(|n| n as u8)).collect()
    };

    info!("Loaded {} base12 digits for {}", base12.len(), source.id);

    let mapping = config.mappings.get(mapping_name)
        .map(|v| {
            let mut arr = [0u8; 12];
            for (i, &val) in v.iter().enumerate().take(12) {
                arr[i] = val;
            }
            arr
        })
        .unwrap_or([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);

    let points = walk_base12(&base12, &mapping, max_points);
    info!("Generated {} walk points for {}", points.len(), source.id);

    // Color based on hash of id
    let hash = source.id.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    let hue = (hash % 360) as f32 / 360.0;
    let color = hsv_to_rgb(hue, 0.7, 0.9);

    Some(WalkData {
        name: source.name.clone(),
        points,
        color,
        visible: true,
    })
}

/// Initialize SpaceMouse using hidapi
fn init_spacemouse() -> Option<Arc<Mutex<SpaceMouseState>>> {
    const VENDOR_3DCONNEXION_NEW: u16 = 0x256f;
    const VENDOR_3DCONNEXION_OLD: u16 = 0x046d;
    const SPACEMOUSE_WIRELESS_NEW: u16 = 0xc62e;
    const SPACEMOUSE_WIRELESS_OLD: u16 = 0xc62f;
    const SPACEMOUSE_COMPACT: u16 = 0xc635;
    const SPACEMOUSE_PRO: u16 = 0xc62b;
    const SPACEMOUSE_PRO_WIRELESS: u16 = 0xc632;

    let state = Arc::new(Mutex::new(SpaceMouseState {
        axes: [0.0; 6],
    }));

    let state_clone = state.clone();

    std::thread::spawn(move || {
        let api = match hidapi::HidApi::new() {
            Ok(api) => api,
            Err(e) => {
                info!("HID API init failed: {}", e);
                return;
            }
        };

        let device = api.open(VENDOR_3DCONNEXION_NEW, SPACEMOUSE_WIRELESS_NEW)
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
                    if let Ok(mut state) = state_clone.lock() {
                        state.axes = [0.0; 6];
                    }
                }
                Err(_) => break,
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
