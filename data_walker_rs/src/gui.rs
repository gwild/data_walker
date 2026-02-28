//! Native GUI viewer using three-d for real 3D rendering
//!
//! Proper 3D visualization with orbit camera and SpaceMouse support

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use tracing::{info, warn};
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
    // Point revisit counts: (position, count) - positions rounded to grid
    revisit_counts: std::collections::HashMap<(i32, i32, i32), u32>,
}

/// Catmull-Rom spline interpolation along a walk's points
/// Creates smooth curves that pass through all control points
fn interpolate_walk_position(points: &[[f32; 3]], t: f32) -> Vec3 {
    if points.is_empty() {
        return vec3(0.0, 0.0, 0.0);
    }
    if points.len() == 1 {
        return vec3(points[0][0], points[0][1], points[0][2]);
    }
    if points.len() == 2 {
        // Fall back to linear for just 2 points
        let p0 = vec3(points[0][0], points[0][1], points[0][2]);
        let p1 = vec3(points[1][0], points[1][1], points[1][2]);
        return p0 * (1.0 - t) + p1 * t;
    }

    // t is 0.0-1.0, map to segment index space
    let num_segments = points.len() - 1;
    let exact_idx = t * num_segments as f32;
    let segment = (exact_idx.floor() as usize).min(num_segments - 1);
    let local_t = exact_idx - segment as f32;

    // Get 4 control points for Catmull-Rom (P0, P1, P2, P3)
    // P1 and P2 are the segment endpoints, P0 and P3 are neighbors for tangent calculation
    let i1 = segment;
    let i2 = (segment + 1).min(points.len() - 1);

    // For boundary points, extrapolate or clamp
    let i0 = if segment == 0 { 0 } else { segment - 1 };
    let i3 = (segment + 2).min(points.len() - 1);

    let p0 = vec3(points[i0][0], points[i0][1], points[i0][2]);
    let p1 = vec3(points[i1][0], points[i1][1], points[i1][2]);
    let p2 = vec3(points[i2][0], points[i2][1], points[i2][2]);
    let p3 = vec3(points[i3][0], points[i3][1], points[i3][2]);

    // Catmull-Rom spline formula:
    // P(t) = 0.5 * [(2*P1) + (-P0 + P2)*t + (2*P0 - 5*P1 + 4*P2 - P3)*t² + (-P0 + 3*P1 - 3*P2 + P3)*t³]
    let t2 = local_t * local_t;
    let t3 = t2 * local_t;

    let result = (p1 * 2.0
        + (p2 - p0) * local_t
        + (p0 * 2.0 - p1 * 5.0 + p2 * 4.0 - p3) * t2
        + (p1 * 3.0 - p0 - p2 * 3.0 + p3) * t3)
        * 0.5;

    result
}

/// Calculate average position along all selected walks at a given progress (0.0-1.0)
/// Returns (current_pos, look_ahead_pos) for direction calculation, or None if no walks
fn calculate_average_path_position(
    walks: &BTreeMap<String, WalkData>,
    selected_ids: &std::collections::HashSet<String>,
    progress: f32,
) -> Option<(Vec3, Vec3)> {
    let selected_walks: Vec<_> = walks.iter()
        .filter(|(id, w)| selected_ids.contains(*id) && w.visible && !w.points.is_empty())
        .collect();

    if selected_walks.is_empty() {
        return None;
    }

    // Smoothly interpolate current position and a look-ahead position
    let look_ahead = (progress + 0.01).min(1.0);

    let mut current_sum = vec3(0.0, 0.0, 0.0);
    let mut ahead_sum = vec3(0.0, 0.0, 0.0);
    let mut count = 0.0;

    for (_, walk) in &selected_walks {
        current_sum += interpolate_walk_position(&walk.points, progress);
        ahead_sum += interpolate_walk_position(&walk.points, look_ahead);
        count += 1.0;
    }

    Some((current_sum / count, ahead_sum / count))
}

/// Run the native 3D GUI viewer using three-d
pub fn run_viewer(config: Config) -> anyhow::Result<()> {
    // Create window
    let window = Window::new(WindowSettings {
        title: "Data Walker - 3D".to_string(),
        max_size: None,
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
    let mut spacemouse_config = SpaceMouseConfig::load();
    let mut show_spacemouse_config = false;

    // State
    let mut walks: BTreeMap<String, WalkData> = BTreeMap::new();
    let mut selected_sources: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut selected_mapping = "Identity".to_string();
    let mut prev_mapping = selected_mapping.clone();
    let mut selected_base: u32 = 12;
    let mut prev_base: u32 = 12;
    let mut max_points: usize = 5000;
    let mut prev_max_points: usize = max_points;
    let mut show_grid = true;
    let mut show_axes = true;
    let mut show_points = true;
    let mut show_lines = true;
    let mut point_scale: f32 = 0.5;
    let mut line_scale: f32 = 0.3;
    let mut axis_ticks: u32 = 10;
    let mut auto_rotate = false;
    let mut screenshot_requested = false;
    let mut screenshot_status: Option<(String, f64)> = None; // (message, expire_time)
    let mut rotation_angle: f32 = 0.0;

    // Data Flight mode
    let mut flight_mode = false;
    let mut flight_playing = false;          // Is flight animation playing
    let mut flight_speed: f32 = 10.0;        // Points per second (Hz)
    let mut flight_reverse = false;          // Go backwards
    let mut flight_look_back = false;        // Look behind instead of ahead
    let mut flight_progress: f32 = 0.0;      // 0.0 to 1.0 along path
    let mut last_frame_time: f64 = 0.0;      // For delta time calculation
    let mut flight_loop = false;             // Enable looping
    let mut flight_loop_mode: u8 = 0;        // 0 = Repeat, 1 = Fwd/Rev (ping-pong)
    let mut show_loop_menu = false;          // Dropdown visibility

    // GUI state
    let mut gui = GUI::new(&context);

    // Color pool for dynamic assignment
    let mut color_pool = ColorPool::new();

    // Pre-build category list
    let mut by_category: BTreeMap<String, Vec<crate::config::Source>> = BTreeMap::new();
    for source in &config.sources {
        by_category.entry(source.category.clone()).or_default().push(source.clone());
    }

    // Main loop
    window.render_loop(move |mut frame_input| {
        // Handle keyboard events (spacebar for play/pause)
        for event in &frame_input.events {
            if let three_d::Event::KeyPress { kind, .. } = event {
                if *kind == three_d::Key::Space && flight_mode {
                    flight_playing = !flight_playing;
                }
            }
        }

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

        // Auto-rotate (only when not in flight mode)
        if auto_rotate && !flight_mode {
            rotation_angle += 0.01;
            let pos = camera.position();
            let target = camera.target();
            let dir = pos - target;
            let dist = dir.magnitude();
            let new_x = rotation_angle.cos() * dist;
            let new_z = rotation_angle.sin() * dist;
            camera.set_view(vec3(new_x, pos.y, new_z), target, vec3(0.0, 1.0, 0.0));
        }

        // Data Flight camera update
        if flight_mode {
            // Calculate delta time
            let delta = if last_frame_time > 0.0 {
                (frame_input.accumulated_time - last_frame_time) as f32
            } else {
                0.0
            };
            last_frame_time = frame_input.accumulated_time;

            // Update progress only when playing
            if flight_playing {
                // Get max walk length from selected walks
                let max_len = walks.iter()
                    .filter(|(id, w)| selected_sources.contains(*id) && w.visible && !w.points.is_empty())
                    .map(|(_, w)| w.points.len())
                    .max()
                    .unwrap_or(1000) as f32;

                // flight_speed is in Hz (points per second)
                // progress_per_second = points_per_second / total_points
                let progress_per_second = flight_speed / max_len;
                let progress_delta = progress_per_second * delta;

                if flight_reverse {
                    flight_progress -= progress_delta;
                    if flight_progress <= 0.0 {
                        if flight_loop {
                            if flight_loop_mode == 1 {
                                // Fwd/Rev: reverse direction
                                flight_reverse = false;
                                flight_progress = 0.0;
                            } else {
                                // Repeat: jump to end
                                flight_progress = 1.0;
                            }
                        } else {
                            flight_progress = 0.0;
                        }
                    }
                } else {
                    flight_progress += progress_delta;
                    if flight_progress >= 1.0 {
                        if flight_loop {
                            if flight_loop_mode == 1 {
                                // Fwd/Rev: reverse direction
                                flight_reverse = true;
                                flight_progress = 1.0;
                            } else {
                                // Repeat: jump to start
                                flight_progress = 0.0;
                            }
                        } else {
                            flight_progress = 1.0;
                        }
                    }
                }
            }

            // Get average position along path
            if let Some((current_pos, next_pos)) = calculate_average_path_position(
                &walks, &selected_sources, flight_progress
            ) {
                // Direction of travel
                let dir_vec = next_pos - current_pos;
                let dir_mag = dir_vec.magnitude();

                if dir_mag > 0.001 {
                    let direction = dir_vec / dir_mag;

                    // Camera offset: slightly behind and above the path
                    let up = vec3(0.0, 1.0, 0.0);
                    let right_vec = direction.cross(up);
                    let right_mag = right_vec.magnitude();
                    let cam_up = if right_mag > 0.001 {
                        (right_vec / right_mag).cross(direction).normalize()
                    } else {
                        up
                    };

                    let offset_distance = 15.0;
                    let offset_height = 5.0;
                    let camera_pos = current_pos - direction * offset_distance + cam_up * offset_height;

                    // Look target: ahead or behind based on toggle
                    let look_target = if flight_look_back {
                        current_pos - direction * 10.0
                    } else {
                        next_pos
                    };

                    camera.set_view(camera_pos, look_target, up);
                }
            }
        }

        // Handle orbit control - only when cursor is in plot area (not over GUI panels)
        // Side panel is 250px wide, bottom panel is ~25px
        let panel_width = 260.0;
        let bottom_panel_height = 30.0;
        let in_plot_area = frame_input.events.iter().all(|event| {
            match event {
                three_d::Event::MousePress { position, .. } |
                three_d::Event::MouseRelease { position, .. } |
                three_d::Event::MouseMotion { position, .. } |
                three_d::Event::MouseWheel { position, .. } => {
                    position.x > panel_width &&
                    position.y < (frame_input.viewport.height as f32 - bottom_panel_height)
                }
                _ => true,
            }
        });

        // Disable orbit control when in flight mode
        if in_plot_area && !flight_mode {
            orbit_control.handle_events(&mut camera, &mut frame_input.events);
        }
        camera.set_viewport(frame_input.viewport);

        // Build line geometry for visible walks
        let mut walk_lines: Vec<Gm<InstancedMesh, ColorMaterial>> = Vec::new();
        let mut walk_points: Vec<Gm<InstancedMesh, ColorMaterial>> = Vec::new();

        for (_id, walk) in &walks {
            if !walk.visible || walk.points.is_empty() {
                continue;
            }

            let color = Srgba::new(
                (walk.color[0] * 255.0) as u8,
                (walk.color[1] * 255.0) as u8,
                (walk.color[2] * 255.0) as u8,
                255,
            );

            // Lines (cones scaled by visit count)
            if show_lines && walk.points.len() >= 2 {
                let mut instances = Instances::default();
                instances.transformations = Vec::new();
                instances.colors = Some(Vec::new());

                let max_revisits = walk.revisit_counts.values().max().copied().unwrap_or(1) as f32;
                let ln_max = max_revisits.ln().max(1.0);

                for i in 0..walk.points.len() - 1 {
                    let p1 = vec3(walk.points[i][0], walk.points[i][1], walk.points[i][2]);
                    let p2 = vec3(walk.points[i + 1][0], walk.points[i + 1][1], walk.points[i + 1][2]);

                    let center = (p1 + p2) * 0.5;
                    let dir = p2 - p1;
                    let length = dir.magnitude();

                    if length > 0.001 {
                        // Look up visit counts at both endpoints
                        let key1 = (
                            walk.points[i][0].round() as i32,
                            walk.points[i][1].round() as i32,
                            walk.points[i][2].round() as i32,
                        );
                        let key2 = (
                            walk.points[i + 1][0].round() as i32,
                            walk.points[i + 1][1].round() as i32,
                            walk.points[i + 1][2].round() as i32,
                        );
                        let count1 = *walk.revisit_counts.get(&key1).unwrap_or(&1) as f32;
                        let count2 = *walk.revisit_counts.get(&key2).unwrap_or(&1) as f32;
                        let avg_count = (count1 + count2) * 0.5;

                        // Scale radius by visit count (log scale)
                        let radius = line_scale
                            * (0.15 + 0.85 * avg_count.ln().max(0.0) / ln_max);

                        let up = vec3(0.0, 1.0, 0.0);
                        let rotation = if dir.normalize().dot(up).abs() > 0.999 {
                            Mat4::identity()
                        } else {
                            let axis = up.cross(dir.normalize()).normalize();
                            let angle = up.dot(dir.normalize()).acos();
                            Mat4::from_axis_angle(axis, radians(angle))
                        };

                        let transform = Mat4::from_translation(center)
                            * rotation
                            * Mat4::from_nonuniform_scale(radius, length * 0.5, radius);

                        instances.transformations.push(transform);
                        if let Some(ref mut colors) = instances.colors {
                            colors.push(color);
                        }
                    }
                }

                if !instances.transformations.is_empty() {
                    let cone = CpuMesh::cone(12);
                    let instanced = Gm::new(
                        InstancedMesh::new(&context, &instances, &cone),
                        ColorMaterial::default(),
                    );
                    walk_lines.push(instanced);
                }
            }

            // Points (spheres scaled by revisit count)
            if show_points {
                let mut instances = Instances::default();
                instances.transformations = Vec::new();
                instances.colors = Some(Vec::new());

                // Get max revisit count for scaling
                let max_revisits = walk.revisit_counts.values().max().copied().unwrap_or(1) as f32;

                // Render a sphere at each unique position
                for (&(x, y, z), &count) in &walk.revisit_counts {
                    // Scale sphere size based on revisit count (log scale for better visibility)
                    let base_size = 0.8 * point_scale;
                    let scale_factor = 1.0 + (count as f32).ln().max(0.0) / max_revisits.ln().max(1.0) * 2.0;
                    let size = base_size * scale_factor;

                    let transform = Mat4::from_translation(vec3(x as f32, y as f32, z as f32))
                        * Mat4::from_scale(size);

                    instances.transformations.push(transform);

                    // Color intensity based on revisit count
                    let intensity = 0.5 + 0.5 * (count as f32 / max_revisits).sqrt();
                    let point_color = Srgba::new(
                        ((walk.color[0] * intensity) * 255.0).min(255.0) as u8,
                        ((walk.color[1] * intensity) * 255.0).min(255.0) as u8,
                        ((walk.color[2] * intensity) * 255.0).min(255.0) as u8,
                        255,
                    );
                    if let Some(ref mut colors) = instances.colors {
                        colors.push(point_color);
                    }
                }

                if !instances.transformations.is_empty() {
                    let sphere = CpuMesh::sphere(8);
                    let instanced = Gm::new(
                        InstancedMesh::new(&context, &instances, &sphere),
                        ColorMaterial::default(),
                    );
                    walk_points.push(instanced);
                }
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

                    // Base and mapping selectors
                    ui.horizontal(|ui| {
                        ui.label("Base:");
                        egui::ComboBox::from_id_salt("base")
                            .width(40.0)
                            .selected_text(format!("{}", selected_base))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut selected_base, 12, "12");
                                ui.selectable_value(&mut selected_base, 4, "4");
                            });
                        ui.label("Mapping:");
                        let mapping_enabled = selected_base == 12;
                        ui.add_enabled_ui(mapping_enabled, |ui| {
                            egui::ComboBox::from_id_salt("mapping")
                                .selected_text(&selected_mapping)
                                .show_ui(ui, |ui| {
                                    for name in config.mappings.keys() {
                                        ui.selectable_value(&mut selected_mapping, name.clone(), name);
                                    }
                                });
                        });
                    });

                    ui.add(egui::Slider::new(&mut max_points, 100..=10000).text("Max points"));

                    ui.horizontal(|ui| {
                        if ui.button("Deselect All").clicked() {
                            selected_sources.clear();
                            walks.clear();
                            color_pool.clear();
                        }
                        if ui.button("Center View").clicked() {
                            camera.set_view(
                                vec3(0.0, 50.0, 200.0),
                                vec3(0.0, 0.0, 0.0),
                                vec3(0.0, 1.0, 0.0),
                            );
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.checkbox(&mut show_grid, "Grid");
                        ui.checkbox(&mut show_axes, "Axes");
                        ui.checkbox(&mut auto_rotate, "Auto-rotate");
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut show_points, "Points");
                        ui.checkbox(&mut show_lines, "Lines");
                    });
                    if show_points {
                        ui.add(egui::Slider::new(&mut point_scale, 0.1..=1.0).text("Point scale"));
                    }
                    if show_lines {
                        ui.add(egui::Slider::new(&mut line_scale, 0.05..=2.0).text("Line scale"));
                    }
                    ui.horizontal(|ui| {
                        ui.label("Ticks:");
                        egui::ComboBox::from_id_salt("axis_ticks")
                            .width(60.0)
                            .selected_text(if axis_ticks == 0 { "Off".to_string() } else { axis_ticks.to_string() })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut axis_ticks, 0, "Off");
                                ui.selectable_value(&mut axis_ticks, 5, "5");
                                ui.selectable_value(&mut axis_ticks, 10, "10");
                                ui.selectable_value(&mut axis_ticks, 20, "20");
                                ui.selectable_value(&mut axis_ticks, 50, "50");
                            });
                    });

                    ui.horizontal(|ui| {
                        if ui.button("SpaceMouse Config").clicked() {
                            show_spacemouse_config = !show_spacemouse_config;
                        }
                        if ui.button("Screenshot").clicked() {
                            screenshot_requested = true;
                        }
                    });

                    // Data Flight controls
                    ui.separator();
                    ui.heading("Data Flight");
                    ui.checkbox(&mut flight_mode, "Enable Flight");

                    if flight_mode {
                        // Play/Pause button with spacebar hint
                        ui.horizontal(|ui| {
                            let button_text = if flight_playing { "⏸ Pause" } else { "▶ Play" };
                            if ui.button(button_text).clicked() {
                                flight_playing = !flight_playing;
                            }
                            ui.label("(Space)");
                        });

                        ui.horizontal(|ui| {
                            ui.label("Speed:");
                            ui.add(egui::Slider::new(&mut flight_speed, 0.01..=60.0).logarithmic(true).suffix(" Hz"));
                        });

                        ui.horizontal(|ui| {
                            ui.label("Progress:");
                            ui.add(egui::Slider::new(&mut flight_progress, 0.0..=1.0));
                        });

                        ui.horizontal(|ui| {
                            ui.checkbox(&mut flight_reverse, "Reverse");
                            ui.checkbox(&mut flight_look_back, "Look Back");
                        });

                        // Loop control: checkbox enables, label click opens dropdown
                        ui.horizontal(|ui| {
                            // Checkbox without label
                            if ui.checkbox(&mut flight_loop, "").changed() {
                                // Just toggling loop on/off
                            }

                            // Clickable label that opens mode dropdown
                            let loop_mode_text = if flight_loop_mode == 1 { "Loop: Fwd/Rev" } else { "Loop: Repeat" };
                            let label_response = ui.selectable_label(show_loop_menu, loop_mode_text);
                            if label_response.clicked() {
                                show_loop_menu = !show_loop_menu;
                            }
                        });

                        // Dropdown menu for loop mode
                        if show_loop_menu {
                            ui.horizontal(|ui| {
                                ui.label("  ");  // indent
                                if ui.selectable_label(flight_loop_mode == 0, "Repeat").clicked() {
                                    flight_loop_mode = 0;
                                    show_loop_menu = false;
                                }
                                if ui.selectable_label(flight_loop_mode == 1, "Fwd/Rev").clicked() {
                                    flight_loop_mode = 1;
                                    show_loop_menu = false;
                                }
                            });
                        }

                        if ui.button("Reset to Start").clicked() {
                            flight_progress = if flight_reverse { 1.0 } else { 0.0 };
                            flight_playing = false;
                        }
                    }

                    ui.separator();

                    // Source list
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (category, sources) in &by_category {
                            let cat_name = config.categories.get(category).unwrap_or(category);
                            egui::CollapsingHeader::new(cat_name).show(ui, |ui| {
                                for source in sources {
                                    let mut checked = selected_sources.contains(&source.id);

                                    // Check if data is available (raw files for downloaded data, or math sources)
                                    let is_available = check_data_exists(&source.id, &source.converter, &source.url);

                                    if is_available {
                                        if ui.checkbox(&mut checked, &source.name).changed() {
                                            if checked {
                                                selected_sources.insert(source.id.clone());
                                                // Get color from pool for maximum contrast
                                                let color = color_pool.get_color(&source.id);
                                                // Load walk
                                                if let Some(walk_data) = load_walk_data(&source, &config, max_points, &selected_mapping, selected_base, color) {
                                                    walks.insert(source.id.clone(), walk_data);
                                                }
                                            } else {
                                                selected_sources.remove(&source.id);
                                                walks.remove(&source.id);
                                                // Release color back to pool
                                                color_pool.release_color(&source.id);
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
                        // Show screenshot status if active
                        if let Some((ref msg, expire_time)) = screenshot_status {
                            if frame_input.accumulated_time < expire_time {
                                ui.label(egui::RichText::new(msg).color(egui::Color32::GREEN));
                            } else {
                                screenshot_status = None;
                            }
                        } else {
                            ui.label(format!("{} walks loaded", walks.len()));
                            ui.separator();
                            ui.label("Right-drag: orbit | Scroll: zoom | Middle-drag: pan");
                        }
                    });
                });

                // Axis labels in screen space
                if show_axes {
                    let painter = egui_ctx.layer_painter(egui::LayerId::new(
                        egui::Order::Foreground,
                        egui::Id::new("axis_labels"),
                    ));

                    // Helper to project world pos to screen using UV coordinates
                    // UV coords are 0-1 normalized, convert to logical screen points
                    let vp = frame_input.viewport;
                    let dpr = frame_input.device_pixel_ratio;
                    let screen_width = vp.width as f32 / dpr;
                    let screen_height = vp.height as f32 / dpr;

                    let project = |world_pos: Vec3| -> Option<egui::Pos2> {
                        // Check if position is in front of camera
                        let view_dir = camera.view_direction();
                        let to_point = world_pos - camera.position();
                        if view_dir.dot(to_point) <= 0.0 {
                            return None;
                        }

                        let uv = camera.uv_coordinates_at_position(world_pos);
                        // UV: (0,0) is bottom-left, (1,1) is top-right
                        // Screen: (0,0) is top-left, so flip Y
                        let screen_x = uv.u * screen_width;
                        let screen_y = (1.0 - uv.v) * screen_height;
                        Some(egui::pos2(screen_x, screen_y))
                    };

                    // Axis name labels at ends
                    let axis_label_pos: [(&str, Vec3, egui::Color32); 3] = [
                        ("X", vec3(105.0, 0.0, 0.0), egui::Color32::from_rgb(220, 50, 50)),
                        ("Y", vec3(0.0, 105.0, 0.0), egui::Color32::from_rgb(50, 220, 50)),
                        ("Z", vec3(0.0, 0.0, 105.0), egui::Color32::from_rgb(50, 100, 220)),
                    ];

                    for (label, world_pos, color) in &axis_label_pos {
                        if let Some(screen_pos) = project(*world_pos) {
                            painter.text(
                                screen_pos,
                                egui::Align2::CENTER_CENTER,
                                *label,
                                egui::FontId::proportional(16.0),
                                *color,
                            );
                        }
                    }

                    // Numeric tick labels - directly on axes, no offset
                    if axis_ticks > 0 {
                        let spacing = axis_ticks as f32;
                        let axis_len = 100.0;

                        // X axis ticks (red)
                        let x_color = egui::Color32::from_rgb(220, 120, 120);
                        let mut pos = spacing;
                        while pos <= axis_len {
                            if let Some(screen_pos) = project(vec3(pos, 0.0, 0.0)) {
                                painter.text(
                                    screen_pos,
                                    egui::Align2::CENTER_CENTER,
                                    format!("{}", pos as i32),
                                    egui::FontId::proportional(11.0),
                                    x_color,
                                );
                            }
                            pos += spacing;
                        }

                        // Y axis ticks (green)
                        let y_color = egui::Color32::from_rgb(120, 220, 120);
                        let mut pos = spacing;
                        while pos <= axis_len {
                            if let Some(screen_pos) = project(vec3(0.0, pos, 0.0)) {
                                painter.text(
                                    screen_pos,
                                    egui::Align2::CENTER_CENTER,
                                    format!("{}", pos as i32),
                                    egui::FontId::proportional(11.0),
                                    y_color,
                                );
                            }
                            pos += spacing;
                        }

                        // Z axis ticks (blue)
                        let z_color = egui::Color32::from_rgb(120, 150, 220);
                        let mut pos = spacing;
                        while pos <= axis_len {
                            if let Some(screen_pos) = project(vec3(0.0, 0.0, pos)) {
                                painter.text(
                                    screen_pos,
                                    egui::Align2::CENTER_CENTER,
                                    format!("{}", pos as i32),
                                    egui::FontId::proportional(11.0),
                                    z_color,
                                );
                            }
                            pos += spacing;
                        }
                    }
                }

                // SpaceMouse config window
                egui::Window::new("SpaceMouse Config")
                    .open(&mut show_spacemouse_config)
                    .resizable(false)
                    .show(egui_ctx, |ui| {
                        let axis_labels = ["TX", "TY", "TZ", "RX", "RY", "RZ"];

                        ui.heading("Axis Mapping");
                        egui::Grid::new("axis_map").num_columns(2).show(ui, |ui| {
                            let mappings: &mut [(&str, &mut usize)] = &mut [
                                ("Pan X",  &mut spacemouse_config.pan_x_axis),
                                ("Pan Y",  &mut spacemouse_config.pan_y_axis),
                                ("Zoom",   &mut spacemouse_config.zoom_axis),
                                ("Rot X",  &mut spacemouse_config.rot_x_axis),
                                ("Rot Y",  &mut spacemouse_config.rot_y_axis),
                                ("Rot Z",  &mut spacemouse_config.rot_z_axis),
                            ];
                            for (label, value) in mappings.iter_mut() {
                                ui.label(*label);
                                egui::ComboBox::from_id_salt(format!("sm_{}", label))
                                    .width(50.0)
                                    .selected_text(axis_labels[**value])
                                    .show_ui(ui, |ui| {
                                        for (i, name) in axis_labels.iter().enumerate() {
                                            ui.selectable_value(*value, i, *name);
                                        }
                                    });
                                ui.end_row();
                            }
                        });

                        ui.separator();
                        ui.heading("Invert Axes");
                        ui.horizontal(|ui| {
                            for (i, label) in axis_labels.iter().enumerate() {
                                ui.checkbox(&mut spacemouse_config.invert[i], *label);
                            }
                        });

                        ui.separator();
                        ui.add(egui::Slider::new(&mut spacemouse_config.sensitivity, 0.1..=5.0).text("Sensitivity"));

                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("Save").clicked() {
                                spacemouse_config.save();
                            }
                            if ui.button("Reset").clicked() {
                                spacemouse_config = SpaceMouseConfig::default();
                            }
                        });
                    });
            },
        );

        // Regenerate walks if mapping, base, or max_points changed
        if selected_mapping != prev_mapping || max_points != prev_max_points || selected_base != prev_base {
            prev_mapping = selected_mapping.clone();
            prev_max_points = max_points;
            prev_base = selected_base;
            let source_ids: Vec<String> = selected_sources.iter().cloned().collect();
            for sid in &source_ids {
                if let Some(source) = config.sources.iter().find(|s| &s.id == sid) {
                    // Reuse existing color assignment
                    let color = color_pool.get_color(&sid);
                    if let Some(walk_data) = load_walk_data(source, &config, max_points, &selected_mapping, selected_base, color) {
                        walks.insert(sid.clone(), walk_data);
                    }
                }
            }
        }

        // Clear and render
        frame_input.screen().clear(ClearState::color_and_depth(0.1, 0.1, 0.15, 1.0, 1.0));

        // Render grid
        for grid_obj in &grid_objects {
            grid_obj.render(&camera, &[]);
        }

        // Render axes
        if show_axes {
            let axis_len = 100.0;
            let axis_radius = 0.4;
            let axes_data: [(Vec3, Srgba); 3] = [
                (vec3(1.0, 0.0, 0.0), Srgba::new(220, 50, 50, 255)),   // X = red
                (vec3(0.0, 1.0, 0.0), Srgba::new(50, 220, 50, 255)),   // Y = green
                (vec3(0.0, 0.0, 1.0), Srgba::new(50, 100, 220, 255)),  // Z = blue
            ];
            for (dir, color) in &axes_data {
                let center = *dir * (axis_len * 0.5);
                let up = vec3(0.0, 1.0, 0.0);
                let rotation = if dir.dot(up).abs() > 0.999 {
                    Mat4::identity()
                } else {
                    let axis = up.cross(*dir).normalize();
                    let angle = up.dot(*dir).acos();
                    Mat4::from_axis_angle(axis, radians(angle))
                };
                let transform = Mat4::from_translation(center)
                    * rotation
                    * Mat4::from_nonuniform_scale(axis_radius, axis_len * 0.5, axis_radius);

                let mut instances = Instances::default();
                instances.transformations = vec![transform];
                instances.colors = Some(vec![*color]);

                let cylinder = CpuMesh::cylinder(8);
                let axis_obj = Gm::new(
                    InstancedMesh::new(&context, &instances, &cylinder),
                    ColorMaterial::default(),
                );
                axis_obj.render(&camera, &[]);
            }
        }

        // Render axis tick marks
        if show_axes && axis_ticks > 0 {
            let tick_size = 1.5;
            let tick_radius = 0.3;
            let spacing = axis_ticks as f32;
            let axis_len = 100.0;
            let tick_color = Srgba::new(180, 180, 180, 255);

            let mut instances = Instances::default();
            instances.transformations = Vec::new();
            instances.colors = Some(Vec::new());

            let axes_dirs: [Vec3; 3] = [
                vec3(1.0, 0.0, 0.0),
                vec3(0.0, 1.0, 0.0),
                vec3(0.0, 0.0, 1.0),
            ];
            // Perpendicular directions for tick orientation
            let tick_perps: [Vec3; 3] = [
                vec3(0.0, 1.0, 0.0), // X ticks point up
                vec3(1.0, 0.0, 0.0), // Y ticks point right
                vec3(0.0, 1.0, 0.0), // Z ticks point up
            ];

            for (dir, perp) in axes_dirs.iter().zip(tick_perps.iter()) {
                let mut pos = spacing;
                while pos <= axis_len {
                    let center = *dir * pos;
                    let up = vec3(0.0, 1.0, 0.0);
                    let rotation = if perp.dot(up).abs() > 0.999 {
                        Mat4::identity()
                    } else {
                        let axis = up.cross(*perp).normalize();
                        let angle = up.dot(*perp).acos();
                        Mat4::from_axis_angle(axis, radians(angle))
                    };
                    let transform = Mat4::from_translation(center)
                        * rotation
                        * Mat4::from_nonuniform_scale(tick_radius, tick_size, tick_radius);
                    instances.transformations.push(transform);
                    if let Some(ref mut colors) = instances.colors {
                        colors.push(tick_color);
                    }
                    pos += spacing;
                }
            }

            if !instances.transformations.is_empty() {
                let cylinder = CpuMesh::cylinder(6);
                let ticks_obj = Gm::new(
                    InstancedMesh::new(&context, &instances, &cylinder),
                    ColorMaterial::default(),
                );
                ticks_obj.render(&camera, &[]);
            }
        }

        // Render walks
        for walk_obj in &walk_lines {
            walk_obj.render(&camera, &[]);
        }

        // Render walk points
        for point_obj in &walk_points {
            point_obj.render(&camera, &[]);
        }

        // Render GUI
        let _ = frame_input.screen().write(|| gui.render());

        // Screenshot capture
        if screenshot_requested {
            screenshot_requested = false;
            let vp = frame_input.viewport;
            let pixels: Vec<[u8; 4]> = frame_input.screen().read_color();
            let flat: Vec<u8> = pixels.iter().flat_map(|p| p.iter().copied()).collect();
            if let Some(img) = image::RgbaImage::from_raw(vp.width, vp.height, flat) {
                let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
                let screenshots_dir = std::path::PathBuf::from("screenshots");
                let _ = std::fs::create_dir_all(&screenshots_dir);
                let filename = screenshots_dir.join(format!("data_walker_{}.png", timestamp));
                // Get absolute path for display
                let abs_path = std::env::current_dir()
                    .map(|p| p.join(&filename).display().to_string())
                    .unwrap_or_else(|_| filename.display().to_string());
                match img.save(&filename) {
                    Ok(()) => {
                        info!("Screenshot saved to {}", abs_path);
                        screenshot_status = Some((
                            format!("Saved: {}", abs_path),
                            frame_input.accumulated_time + 4.0, // Show for 4 seconds
                        ));
                    }
                    Err(e) => {
                        warn!("Failed to save screenshot: {}", e);
                        screenshot_status = Some((
                            format!("Failed: {}", e),
                            frame_input.accumulated_time + 4.0,
                        ));
                    }
                }
            }
        }

        FrameOutput::default()
    });

    Ok(())
}

fn check_data_exists(id: &str, converter: &str, url: &str) -> bool {
    match converter {
        "audio" => {
            // Check for WAV or MP3
            std::path::Path::new(&format!("data/audio/{}.wav", id)).exists() ||
            std::path::Path::new(&format!("data/audio/{}.mp3", id)).exists()
        }
        "dna" => {
            // Extract accession from URL
            let accession = url.rsplit('/').next().unwrap_or(id);
            std::path::Path::new(&format!("data/dna/{}.fasta", accession.replace(".", "_"))).exists()
        }
        "cosmos" => {
            std::path::Path::new(&format!("data/cosmos/{}.txt.gz", id)).exists()
        }
        "finance" => {
            let symbol = url.split('/').last().unwrap_or(id)
                .replace("%5E", "^")
                .replace("^", "")
                .replace("-", "_");
            std::path::Path::new(&format!("data/finance/{}.json", symbol)).exists()
        }
        c if c.starts_with("math.") => true, // Math is computed, always available
        _ => false,
    }
}

fn load_walk_data(
    source: &crate::config::Source,
    config: &Config,
    max_points: usize,
    mapping_name: &str,
    base: u32,
    color: [f32; 3],
) -> Option<WalkData> {
    use crate::converters;
    use crate::walk::walk_base4;

    // All conversion happens on-the-fly - no pre-computed storage
    let digits = if source.converter.starts_with("math.") {
        // Math always generates base-12; for base-4, reduce mod 4
        let base12 = MathGenerator::from_converter_string(&source.converter)?.generate(max_points);
        if base == 4 {
            base12.iter().map(|&d| d % 4).collect()
        } else {
            base12
        }
    } else {
        match source.converter.as_str() {
            "audio" => {
                let wav_path = std::path::PathBuf::from(format!("data/audio/{}.wav", source.id));
                let mp3_path = std::path::PathBuf::from(format!("data/audio/{}.mp3", source.id));

                let path = if wav_path.exists() {
                    wav_path
                } else if mp3_path.exists() {
                    mp3_path
                } else {
                    warn!("No audio file found for {}", source.id);
                    return None;
                };

                match converters::load_audio_raw(&path, base) {
                    Ok(data) => data,
                    Err(e) => {
                        warn!("Failed to convert audio {}: {}", source.id, e);
                        return None;
                    }
                }
            }
            "dna" => {
                let accession = source.url.rsplit('/').next().unwrap_or(&source.id);
                let path = std::path::PathBuf::from(format!("data/dna/{}.fasta", accession.replace(".", "_")));

                if !path.exists() {
                    warn!("No FASTA file found for {}: {:?}", source.id, path);
                    return None;
                }

                match converters::load_dna_raw(&path, base) {
                    Ok(data) => data,
                    Err(e) => {
                        warn!("Failed to convert DNA {}: {}", source.id, e);
                        return None;
                    }
                }
            }
            "cosmos" => {
                let path = std::path::PathBuf::from(format!("data/cosmos/{}.txt.gz", source.id));

                if !path.exists() {
                    warn!("No cosmos file found for {}: {:?}", source.id, path);
                    return None;
                }

                match converters::load_cosmos_raw(&path, base) {
                    Ok(data) => data,
                    Err(e) => {
                        warn!("Failed to convert cosmos {}: {}", source.id, e);
                        return None;
                    }
                }
            }
            "finance" => {
                let symbol = source.url.split('/').last().unwrap_or(&source.id)
                    .replace("%5E", "^")
                    .replace("^", "")
                    .replace("-", "_");
                let path = std::path::PathBuf::from(format!("data/finance/{}.json", symbol));

                if !path.exists() {
                    warn!("No finance file found for {}: {:?}", source.id, path);
                    return None;
                }

                match converters::load_finance_raw(&path, base) {
                    Ok(data) => data,
                    Err(e) => {
                        warn!("Failed to convert finance {}: {}", source.id, e);
                        return None;
                    }
                }
            }
            _ => return None,
        }
    };

    info!("Loaded {} base-{} digits for {}", digits.len(), base, source.id);

    let points = if base == 4 {
        walk_base4(&digits, max_points)
    } else {
        let mapping = config.mappings.get(mapping_name)
            .map(|v| {
                let mut arr = [0u8; 12];
                for (i, &val) in v.iter().enumerate().take(12) {
                    arr[i] = val;
                }
                arr
            })
            .unwrap_or([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
        walk_base12(&digits, &mapping, max_points)
    };
    info!("Generated {} walk points for {}", points.len(), source.id);

    // Compute revisit counts - round positions to integer grid
    let mut revisit_counts: std::collections::HashMap<(i32, i32, i32), u32> = std::collections::HashMap::new();
    for p in &points {
        let key = (p[0].round() as i32, p[1].round() as i32, p[2].round() as i32);
        *revisit_counts.entry(key).or_insert(0) += 1;
    }
    let max_revisits = revisit_counts.values().max().copied().unwrap_or(1);
    info!("Max revisits for {}: {} at {} unique positions", source.id, max_revisits, revisit_counts.len());

    Some(WalkData {
        name: source.name.clone(),
        points,
        color,
        visible: true,
        revisit_counts,
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

/// Maximally distinct color palette based on Glasbey algorithm principles.
/// Colors are pre-computed to maximize minimum perceptual distance in CIELAB space.
/// Designed for dark backgrounds (0.1, 0.1, 0.15).
/// First 12 colors have excellent contrast; extends to 24 for larger datasets.
const DISTINCT_COLORS: [[f32; 3]; 24] = [
    // Primary distinct colors (maximally separated in CIELAB)
    [1.00, 0.30, 0.30],  // 0: Bright red
    [0.30, 0.85, 0.40],  // 1: Green
    [0.40, 0.60, 1.00],  // 2: Sky blue
    [1.00, 0.85, 0.20],  // 3: Yellow
    [0.90, 0.40, 0.90],  // 4: Magenta
    [0.20, 0.90, 0.90],  // 5: Cyan
    [1.00, 0.60, 0.20],  // 6: Orange
    [0.70, 0.50, 1.00],  // 7: Purple
    [0.60, 1.00, 0.60],  // 8: Lime
    [1.00, 0.50, 0.60],  // 9: Salmon
    [0.50, 0.80, 1.00],  // 10: Light blue
    [0.95, 0.75, 0.50],  // 11: Peach
    // Secondary colors (still good contrast)
    [0.80, 0.20, 0.50],  // 12: Rose
    [0.40, 0.70, 0.50],  // 13: Sea green
    [0.70, 0.70, 0.30],  // 14: Olive
    [0.50, 0.30, 0.70],  // 15: Deep purple
    [0.30, 0.60, 0.60],  // 16: Teal
    [0.90, 0.55, 0.45],  // 17: Coral
    [0.60, 0.40, 0.30],  // 18: Brown
    [0.75, 0.85, 0.85],  // 19: Light gray-cyan
    [0.85, 0.65, 0.80],  // 20: Pink
    [0.55, 0.75, 0.35],  // 21: Yellow-green
    [0.45, 0.45, 0.85],  // 22: Slate blue
    [0.90, 0.35, 0.60],  // 23: Hot pink
];

/// Color pool for dynamic assignment with maximum contrast
#[derive(Default)]
struct ColorPool {
    /// Maps source ID to assigned color index
    assignments: std::collections::HashMap<String, usize>,
    /// Tracks which color indices are in use
    in_use: std::collections::HashSet<usize>,
}

impl ColorPool {
    fn new() -> Self {
        Self::default()
    }

    /// Get or assign a color for a source ID
    /// Returns the RGB color array
    fn get_color(&mut self, source_id: &str) -> [f32; 3] {
        // Return existing assignment
        if let Some(&idx) = self.assignments.get(source_id) {
            return DISTINCT_COLORS[idx];
        }

        // Find first unused color index
        let mut idx = 0;
        while self.in_use.contains(&idx) && idx < DISTINCT_COLORS.len() {
            idx += 1;
        }

        // If all colors used, find color with maximum distance to currently used colors
        if idx >= DISTINCT_COLORS.len() {
            idx = self.find_best_reuse_color();
        }

        self.assignments.insert(source_id.to_string(), idx);
        self.in_use.insert(idx);
        DISTINCT_COLORS[idx]
    }

    /// Release a color when a walk is removed
    fn release_color(&mut self, source_id: &str) {
        if let Some(idx) = self.assignments.remove(source_id) {
            self.in_use.remove(&idx);
        }
    }

    /// Find the best color to reuse when all are taken
    /// Uses simple RGB distance (not CIELAB but fast)
    fn find_best_reuse_color(&self) -> usize {
        // Collect currently used colors
        let used_colors: Vec<[f32; 3]> = self.in_use.iter()
            .filter(|&&i| i < DISTINCT_COLORS.len())
            .map(|&i| DISTINCT_COLORS[i])
            .collect();

        if used_colors.is_empty() {
            return 0;
        }

        // Find palette color with maximum minimum distance to used colors
        let mut best_idx = 0;
        let mut best_min_dist = 0.0f32;

        for (idx, color) in DISTINCT_COLORS.iter().enumerate() {
            let min_dist = used_colors.iter()
                .map(|used| {
                    let dr = color[0] - used[0];
                    let dg = color[1] - used[1];
                    let db = color[2] - used[2];
                    // Weighted RGB distance (green is more perceptually significant)
                    (2.0 * dr * dr + 4.0 * dg * dg + 3.0 * db * db).sqrt()
                })
                .fold(f32::INFINITY, f32::min);

            if min_dist > best_min_dist {
                best_min_dist = min_dist;
                best_idx = idx;
            }
        }

        best_idx
    }

    /// Clear all assignments
    fn clear(&mut self) {
        self.assignments.clear();
        self.in_use.clear();
    }
}
