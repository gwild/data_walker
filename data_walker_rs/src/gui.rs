//! Native GUI viewer using three-d for real 3D rendering
//!
//! Proper 3D visualization with orbit camera and SpaceMouse support

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{info, warn, error, debug};
use three_d::*;
use three_d::egui;

use crate::config::{Config, DataPaths};
use crate::walk::walk_base12;
use crate::converters::math::MathGenerator;
use crate::audio::{AudioEngine, AudioSettings, MixingMode, SynthMethod, SourceType};
use crate::automation::{AutomationConfig, AutoCommand, GuiState, GuiEvent, WalkInfo};
use crate::rules::{
    validate_digit_playback_rule,
    validate_step_trigger_playback_rule,
    enforce_zero_tolerance_rule_hook,
    validate_zero_tolerance_rules,
};

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
    // Point revisit counts keyed by quantized position to absorb float jitter.
    revisit_counts: std::collections::HashMap<(i64, i64, i64), u32>,
    // Representative render position for each quantized key.
    point_positions: std::collections::HashMap<(i64, i64, i64), [f32; 3]>,
    // Optional per-point colors (for PDB structure coloring by residue)
    point_colors: Option<Vec<[f32; 3]>>,
    // Chain break indices - don't draw bonds between point[i - 1] and point[i].
    chain_breaks: Option<Vec<usize>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum ColorScheme {
    #[default]
    Distinct,
    Wong,
    Tol,
    Pastel,
    Neon,
    Earth,
}

impl ColorScheme {
    fn label(self) -> &'static str {
        match self {
            ColorScheme::Distinct => "Distinct",
            ColorScheme::Wong => "Wong",
            ColorScheme::Tol => "Tol",
            ColorScheme::Pastel => "Pastel",
            ColorScheme::Neon => "Neon",
            ColorScheme::Earth => "Earth",
        }
    }
}

struct PendingWalkLoad {
    request_id: u64,
    source_name: String,
}

struct WalkLoadResult {
    request_id: u64,
    source: crate::config::Source,
    walk_data: Option<WalkData>,
    prepare_audio: bool,
}

struct PendingAudioPrep {
    request_id: u64,
    source_name: String,
}

struct AudioPrepResult {
    request_id: u64,
    source: crate::config::Source,
    result: anyhow::Result<PreparedAudioFile>,
}

struct PreparedAudioFile {
    path: PathBuf,
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
}

const POSITION_KEY_SCALE: f32 = 1000.0;

fn point_key(point: [f32; 3]) -> (i64, i64, i64) {
    (
        (point[0] * POSITION_KEY_SCALE).round() as i64,
        (point[1] * POSITION_KEY_SCALE).round() as i64,
        (point[2] * POSITION_KEY_SCALE).round() as i64,
    )
}

fn build_point_visit_maps(
    points: &[[f32; 3]],
) -> (
    std::collections::HashMap<(i64, i64, i64), u32>,
    std::collections::HashMap<(i64, i64, i64), [f32; 3]>,
) {
    let mut revisit_counts = std::collections::HashMap::new();
    let mut point_positions = std::collections::HashMap::new();

    for &point in points {
        let key = point_key(point);
        *revisit_counts.entry(key).or_insert(0) += 1;
        point_positions.entry(key).or_insert(point);
    }

    (revisit_counts, point_positions)
}

/// Interpolate along the segment between adjacent walk points.
fn interpolate_walk_position(points: &[[f32; 3]], position: f32) -> Vec3 {
    if points.is_empty() {
        return vec3(0.0, 0.0, 0.0);
    }
    if points.len() == 1 {
        return vec3(points[0][0], points[0][1], points[0][2]);
    }
    let max_segment = points.len() - 1;
    let exact_idx = position.clamp(0.0, max_segment as f32);
    let start_idx = exact_idx.floor() as usize;
    let end_idx = (start_idx + 1).min(max_segment);
    let t = exact_idx - start_idx as f32;
    let start = vec3(
        points[start_idx][0],
        points[start_idx][1],
        points[start_idx][2],
    );
    let end = vec3(points[end_idx][0], points[end_idx][1], points[end_idx][2]);
    start * (1.0 - t) + end * t
}

fn point_vec3(points: &[[f32; 3]], index: usize) -> Vec3 {
    let clamped = index.min(points.len().saturating_sub(1));
    vec3(
        points[clamped][0],
        points[clamped][1],
        points[clamped][2],
    )
}

fn flight_target_point(points: &[[f32; 3]], flight_position: f32, look_back: bool) -> Vec3 {
    let current_step = flight_step_from_position(flight_position, points.len());
    let mut candidate = current_step;

    loop {
        candidate = if look_back {
            candidate.saturating_sub(1)
        } else {
            (candidate + 1).min(points.len().saturating_sub(1))
        };

        let target = point_vec3(points, candidate);
        let current = interpolate_walk_position(points, flight_position);
        if (target - current).magnitude() > 0.001 || candidate == current_step {
            return target;
        }

        if look_back {
            if candidate == 0 {
                return current;
            }
        } else if candidate == points.len().saturating_sub(1) {
            return current;
        }
    }
}

/// Calculate averaged flight camera and target positions from actual walk points.
fn calculate_average_path_position(
    walks: &BTreeMap<String, WalkData>,
    selected_ids: &std::collections::HashSet<String>,
    flight_position: f32,
    look_back: bool,
) -> Option<(Vec3, Vec3)> {
    let selected_walks: Vec<_> = walks.iter()
        .filter(|(id, w)| selected_ids.contains(*id) && w.visible && !w.points.is_empty())
        .collect();

    if selected_walks.is_empty() {
        return None;
    }

    let mut current_sum = vec3(0.0, 0.0, 0.0);
    let mut count = 0.0;
    let mut target_sum = vec3(0.0, 0.0, 0.0);

    for (_, walk) in &selected_walks {
        current_sum += interpolate_walk_position(&walk.points, flight_position);
        target_sum += flight_target_point(&walk.points, flight_position, look_back);
        count += 1.0;
    }

    let current_pos = current_sum / count;
    let target_pos = target_sum / count;
    Some((current_pos, target_pos))
}

fn max_selected_flight_len(
    walks: &BTreeMap<String, WalkData>,
    selected_ids: &std::collections::HashSet<String>,
) -> usize {
    walks
        .iter()
        .filter(|(id, walk)| selected_ids.contains(*id) && walk.visible && !walk.points.is_empty())
        .map(|(_, walk)| walk.points.len())
        .max()
        .unwrap_or(0)
}

fn flight_step_from_position(flight_position: f32, total_points: usize) -> usize {
    if total_points <= 1 {
        0
    } else {
        flight_position
            .floor()
            .clamp(0.0, total_points.saturating_sub(1) as f32) as usize
    }
}

fn flight_duration_from_speed_hz(walk_len: usize, flight_speed: f32) -> anyhow::Result<f32> {
    if walk_len == 0 {
        anyhow::bail!("Zero tolerance: cannot sync audio with zero walk points");
    }
    if flight_speed <= 0.0 {
        anyhow::bail!("Zero tolerance: flight_speed must be > 0");
    }
    Ok(walk_len as f32 / flight_speed)
}

fn format_vec3_debug(v: Vec3) -> String {
    format!("[{:.2}, {:.2}, {:.2}]", v.x, v.y, v.z)
}

fn summarize_selected_walks(
    walks: &BTreeMap<String, WalkData>,
    selected_ids: &std::collections::HashSet<String>,
) -> String {
    let mut parts = Vec::new();

    for (id, walk) in walks.iter().filter(|(id, _)| selected_ids.contains(*id)) {
        let first = walk
            .points
            .first()
            .map(|p| format!("[{:.2}, {:.2}, {:.2}]", p[0], p[1], p[2]))
            .unwrap_or_else(|| "[]".to_string());
        let last = walk
            .points
            .last()
            .map(|p| format!("[{:.2}, {:.2}, {:.2}]", p[0], p[1], p[2]))
            .unwrap_or_else(|| "[]".to_string());
        parts.push(format!(
            "{}: visible={} points={} first={} last={}",
            id,
            walk.visible,
            walk.points.len(),
            first,
            last
        ));
    }

    if parts.is_empty() {
        "<none>".to_string()
    } else {
        parts.join(" | ")
    }
}

fn update_shared_gui_state(
    shared_state: &Arc<Mutex<GuiState>>,
    walks: &BTreeMap<String, WalkData>,
    selected_sources: &std::collections::HashSet<String>,
    flight_mode: bool,
    flight_playing: bool,
    flight_position: f32,
    flight_speed: f32,
    selected_base: u32,
    selected_mapping: &str,
    camera: &Camera,
    frame_count: u64,
    uptime_secs: f64,
) {
    let mut selected_ids: Vec<String> = selected_sources.iter().cloned().collect();
    selected_ids.sort();

    let mut loaded_walks: Vec<WalkInfo> = walks
        .iter()
        .map(|(id, walk)| WalkInfo {
            id: id.clone(),
            name: walk.name.clone(),
            num_points: walk.points.len(),
            color: walk.color,
        })
        .collect();
    loaded_walks.sort_by(|a, b| a.id.cmp(&b.id));

    let camera_position = camera.position();
    let camera_target = camera.target();

    let state = GuiState {
        selected_sources: selected_ids,
        loaded_walks,
        flight_mode,
        flight_playing,
        flight_position,
        flight_speed,
        selected_base,
        selected_mapping: selected_mapping.to_string(),
        camera_position: [camera_position.x, camera_position.y, camera_position.z],
        camera_target: [camera_target.x, camera_target.y, camera_target.z],
        frame_count,
        uptime_secs,
    };

    if let Ok(mut guard) = shared_state.lock() {
        *guard = state;
    }
}

/// Run the native 3D GUI viewer using three-d
pub fn run_viewer(config: Config, auto_config: AutomationConfig, data_dir: PathBuf) -> anyhow::Result<()> {
    // Create winit event loop and window manually for fullscreen toggle access
    use winit::event_loop::EventLoop;
    use winit::window::WindowBuilder;

    let event_loop = EventLoop::new();
    let winit_window = WindowBuilder::new()
        .with_title("Data Walker - 3D")
        .with_maximized(true)
        .build(&event_loop)?;
    winit_window.focus_window();

    // Stash a raw pointer so the render callback can toggle fullscreen.
    // SAFETY: the winit window lives for the entire render_loop; the pointer
    // is never used after the loop exits.
    let winit_ptr = &winit_window as *const winit::window::Window as usize;

    let window = Window::from_winit_window(
        winit_window,
        event_loop,
        SurfaceSettings::default(),
        true, // maximized
    )?;

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
    let mut color_scheme = ColorScheme::default();
    let mut prev_color_scheme = color_scheme;
    let mut max_points: usize = 5000;
    let mut prev_max_points: usize = max_points;
    let mut show_grid = true;
    let mut show_axes = true;
    let mut show_points = true;
    let mut show_lines = true;
    let mut point_scale: f32 = 0.5;
    let mut line_scale: f32 = 0.3;
    let mut axis_ticks: u32 = 10;
    let mut auto_rotate_x = false;
    let mut auto_rotate_y = false;
    let mut auto_rotate_z = false;
    let mut auto_rotate_speed_x: f32 = 0.5;
    let mut auto_rotate_speed_y: f32 = 0.5;
    let mut auto_rotate_speed_z: f32 = 0.5;
    let mut screenshot_requested = false;
    let mut screenshot_status: Option<(String, f64)> = None; // (message, expire_time)
    let (walk_load_tx, walk_load_rx) = std::sync::mpsc::channel::<WalkLoadResult>();
    let mut pending_walk_loads: BTreeMap<String, PendingWalkLoad> = BTreeMap::new();
    let mut next_walk_load_request_id: u64 = 1;
    let (audio_prep_tx, audio_prep_rx) = std::sync::mpsc::channel::<AudioPrepResult>();
    let mut pending_audio_preps: BTreeMap<String, PendingAudioPrep> = BTreeMap::new();
    let mut next_audio_prep_request_id: u64 = 1;

    // Data Flight mode
    let mut flight_mode = false;
    let mut flight_playing = false;          // Is flight animation playing
    let mut flight_speed: f32 = 10.0;        // Points per second (Hz)
    let mut flight_reverse = false;          // Go backwards
    let mut flight_look_back = false;        // Look behind instead of ahead
    let mut flight_position: f32 = 0.0;      // Point position along path
    let mut last_frame_time: f64 = 0.0;      // For delta time calculation
    let mut flight_loop = false;             // Enable looping
    let mut flight_loop_mode: u8 = 0;        // 0 = Repeat, 1 = Fwd/Rev (ping-pong)
    let mut show_loop_menu = false;          // Dropdown visibility
    let mut fullscreen_plot = false;          // ESC toggles panels off for pure plot
    let mut last_flight_debug_state = String::new();
    let mut flight_debug_window_elapsed_secs: f32 = 0.0;
    let mut flight_debug_frames_in_window: usize = 0;
    let mut flight_debug_crossed_points_in_window: usize = 0;
    let mut flight_debug_position_delta_in_window: f32 = 0.0;

    // Audio playback state
    let audio_engine_result = AudioEngine::new();
    match &audio_engine_result {
        Ok(_) => info!("Audio engine successfully initialized"),
        Err(e) => warn!("Audio engine failed to initialize: {}", e),
    }
    let mut audio_engine: Option<AudioEngine> = audio_engine_result.ok();
    let mut audio_settings = AudioSettings::default();
    let data_paths = DataPaths::new(data_dir);
    let mut prev_audio_enabled = audio_settings.enabled;
    let mut prev_force_synthesis = audio_settings.force_synthesis;
    let mut prev_synth_method = audio_settings.synthesis_method;
    let mut prev_flight_speed: f32 = flight_speed;
    let mut prev_sync_to_flight = audio_settings.sync_to_flight;

    // Track which slider has focus for arrow key control
    #[derive(Clone, Copy, PartialEq)]
    enum FocusedSlider { None, MaxPoints, PointScale, LineScale, FlightSpeed, FlightProgress }
    let mut focused_slider = FocusedSlider::None;

    // GUI state
    let mut gui = GUI::new(&context);
    let mut first_frame = true;  // Track first frame to reset egui state

    // Color pool for dynamic assignment
    let mut color_pool = ColorPool::new(color_scheme);

    // Automation state
    let cmd_queue = crate::automation::new_command_queue();
    let shared_state = crate::automation::new_shared_state();
    let auto_quit_time: Option<f64> = if auto_config.quit_after_secs > 0.0 {
        Some(auto_config.quit_after_secs)
    } else {
        None
    };
    let mut auto_screenshot_pending = auto_config.screenshot_and_quit.clone();
    let mut frame_count: u64 = 0;

    // Start IPC server if enabled
    if auto_config.ipc_port > 0 {
        if let Err(e) = crate::automation::start_ipc_server(
            auto_config.ipc_port,
            cmd_queue.clone(),
            shared_state.clone(),
        ) {
            error!("[AUTO] Failed to start IPC server: {}", e);
        }
    }

    // Log startup event if JSON events enabled
    if auto_config.json_events {
        crate::automation::log_event(&GuiEvent::Started { timestamp: 0.0 });
    }

    // Pre-build category list
    let mut by_category: BTreeMap<String, Vec<crate::config::Source>> = BTreeMap::new();
    for source in &config.sources {
        by_category.entry(source.category.clone()).or_default().push(source.clone());
    }

    // Track which auto-select sources haven't been processed yet
    let mut pending_auto_select: Vec<String> = auto_config.auto_select.clone();
    let auto_flight_pending = auto_config.auto_flight;
    let auto_play_pending = auto_config.auto_play;

    // Main loop
    window.render_loop(move |mut frame_input| {
        frame_count += 1;
        let current_time = frame_input.accumulated_time;

        // Process automation on first frame (auto-select, auto-flight, etc.)
        if !pending_auto_select.is_empty() {
            debug!("[AUTO] Processing {} pending auto-select sources", pending_auto_select.len());
            for source_id in pending_auto_select.drain(..) {
                if let Some(source) = config.sources.iter().find(|s| s.id == source_id) {
                    debug!("[AUTO] Auto-selecting source: {}", source_id);
                    selected_sources.insert(source_id.clone());
                    let color = color_pool.get_color(&source_id);
                    queue_walk_load(
                        &walk_load_tx,
                        &mut pending_walk_loads,
                        &mut next_walk_load_request_id,
                        source.clone(),
                        &config,
                        &data_paths,
                        max_points,
                        &selected_mapping,
                        selected_base,
                        color,
                        true,
                    );
                } else {
                    warn!("[AUTO] Source not found: {}", source_id);
                }
            }
            // Auto-enable flight mode if requested
            if auto_flight_pending && !flight_mode {
                debug!("[AUTO] Auto-enabling flight mode");
                flight_mode = true;
            }
            // Auto-start playback if requested
            if auto_play_pending && !flight_playing {
                debug!("[AUTO] Auto-starting playback");
                flight_playing = true;
            }
        }

        // Check auto-quit timer
        if let Some(quit_time) = auto_quit_time {
            if current_time >= quit_time {
                info!("[AUTO] Quit timer expired, exiting");
                return FrameOutput { exit: true, ..Default::default() };
            }
        }

        // Process IPC commands
        if let Ok(mut queue) = cmd_queue.lock() {
            for cmd in queue.drain(..) {
                debug!("[AUTO] Processing IPC command: {:?}", cmd);
                match cmd {
                    AutoCommand::SelectSource { id } => {
                        if let Some(source) = config.sources.iter().find(|s| s.id == id) {
                            selected_sources.insert(id.clone());
                            let color = color_pool.get_color(&id);
                            queue_walk_load(
                                &walk_load_tx,
                                &mut pending_walk_loads,
                                &mut next_walk_load_request_id,
                                source.clone(),
                                &config,
                                &data_paths,
                                max_points,
                                &selected_mapping,
                                selected_base,
                                color,
                                true,
                            );
                        }
                    }
                    AutoCommand::DeselectSource { id } => {
                        selected_sources.remove(&id);
                        walks.remove(&id);
                        pending_walk_loads.remove(&id);
                        pending_audio_preps.remove(&id);
                        color_pool.release_color(&id);
                    }
                    AutoCommand::DeselectAll => {
                        selected_sources.clear();
                        walks.clear();
                        pending_walk_loads.clear();
                        pending_audio_preps.clear();
                        color_pool.clear();
                    }
                    AutoCommand::SetFlightMode { enabled } => {
                        flight_mode = enabled;
                    }
                    AutoCommand::SetFlightPlaying { playing } => {
                        flight_playing = playing;
                    }
                    AutoCommand::SetFlightPosition { position } => {
                        let max_step = max_selected_flight_len(&walks, &selected_sources)
                            .saturating_sub(1) as f32;
                        flight_position = position.clamp(0.0, max_step);
                    }
                    AutoCommand::SetFlightSpeed { speed } => {
                        flight_speed = speed.clamp(0.01, 60.0);
                    }
                    AutoCommand::SetBase { base } => {
                        if base == 4 || base == 6 || base == 12 {
                            selected_base = base;
                        }
                    }
                    AutoCommand::SetMapping { name } => {
                        selected_mapping = name;
                    }
                    AutoCommand::Screenshot { path: _ } => {
                        screenshot_requested = true;
                    }
                    AutoCommand::GetState => {
                        // State will be logged below
                    }
                    AutoCommand::Quit => {
                        info!("[AUTO] Quit command received");
                        return FrameOutput { exit: true, ..Default::default() };
                    }
                }
            }
        }

        while let Ok(result) = walk_load_rx.try_recv() {
            let Some(pending) = pending_walk_loads.get(&result.source.id) else {
                continue;
            };
            if pending.request_id != result.request_id {
                continue;
            }
            pending_walk_loads.remove(&result.source.id);

            if !selected_sources.contains(&result.source.id) {
                continue;
            }

            match result.walk_data {
                Some(walk_data) => {
                    let walk_len = walk_data.points.len();
                    debug!(
                        "[GUI] Loaded {} walk points for {}",
                        walk_len,
                        result.source.id
                    );
                    if result.prepare_audio {
                        if let Some(engine) = audio_engine.as_mut() {
                            prepare_audio_for_source(
                                &result.source,
                                &data_paths,
                                engine,
                                &audio_settings,
                                selected_base,
                                flight_mode,
                                flight_playing,
                                flight_speed,
                                walk_len,
                                &audio_prep_tx,
                                &mut pending_audio_preps,
                                &mut next_audio_prep_request_id,
                            );
                        } else {
                            warn!("[GUI] No audio engine available when selecting source {}", result.source.id);
                        }
                    }
                    walks.insert(result.source.id.clone(), walk_data);
                    if flight_mode {
                        if !flight_playing {
                            flight_position = 0.0;
                        }
                        debug!(
                            "[GUI][FLIGHT] Walk loaded while flight enabled: position={:.3}, playing={}, selected={}, walks={}",
                            flight_position,
                            flight_playing,
                            selected_sources.len(),
                            summarize_selected_walks(&walks, &selected_sources)
                        );
                    }
                }
                None => {
                    warn!("[GUI] load_walk_data returned None for {}", result.source.id);
                }
            }
        }

        while let Ok(result) = audio_prep_rx.try_recv() {
            let Some(pending) = pending_audio_preps.get(&result.source.id) else {
                continue;
            };
            if pending.request_id != result.request_id {
                continue;
            }
            pending_audio_preps.remove(&result.source.id);

            if !selected_sources.contains(&result.source.id) || !audio_settings.enabled {
                continue;
            }

            match result.result {
                Ok(prepared) => {
                    if let Some(engine) = audio_engine.as_mut() {
                        match engine.prepare_pre_stretched_file_source(
                            &result.source.id,
                            prepared.path,
                            prepared.samples,
                            prepared.sample_rate,
                            prepared.channels,
                        ) {
                            Ok(_) => {
                                debug!("[GUI] Background audio prep complete for {}", result.source.id);
                                if flight_mode && flight_playing {
                                    engine.play(&audio_settings);
                                }
                            }
                            Err(e) => error!(
                                "[GUI] Failed to install stretched audio source {}: {}",
                                result.source.id,
                                e
                            ),
                        }
                    } else {
                        warn!("[GUI] No audio engine available when installing {}", result.source.id);
                    }
                }
                Err(e) => {
                    error!("[GUI] Background audio prep failed for {}: {}", result.source.id, e);
                }
            }
        }

        // Handle auto-screenshot-and-quit
        if auto_screenshot_pending.is_some() && frame_count > 10 {
            // Wait a few frames for scene to render
            screenshot_requested = true;
        }

        enforce_zero_tolerance_rule_hook(&mut audio_settings, flight_mode);

        let selected_flight_len = max_selected_flight_len(&walks, &selected_sources);
        let max_flight_step = selected_flight_len.saturating_sub(1) as f32;
        if selected_flight_len <= 1 {
            flight_position = 0.0;
        } else {
            flight_position = flight_position.clamp(0.0, max_flight_step);
        }

        update_shared_gui_state(
            &shared_state,
            &walks,
            &selected_sources,
            flight_mode,
            flight_playing,
            flight_position,
            flight_speed,
            selected_base,
            &selected_mapping,
            &camera,
            frame_count,
            current_time,
        );

        // Handle keyboard events (spacebar for play/pause)
        for event in &frame_input.events {
            if let three_d::Event::KeyPress { kind, modifiers, .. } = event {
                debug!("[GUI] KeyPress: {:?} (modifiers: shift={}, ctrl={}, alt={})",
                    kind, modifiers.shift, modifiers.ctrl, modifiers.alt);
                if *kind == three_d::Key::Space && flight_mode {
                    debug!("[GUI] Spacebar pressed: toggling flight_playing to {}", !flight_playing);
                    flight_playing = !flight_playing;
                    // Sync audio play/pause
                    if audio_settings.enabled {
                        if let Some(ref mut engine) = audio_engine {
                            if flight_playing {
                                debug!("[GUI] Starting audio via spacebar");
                                engine.play(&audio_settings);
                            } else {
                                debug!("[GUI] Pausing audio via spacebar");
                                engine.pause();
                            }
                        }
                    }
                }
                if *kind == three_d::Key::Escape {
                    debug!("[GUI] Escape pressed: toggling fullscreen to {}", !fullscreen_plot);
                    fullscreen_plot = !fullscreen_plot;
                    // Toggle borderless fullscreen via the winit window handle
                    // SAFETY: winit_ptr points to the window owned by render_loop's self,
                    // which is alive for the entire loop.
                    let ww = unsafe { &*(winit_ptr as *const winit::window::Window) };
                    if fullscreen_plot {
                        ww.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
                        ww.set_cursor_visible(false);
                    } else {
                        ww.set_fullscreen(None);
                        ww.set_cursor_visible(true);
                    }
                }
                // Arrow keys adjust focused slider; shift = fine control
                if *kind == three_d::Key::ArrowRight || *kind == three_d::Key::ArrowLeft {
                    let sign = if *kind == three_d::Key::ArrowRight { 1.0 } else { -1.0 };
                    let fine = if modifiers.shift { 0.1 } else { 1.0 };
                    match focused_slider {
                        FocusedSlider::MaxPoints => {
                            let step = if modifiers.shift { 10.0 } else { 100.0 };
                            max_points = (max_points as f32 + sign * step).clamp(100.0, 10000.0) as usize;
                        }
                        FocusedSlider::PointScale => {
                            let step = if modifiers.shift { 0.01 } else { 0.05 };
                            point_scale = (point_scale + sign * step).clamp(0.01, 1.0);
                        }
                        FocusedSlider::LineScale => {
                            let step = if modifiers.shift { 0.01 } else { 0.05 };
                            line_scale = (line_scale + sign * step).clamp(0.05, 2.0);
                        }
                        FocusedSlider::FlightSpeed => {
                            let step = if modifiers.shift { 0.1 } else { 1.0 } * fine;
                            flight_speed = (flight_speed + sign * step).clamp(0.01, 60.0);
                        }
                        FocusedSlider::FlightProgress => {
                            let step = if modifiers.shift { 0.1 } else { 1.0 };
                            let max_step = max_selected_flight_len(&walks, &selected_sources)
                                .saturating_sub(1) as f32;
                            flight_position = (flight_position + sign * step).clamp(0.0, max_step);
                        }
                        FocusedSlider::None => {}
                    }
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
        if (auto_rotate_x || auto_rotate_y || auto_rotate_z) && !flight_mode {
            let pos = camera.position();
            let target = camera.target();
            let mut dir = pos - target;
            let mut up = camera.up();

            // Rotation speeds in radians per frame (speed slider is 0-2, so multiply by 0.02)
            let speed_factor = 0.02;

            // Rotate around Y axis (horizontal rotation)
            if auto_rotate_y {
                let angle = auto_rotate_speed_y * speed_factor;
                let cos_a = angle.cos();
                let sin_a = angle.sin();
                let new_x = dir.x * cos_a - dir.z * sin_a;
                let new_z = dir.x * sin_a + dir.z * cos_a;
                dir.x = new_x;
                dir.z = new_z;
                let new_ux = up.x * cos_a - up.z * sin_a;
                let new_uz = up.x * sin_a + up.z * cos_a;
                up.x = new_ux;
                up.z = new_uz;
            }

            // Rotate around X axis (vertical tilt)
            if auto_rotate_x {
                let angle = auto_rotate_speed_x * speed_factor;
                let cos_a = angle.cos();
                let sin_a = angle.sin();
                let new_y = dir.y * cos_a - dir.z * sin_a;
                let new_z = dir.y * sin_a + dir.z * cos_a;
                dir.y = new_y;
                dir.z = new_z;
                let new_uy = up.y * cos_a - up.z * sin_a;
                let new_uz = up.y * sin_a + up.z * cos_a;
                up.y = new_uy;
                up.z = new_uz;
            }

            // Rotate around Z axis (roll)
            if auto_rotate_z {
                let angle = auto_rotate_speed_z * speed_factor;
                let cos_a = angle.cos();
                let sin_a = angle.sin();
                let new_x = dir.x * cos_a - dir.y * sin_a;
                let new_y = dir.x * sin_a + dir.y * cos_a;
                dir.x = new_x;
                dir.y = new_y;
                let new_ux = up.x * cos_a - up.y * sin_a;
                let new_uy = up.x * sin_a + up.y * cos_a;
                up.x = new_ux;
                up.y = new_uy;
            }

            camera.set_view(target + dir, target, up);
        }

        // Data Flight camera update
        if flight_mode {
            // Calculate delta time
            let delta = if last_frame_time > 0.0 {
                ((frame_input.accumulated_time - last_frame_time) as f32 / 1000.0).max(0.0)
            } else {
                0.0
            };
            last_frame_time = frame_input.accumulated_time;

            // Update progress only when playing
            if flight_playing {
                let max_len = max_selected_flight_len(&walks, &selected_sources);
                let max_step = max_len.saturating_sub(1) as f32;
                let previous_position = flight_position;
                let previous_step = flight_step_from_position(flight_position, max_len);
                let position_delta = flight_speed * delta;

                if flight_reverse {
                    flight_position -= position_delta;
                    if flight_position <= 0.0 {
                        if flight_loop {
                            if flight_loop_mode == 1 {
                                // Fwd/Rev: reverse direction
                                flight_reverse = false;
                                flight_position = 0.0;
                            } else {
                                // Repeat: jump to end
                                flight_position = max_step;
                            }
                        } else {
                            flight_position = 0.0;
                        }
                    }
                } else {
                    flight_position += position_delta;
                    if flight_position >= max_step {
                        if flight_loop {
                            if flight_loop_mode == 1 {
                                // Fwd/Rev: reverse direction
                                flight_reverse = true;
                                flight_position = max_step;
                            } else {
                                // Repeat: jump to start
                                flight_position = 0.0;
                            }
                        } else {
                            flight_position = max_step;
                        }
                    }
                }

                let flight_step = flight_step_from_position(flight_position, max_len);
                let crossed_points = flight_step.abs_diff(previous_step);
                flight_debug_frames_in_window += 1;
                flight_debug_crossed_points_in_window += crossed_points;
                flight_debug_position_delta_in_window += (flight_position - previous_position).abs();
                flight_debug_window_elapsed_secs += delta;

                // Sync audio to flight step
                if audio_settings.enabled {
                    if let Some(ref mut engine) = audio_engine {
                        engine.sync_to_step(flight_step, max_len, &audio_settings);
                    }
                }

                if flight_debug_window_elapsed_secs >= 1.0 {
                    let avg_frame_dt = if flight_debug_frames_in_window > 0 {
                        flight_debug_window_elapsed_secs / flight_debug_frames_in_window as f32
                    } else {
                        0.0
                    };
                    debug!(
                        "[GUI][FLIGHT_RATE] elapsed={:.2}s requested_hz={:.3} actual_points_per_sec={:.3} position_delta_per_sec={:.3} frames={} avg_dt={:.4}s prev_pos={:.3} pos={:.3} prev_step={} step={} crossed_points={} max_len={} reverse={}",
                        flight_debug_window_elapsed_secs,
                        flight_speed,
                        flight_debug_crossed_points_in_window as f32 / flight_debug_window_elapsed_secs,
                        flight_debug_position_delta_in_window / flight_debug_window_elapsed_secs,
                        flight_debug_frames_in_window,
                        avg_frame_dt,
                        previous_position,
                        flight_position,
                        previous_step,
                        flight_step,
                        flight_debug_crossed_points_in_window,
                        max_len,
                        flight_reverse,
                    );
                    flight_debug_window_elapsed_secs = 0.0;
                    flight_debug_frames_in_window = 0;
                    flight_debug_crossed_points_in_window = 0;
                    flight_debug_position_delta_in_window = 0.0;
                }
            }

            // Get average position along path
            if let Some((current_pos, target_pos)) = calculate_average_path_position(
                &walks, &selected_sources, flight_position, flight_look_back
            ) {
                // Direction of travel
                let dir_vec = target_pos - current_pos;
                let dir_mag = dir_vec.magnitude();

                if dir_mag > 0.001 {
                    if last_flight_debug_state != "tracking" {
                        debug!(
                            "[GUI][FLIGHT] tracking position={:.3} current={} target={} camera_pos={} camera_target={} walks={}",
                            flight_position,
                            format_vec3_debug(current_pos),
                            format_vec3_debug(target_pos),
                            format_vec3_debug(camera.position()),
                            format_vec3_debug(camera.target()),
                            summarize_selected_walks(&walks, &selected_sources)
                        );
                        last_flight_debug_state = "tracking".to_string();
                    }
                    let direction = dir_vec / dir_mag;

                    let world_up = vec3(0.0, 1.0, 0.0);
                    let reference_up = if direction.dot(world_up).abs() > 0.95 {
                        vec3(0.0, 0.0, 1.0)
                    } else {
                        world_up
                    };
                    let right_vec = direction.cross(reference_up);
                    let right_mag = right_vec.magnitude();
                    let cam_up = if right_mag > 0.001 {
                        (right_vec / right_mag).cross(direction).normalize()
                    } else {
                        reference_up
                    };

                    camera.set_view(current_pos, target_pos, cam_up);
                } else {
                    if last_flight_debug_state != "degenerate_dir" {
                        debug!(
                            "[GUI][FLIGHT] degenerate_dir position={:.3} current={} target={} camera_pos={} camera_target={} walks={}",
                            flight_position,
                            format_vec3_debug(current_pos),
                            format_vec3_debug(target_pos),
                            format_vec3_debug(camera.position()),
                            format_vec3_debug(camera.target()),
                            summarize_selected_walks(&walks, &selected_sources)
                        );
                        last_flight_debug_state = "degenerate_dir".to_string();
                    }
                    camera.set_view(
                        current_pos + vec3(0.0, 50.0, 200.0),
                        current_pos,
                        vec3(0.0, 1.0, 0.0),
                    );
                }
            } else if last_flight_debug_state != "no_path" {
                debug!(
                    "[GUI][FLIGHT] no_path position={:.3} selected={} camera_pos={} camera_target={} walks={}",
                    flight_position,
                    selected_sources.len(),
                    format_vec3_debug(camera.position()),
                    format_vec3_debug(camera.target()),
                    summarize_selected_walks(&walks, &selected_sources)
                );
                last_flight_debug_state = "no_path".to_string();
            }
        } else {
            last_flight_debug_state.clear();
        }

        // Track if egui wants pointer input (set after GUI update)
        let mut egui_wants_pointer = false;

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
                let has_per_point = walk.point_colors.is_some();

                for i in 0..walk.points.len() - 1 {
                    if let Some(ref breaks) = walk.chain_breaks {
                        if breaks.contains(&(i + 1)) {
                            continue;
                        }
                    }

                    let p1 = vec3(walk.points[i][0], walk.points[i][1], walk.points[i][2]);
                    let p2 = vec3(walk.points[i + 1][0], walk.points[i + 1][1], walk.points[i + 1][2]);

                    let dir = p2 - p1;
                    let length = dir.magnitude();

                    if length > 0.001 {
                        let radius = if has_per_point {
                            // PDB structure mode: uniform tube radius
                            line_scale * 0.4
                        } else {
                            // Walk mode: scale radius by visit count (log scale)
                            let key1 = point_key(walk.points[i]);
                            let key2 = point_key(walk.points[i + 1]);
                            let count1 = *walk.revisit_counts.get(&key1).unwrap_or(&1) as f32;
                            let count2 = *walk.revisit_counts.get(&key2).unwrap_or(&1) as f32;
                            let avg_count = (count1 + count2) * 0.5;
                            line_scale * (0.3 + 0.7 * avg_count.ln().max(0.0) / ln_max)
                        };

                        // three-d meshes extend along X from 0 to 1, radius 1 in Y/Z
                        let x_axis = vec3(1.0, 0.0, 0.0);
                        let dir_n = dir.normalize();
                        let rotation = if dir_n.dot(x_axis).abs() > 0.999 {
                            if dir_n.dot(x_axis) < 0.0 {
                                Mat4::from_angle_y(radians(std::f32::consts::PI))
                            } else {
                                Mat4::identity()
                            }
                        } else {
                            let axis = x_axis.cross(dir_n).normalize();
                            let angle = x_axis.dot(dir_n).acos();
                            Mat4::from_axis_angle(axis, radians(angle))
                        };

                        let transform = Mat4::from_translation(p1)
                            * rotation
                            * Mat4::from_nonuniform_scale(length, radius, radius);

                        instances.transformations.push(transform);

                        // Use per-point color if available, otherwise walk color
                        let seg_color = if let Some(ref pc) = walk.point_colors {
                            let c = pc.get(i).unwrap_or(&walk.color);
                            Srgba::new(
                                (c[0] * 255.0) as u8,
                                (c[1] * 255.0) as u8,
                                (c[2] * 255.0) as u8,
                                255,
                            )
                        } else {
                            color
                        };
                        if let Some(ref mut colors) = instances.colors {
                            colors.push(seg_color);
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

            // Points (spheres scaled by revisit count, or per-atom for PDB structures)
            if show_points {
                let mut instances = Instances::default();
                instances.transformations = Vec::new();
                instances.colors = Some(Vec::new());

                if walk.point_colors.is_some() {
                    // PDB structure mode: render a sphere at each Cα atom position
                    let atom_size = 0.6 * point_scale;
                    for (idx, p) in walk.points.iter().enumerate() {
                        let transform = Mat4::from_translation(vec3(p[0], p[1], p[2]))
                            * Mat4::from_scale(atom_size);
                        instances.transformations.push(transform);

                        let c = walk.point_colors.as_ref().unwrap()
                            .get(idx).unwrap_or(&walk.color);
                        if let Some(ref mut colors) = instances.colors {
                            colors.push(Srgba::new(
                                (c[0] * 255.0) as u8,
                                (c[1] * 255.0) as u8,
                                (c[2] * 255.0) as u8,
                                255,
                            ));
                        }
                    }
                } else {
                    // Walk mode: render spheres at the actual walk positions.
                    let max_revisits = walk.revisit_counts.values().max().copied().unwrap_or(1) as f32;

                    for (key, &count) in &walk.revisit_counts {
                        let position = walk.point_positions.get(key).copied().unwrap_or([
                            key.0 as f32 / POSITION_KEY_SCALE,
                            key.1 as f32 / POSITION_KEY_SCALE,
                            key.2 as f32 / POSITION_KEY_SCALE,
                        ]);
                        let base_size = 0.8 * point_scale;
                        let scale_factor = 1.0 + (count as f32).ln().max(0.0) / max_revisits.ln().max(1.0) * 2.0;
                        let size = base_size * scale_factor;

                        let transform = Mat4::from_translation(vec3(position[0], position[1], position[2]))
                            * Mat4::from_scale(size);
                        instances.transformations.push(transform);

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

        // Derive axis/grid extent from tick spacing (show ~10 ticks worth)
        let axis_extent = if axis_ticks == 0 { 100.0 } else { (axis_ticks as f32) * 10.0 };

        // Grid using thin cylinders
        let grid_objects: Vec<Gm<InstancedMesh, ColorMaterial>> = if show_grid {
            let grid_size = axis_extent;
            let grid_step = if axis_ticks == 0 { 20.0 } else { axis_ticks as f32 };
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
                // Reset egui memory on first frame to clear any corrupted state
                if first_frame {
                    // Clear ALL egui persistent state to prevent panel issues
                    egui_ctx.memory_mut(|mem| *mem = Default::default());
                    first_frame = false;
                }

                if !fullscreen_plot {
                egui::SidePanel::left("walks_panel")
                    .min_width(300.0)
                    .max_width(300.0)
                    .resizable(false)
                    .show(egui_ctx, |ui| {
                    ui.heading("Data Walks");
                    ui.separator();

                    // Base and mapping selectors
                    ui.horizontal(|ui| {
                        ui.label("Base:");
                        let prev_base_val = selected_base;
                        egui::ComboBox::from_id_salt("base")
                            .width(40.0)
                            .selected_text(format!("{}", selected_base))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut selected_base, 12, "12");
                                ui.selectable_value(&mut selected_base, 6, "6");
                                ui.selectable_value(&mut selected_base, 4, "4");
                            });
                        if selected_base != prev_base_val {
                            debug!("[GUI] Base changed: {} -> {}", prev_base_val, selected_base);
                        }
                        ui.label("Mapping:");
                        let mapping_enabled = selected_base == 12 || selected_base == 6;
                        ui.add_enabled_ui(mapping_enabled, |ui| {
                            let prev_mapping_val = selected_mapping.clone();
                            egui::ComboBox::from_id_salt("mapping")
                                .selected_text(&selected_mapping)
                                .show_ui(ui, |ui| {
                                    let mapping_keys: Vec<String> = if selected_base == 6 {
                                        config.mappings_base6.keys().cloned().collect()
                                    } else {
                                        config.mappings.keys().cloned().collect()
                                    };
                                    for name in &mapping_keys {
                                        ui.selectable_value(&mut selected_mapping, name.clone(), name);
                                    }
                                });
                            if selected_mapping != prev_mapping_val {
                                debug!("[GUI] Mapping changed: {} -> {}", prev_mapping_val, selected_mapping);
                            }
                        });
                    });

                    ui.horizontal(|ui| {
                        ui.label("Colors:");
                        let prev_color_scheme_val = color_scheme;
                        egui::ComboBox::from_id_salt("color_scheme")
                            .selected_text(color_scheme.label())
                            .show_ui(ui, |ui| {
                                for scheme in [
                                    ColorScheme::Distinct,
                                    ColorScheme::Wong,
                                    ColorScheme::Tol,
                                    ColorScheme::Pastel,
                                    ColorScheme::Neon,
                                    ColorScheme::Earth,
                                ] {
                                    ui.selectable_value(&mut color_scheme, scheme, scheme.label());
                                }
                            });
                        if color_scheme != prev_color_scheme_val {
                            debug!(
                                "[GUI] Color scheme changed: {} -> {}",
                                prev_color_scheme_val.label(),
                                color_scheme.label()
                            );
                        }
                    });

                    if ui.add(egui::Slider::new(&mut max_points, 100..=10000).text("Max points")).clicked() {
                        focused_slider = FocusedSlider::MaxPoints;
                    }

                    ui.horizontal(|ui| {
                        if ui.button("Deselect All").clicked() {
                            debug!("[GUI] Button clicked: Deselect All");
                            selected_sources.clear();
                            walks.clear();
                            color_pool.clear();
                            // Stop and clear all audio sources
                            if let Some(ref mut engine) = audio_engine {
                                debug!("[GUI] Stopping all audio sources");
                                engine.stop_all();
                            }
                        }
                        if ui.button("Center View").clicked() {
                            debug!("[GUI] Button clicked: Center View");
                            camera.set_view(
                                vec3(0.0, 50.0, 200.0),
                                vec3(0.0, 0.0, 0.0),
                                vec3(0.0, 1.0, 0.0),
                            );
                        }
                    });

                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut show_grid, "Grid").changed() {
                            debug!("[GUI] Checkbox changed: show_grid = {}", show_grid);
                        }
                        if ui.checkbox(&mut show_axes, "Axes").changed() {
                            debug!("[GUI] Checkbox changed: show_axes = {}", show_axes);
                        }
                    });

                    // Auto-rotate controls
                    ui.horizontal(|ui| {
                        ui.label("Auto-rotate:");
                        if ui.checkbox(&mut auto_rotate_x, "X").changed() {
                            debug!("[GUI] Checkbox changed: auto_rotate_x = {}", auto_rotate_x);
                        }
                        if ui.checkbox(&mut auto_rotate_y, "Y").changed() {
                            debug!("[GUI] Checkbox changed: auto_rotate_y = {}", auto_rotate_y);
                        }
                        if ui.checkbox(&mut auto_rotate_z, "Z").changed() {
                            debug!("[GUI] Checkbox changed: auto_rotate_z = {}", auto_rotate_z);
                        }
                    });
                    if auto_rotate_x || auto_rotate_y || auto_rotate_z {
                        if auto_rotate_x {
                            ui.horizontal(|ui| {
                                ui.label("  X:");
                                ui.add(egui::Slider::new(&mut auto_rotate_speed_x, -2.0..=2.0).show_value(false));
                            });
                        }
                        if auto_rotate_y {
                            ui.horizontal(|ui| {
                                ui.label("  Y:");
                                ui.add(egui::Slider::new(&mut auto_rotate_speed_y, -2.0..=2.0).show_value(false));
                            });
                        }
                        if auto_rotate_z {
                            ui.horizontal(|ui| {
                                ui.label("  Z:");
                                ui.add(egui::Slider::new(&mut auto_rotate_speed_z, -2.0..=2.0).show_value(false));
                            });
                        }
                    }
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut show_points, "Points").changed() {
                            debug!("[GUI] Checkbox changed: show_points = {}", show_points);
                        }
                        if ui.checkbox(&mut show_lines, "Lines").changed() {
                            debug!("[GUI] Checkbox changed: show_lines = {}", show_lines);
                        }
                    });
                    if show_points {
                        if ui.add(egui::Slider::new(&mut point_scale, 0.01..=1.0).text("Point scale")).clicked() {
                            focused_slider = FocusedSlider::PointScale;
                        }
                    }
                    if show_lines {
                        if ui.add(egui::Slider::new(&mut line_scale, 0.05..=2.0).text("Line scale")).clicked() {
                            focused_slider = FocusedSlider::LineScale;
                        }
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
                                ui.selectable_value(&mut axis_ticks, 100, "100");
                                ui.selectable_value(&mut axis_ticks, 1000, "1000");
                                ui.selectable_value(&mut axis_ticks, 10000, "10000");
                            });
                    });

                    ui.horizontal(|ui| {
                        if ui.button("SpaceMouse Config").clicked() {
                            debug!("[GUI] Button clicked: SpaceMouse Config (toggling to {})", !show_spacemouse_config);
                            show_spacemouse_config = !show_spacemouse_config;
                        }
                        if ui.button("Screenshot").clicked() {
                            debug!("[GUI] Button clicked: Screenshot");
                            screenshot_requested = true;
                        }
                    });

                    // Data Flight controls
                    ui.separator();
                    ui.heading("Data Flight");
                    if ui.checkbox(&mut flight_mode, "Enable Flight").changed() {
                        debug!("[GUI] Checkbox changed: flight_mode = {}", flight_mode);
                        // When disabling flight mode, stop playback and audio
                        if !flight_mode {
                            if flight_playing {
                                debug!("[GUI] Stopping playback because flight_mode disabled");
                                flight_playing = false;
                                if let Some(ref mut engine) = audio_engine {
                                    engine.stop_all();
                                }
                            }
                        }
                    }

                    if flight_mode {
                        // Play/Pause button with spacebar hint
                        ui.horizontal(|ui| {
                            let button_text = if flight_playing { "⏸ Pause" } else { "▶ Play" };
                            if ui.button(button_text).clicked() {
                                debug!("[GUI] Button clicked: {} (flight_playing -> {})", button_text, !flight_playing);
                                flight_playing = !flight_playing;
                                // Sync audio play/pause
                                if audio_settings.enabled {
                                    if let Some(ref mut engine) = audio_engine {
                                        if flight_playing {
                                            debug!("[GUI] Starting audio playback");
                                            engine.play(&audio_settings);
                                        } else {
                                            debug!("[GUI] Pausing audio playback");
                                            engine.pause();
                                        }
                                    }
                                }
                            }
                            ui.label("(Space)");
                        });

                        ui.horizontal(|ui| {
                            ui.label("Speed:");
                            if ui.add(egui::Slider::new(&mut flight_speed, 0.01..=60.0).logarithmic(true).suffix(" Hz")).clicked() {
                                focused_slider = FocusedSlider::FlightSpeed;
                            }
                        });

                        ui.horizontal(|ui| {
                            let max_step = max_selected_flight_len(&walks, &selected_sources)
                                .saturating_sub(1) as f32;
                            ui.label("Position:");
                            if ui.add(egui::Slider::new(&mut flight_position, 0.0..=max_step).suffix(" pt")).clicked() {
                                focused_slider = FocusedSlider::FlightProgress;
                            }
                        });

                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut flight_reverse, "Reverse").changed() {
                                debug!("[GUI] Checkbox changed: flight_reverse = {}", flight_reverse);
                            }
                            if ui.checkbox(&mut flight_look_back, "Look Back").changed() {
                                debug!("[GUI] Checkbox changed: flight_look_back = {}", flight_look_back);
                            }
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
                            debug!("[GUI] Button clicked: Reset to Start");
                            let max_step = max_selected_flight_len(&walks, &selected_sources)
                                .saturating_sub(1) as f32;
                            flight_position = if flight_reverse { max_step } else { 0.0 };
                            flight_playing = false;
                            // Stop audio on reset
                            if let Some(ref mut engine) = audio_engine {
                                debug!("[GUI] Stopping all audio on reset");
                                engine.stop_all();
                            }
                        }

                        // Audio controls
                        ui.separator();
                        if ui.checkbox(&mut audio_settings.enabled, "Audio").changed() {
                            debug!("[GUI] Checkbox changed: audio_settings.enabled = {}", audio_settings.enabled);
                        }

                        if audio_settings.enabled {
                            ui.horizontal(|ui| {
                                ui.label("Volume:");
                                ui.add(egui::Slider::new(&mut audio_settings.master_volume, 0.0..=1.0).show_value(false));
                            });

                            ui.horizontal(|ui| {
                                ui.label("Synth:");
                                egui::ComboBox::from_id_salt("synth_method")
                                    .width(80.0)
                                    .selected_text(match audio_settings.synthesis_method {
                                        SynthMethod::ChromaticNotes => "Chromatic",
                                        SynthMethod::SineTones => "Sine",
                                        SynthMethod::Percussion => "Drums",
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut audio_settings.synthesis_method,
                                            SynthMethod::ChromaticNotes, "Chromatic");
                                        ui.selectable_value(&mut audio_settings.synthesis_method,
                                            SynthMethod::SineTones, "Sine");
                                        ui.selectable_value(&mut audio_settings.synthesis_method,
                                            SynthMethod::Percussion, "Drums");
                                    });
                            });

                            ui.horizontal(|ui| {
                                ui.label("Mix:");
                                egui::ComboBox::from_id_salt("mix_mode")
                                    .width(80.0)
                                    .selected_text(match audio_settings.mixing_mode {
                                        MixingMode::Simultaneous => "All",
                                        MixingMode::CameraFocus => "Focus",
                                        MixingMode::DistanceBased => "Distance",
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut audio_settings.mixing_mode,
                                            MixingMode::Simultaneous, "All Equal");
                                        ui.selectable_value(&mut audio_settings.mixing_mode,
                                            MixingMode::CameraFocus, "Camera Focus");
                                        ui.selectable_value(&mut audio_settings.mixing_mode,
                                            MixingMode::DistanceBased, "Distance");
                                    });
                            });

                            // Generated audio checkbox: synthesize notes/drums instead of source audio when possible
                            if ui.checkbox(&mut audio_settings.force_synthesis, "Use Generated Audio")
                                .on_hover_text("Use generated notes/drums instead of the source audio file when possible")
                                .changed() {
                                debug!("[GUI] Checkbox changed: audio_settings.force_synthesis = {}", audio_settings.force_synthesis);
                            }

                            // Sync checkbox - speed is the enforced SSOT while flight mode is active.
                            if flight_mode {
                                let mut enforced_sync = true;
                                ui.add_enabled(
                                    false,
                                    egui::Checkbox::new(&mut enforced_sync, "Sync to Flight"),
                                )
                                .on_hover_text("Required while flight mode is active because Speed is SSOT for flight and audio");
                            } else if ui.checkbox(&mut audio_settings.sync_to_flight, "Sync to Flight")
                                .on_hover_text(if audio_settings.force_synthesis {
                                    "Sync generated note/drum playback to flight speed (one sound per walk point)"
                                } else {
                                    "Time-stretch source audio to match flight duration"
                                })
                                .changed() {
                                debug!("[GUI] Checkbox changed: audio_settings.sync_to_flight = {}", audio_settings.sync_to_flight);
                            }
                        }
                    }

                    ui.separator();

                    // Source list - calculate max height based on screen
                    let screen_height = ui.ctx().screen_rect().height();
                    let max_scroll_height = (screen_height - 400.0).max(200.0);

                    egui::ScrollArea::vertical()
                        .max_height(max_scroll_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                        for (category, sources) in &by_category {
                            let cat_name = config.categories.get(category).unwrap_or(category);
                            let _num_sources = sources.len();
                            // IMPORTANT: default_open(false) prevents egui from restoring
                            // expanded state that can cause layout issues on startup
                            let header = egui::CollapsingHeader::new(cat_name)
                                .id_salt(category)
                                .default_open(false);
                            let _response = header.show(ui, |ui| {
                                for source in sources {
                                    let mut checked = selected_sources.contains(&source.id);

                                    // Check if data is available (raw files for downloaded data, or math sources)
                                    let is_available = check_data_exists(&source.id, &source.converter, &source.url, &data_paths);

                                    if is_available {
                                        // Show label in the walk's plot color if selected
                                        let label = if let Some(walk) = walks.get(&source.id) {
                                            let c = walk.color;
                                            egui::RichText::new(&source.name).color(egui::Color32::from_rgb(
                                                (c[0] * 255.0) as u8,
                                                (c[1] * 255.0) as u8,
                                                (c[2] * 255.0) as u8,
                                            ))
                                        } else {
                                            egui::RichText::new(&source.name)
                                        };
                                        if ui.checkbox(&mut checked, label).changed() {
                                            debug!("[GUI] Source checkbox changed: {} -> {}", source.id, checked);
                                            if flight_mode {
                                                last_flight_debug_state.clear();
                                                if !flight_playing {
                                                    flight_position = 0.0;
                                                }
                                                debug!(
                                                    "[GUI][FLIGHT] Selection changed while flight enabled: source={} checked={} playing={} position={:.3} current_selection={:?}",
                                                    source.id,
                                                    checked,
                                                    flight_playing,
                                                    flight_position,
                                                    selected_sources
                                                );
                                            }
                                            if checked {
                                                debug!("[GUI] Selecting source: {}", source.id);
                                                selected_sources.insert(source.id.clone());
                                                // Get color from pool for maximum contrast
                                                let color = color_pool.get_color(&source.id);
                                                debug!("[GUI] Got color for {}: {:?}", source.id, color);
                                                debug!("[GUI] Queueing walk data load for {}", source.id);
                                                queue_walk_load(
                                                    &walk_load_tx,
                                                    &mut pending_walk_loads,
                                                    &mut next_walk_load_request_id,
                                                    source.clone(),
                                                    &config,
                                                    &data_paths,
                                                    max_points,
                                                    &selected_mapping,
                                                    selected_base,
                                                    color,
                                                    true,
                                                );
                                            } else {
                                                debug!("[GUI] Deselecting source: {}", source.id);
                                                selected_sources.remove(&source.id);
                                                walks.remove(&source.id);
                                                pending_walk_loads.remove(&source.id);
                                                pending_audio_preps.remove(&source.id);
                                                // Release color back to pool
                                                color_pool.release_color(&source.id);
                                                // Remove audio source
                                                if let Some(ref mut engine) = audio_engine {
                                                    debug!("[GUI] Removing audio source: {}", source.id);
                                                    engine.remove_source(&source.id);
                                                }
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

                if audio_settings.enabled != prev_audio_enabled {
                    debug!(
                        "[GUI] Audio enabled changed: {} -> {}",
                        prev_audio_enabled,
                        audio_settings.enabled
                    );
                    if audio_settings.enabled {
                        refresh_selected_audio_sources(
                            &selected_sources,
                            &walks,
                            &config,
                            &data_paths,
                            audio_engine.as_mut(),
                            &audio_settings,
                            selected_base,
                            flight_mode,
                            flight_playing,
                            flight_speed,
                            &audio_prep_tx,
                            &mut pending_audio_preps,
                            &mut next_audio_prep_request_id,
                        );
                    } else if let Some(ref mut engine) = audio_engine {
                        pending_audio_preps.clear();
                        engine.stop_all();
                    }
                    prev_audio_enabled = audio_settings.enabled;
                }

                if audio_settings.force_synthesis != prev_force_synthesis {
                    debug!(
                        "[GUI] Force synthesis changed: {} -> {}",
                        prev_force_synthesis,
                        audio_settings.force_synthesis
                    );
                    refresh_selected_audio_sources(
                        &selected_sources,
                        &walks,
                        &config,
                        &data_paths,
                        audio_engine.as_mut(),
                        &audio_settings,
                        selected_base,
                        flight_mode,
                        flight_playing,
                        flight_speed,
                        &audio_prep_tx,
                        &mut pending_audio_preps,
                        &mut next_audio_prep_request_id,
                    );
                    prev_force_synthesis = audio_settings.force_synthesis;
                }

                // Bottom panel
                egui::TopBottomPanel::bottom("status").show(egui_ctx, |ui| {
                    ui.horizontal(|ui| {
                        if !pending_walk_loads.is_empty() {
                            ui.add(egui::Spinner::new());
                            ui.label(format!(
                                "Loading {}",
                                format_pending_walks(&pending_walk_loads)
                            ));
                            ui.separator();
                        }
                        if !pending_audio_preps.is_empty() {
                            ui.add(egui::Spinner::new());
                            ui.label(format!(
                                "Preparing audio {}",
                                format_pending_audio(&pending_audio_preps)
                            ));
                            ui.separator();
                        }
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
                } // end if !fullscreen_plot

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
                    let label_offset = axis_extent * 1.05;
                    let axis_label_pos: [(&str, Vec3, egui::Color32); 3] = [
                        ("X", vec3(label_offset, 0.0, 0.0), egui::Color32::from_rgb(220, 50, 50)),
                        ("Y", vec3(0.0, label_offset, 0.0), egui::Color32::from_rgb(50, 220, 50)),
                        ("Z", vec3(0.0, 0.0, label_offset), egui::Color32::from_rgb(50, 100, 220)),
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
                        let axis_len = axis_extent;

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
                                debug!("[GUI] Button clicked: SpaceMouse Save");
                                spacemouse_config.save();
                            }
                            if ui.button("Reset").clicked() {
                                debug!("[GUI] Button clicked: SpaceMouse Reset");
                                spacemouse_config = SpaceMouseConfig::default();
                            }
                        });
                    });

                // Check if egui wants pointer input (for dropdowns, scroll areas, etc.)
                egui_wants_pointer = egui_ctx.wants_pointer_input();
            },
        );

        // Handle orbit control AFTER GUI update - only when egui doesn't want pointer and not in flight mode
        if !egui_wants_pointer && !flight_mode {
            orbit_control.handle_events(&mut camera, &mut frame_input.events);
        }

        // Reset mapping to first valid one when base changes
        if selected_base != prev_base {
            if selected_base == 6 {
                if !config.mappings_base6.contains_key(&selected_mapping) {
                    selected_mapping = config.mappings_base6.keys().next().cloned().unwrap_or("Split".to_string());
                }
            } else if selected_base == 12 {
                if !config.mappings.contains_key(&selected_mapping) {
                    selected_mapping = "Identity".to_string();
                }
            }
        }

        if color_scheme != prev_color_scheme {
            prev_color_scheme = color_scheme;
            color_pool.set_scheme(color_scheme);
            apply_color_scheme_to_walks(&mut walks, &mut color_pool);
        }

        // Regenerate walks if mapping, base, or max_points changed
        if selected_mapping != prev_mapping || max_points != prev_max_points || selected_base != prev_base {
            debug!("[GUI] Regenerating walks: mapping={} (was {}), base={} (was {}), max_points={} (was {})",
                selected_mapping, prev_mapping, selected_base, prev_base, max_points, prev_max_points);
            prev_mapping = selected_mapping.clone();
            prev_max_points = max_points;
            prev_base = selected_base;
            let source_ids: Vec<String> = selected_sources.iter().cloned().collect();
            debug!("[GUI] Regenerating {} walks", source_ids.len());
            for sid in &source_ids {
                if let Some(source) = config.sources.iter().find(|s| &s.id == sid) {
                    // Reuse existing color assignment
                    let color = color_pool.get_color(&sid);
                    debug!("[GUI] Regenerating walk for {}", sid);
                    queue_walk_load(
                        &walk_load_tx,
                        &mut pending_walk_loads,
                        &mut next_walk_load_request_id,
                        source.clone(),
                        &config,
                        &data_paths,
                        max_points,
                        &selected_mapping,
                        selected_base,
                        color,
                        true,
                    );
                }
            }
        }

        // Re-prepare audio sources if synth method changed so synced percussion
        // always goes through the point-triggered path instead of stale sinks.
        if audio_settings.synthesis_method != prev_synth_method {
            debug!("[GUI] Synth method changed: {:?} -> {:?}", prev_synth_method, audio_settings.synthesis_method);
            prev_synth_method = audio_settings.synthesis_method;
            if audio_settings.enabled && !selected_sources.is_empty() {
                debug!("[GUI] Refreshing audio sources for new synth method");
                refresh_selected_audio_sources(
                    &selected_sources,
                    &walks,
                    &config,
                    &data_paths,
                    audio_engine.as_mut(),
                    &audio_settings,
                    selected_base,
                    flight_mode,
                    flight_playing,
                    flight_speed,
                    &audio_prep_tx,
                    &mut pending_audio_preps,
                    &mut next_audio_prep_request_id,
                );
            }
        }

        let speed_ssot_sync = flight_mode;

        // Re-prepare audio sources if flight_speed or sync state changed
        let speed_changed = (flight_speed - prev_flight_speed).abs() > 0.001;
        let sync_changed = audio_settings.sync_to_flight != prev_sync_to_flight;
        if ((speed_changed && speed_ssot_sync) || sync_changed) && !selected_sources.is_empty() {
            debug!("[GUI] Flight speed or sync changed: speed {:.2} -> {:.2}, sync {} -> {}",
                prev_flight_speed, flight_speed, prev_sync_to_flight, audio_settings.sync_to_flight);
            prev_flight_speed = flight_speed;
            prev_sync_to_flight = audio_settings.sync_to_flight;
            refresh_selected_audio_sources(
                &selected_sources,
                &walks,
                &config,
                &data_paths,
                audio_engine.as_mut(),
                &audio_settings,
                selected_base,
                flight_mode,
                flight_playing,
                flight_speed,
                &audio_prep_tx,
                &mut pending_audio_preps,
                &mut next_audio_prep_request_id,
            );
        } else {
            prev_flight_speed = flight_speed;
            prev_sync_to_flight = audio_settings.sync_to_flight;
        }

        // Clear and render
        frame_input.screen().clear(ClearState::color_and_depth(0.1, 0.1, 0.15, 1.0, 1.0));

        // Render grid
        for grid_obj in &grid_objects {
            grid_obj.render(&camera, &[]);
        }

        // Render axes
        if show_axes {
            let axis_len = axis_extent;
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
            let axis_len = axis_extent;
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
        let mut should_quit_after_screenshot = false;
        if screenshot_requested {
            screenshot_requested = false;
            let vp = frame_input.viewport;
            let pixels: Vec<[u8; 4]> = frame_input.screen().read_color();
            let flat: Vec<u8> = pixels.iter().flat_map(|p| p.iter().copied()).collect();
            if let Some(img) = image::RgbaImage::from_raw(vp.width, vp.height, flat) {
                // Use custom path if provided via automation, otherwise default
                let filename = if let Some(ref path) = auto_screenshot_pending {
                    should_quit_after_screenshot = true;
                    std::path::PathBuf::from(path)
                } else {
                    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
                    let screenshots_dir = std::path::PathBuf::from("screenshots");
                    let _ = std::fs::create_dir_all(&screenshots_dir);
                    screenshots_dir.join(format!("data_walker_{}.png", timestamp))
                };

                // Ensure parent directory exists
                if let Some(parent) = filename.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }

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
                        should_quit_after_screenshot = false; // Don't quit on failure
                    }
                }
            }
        }

        // Handle auto-quit after screenshot
        if should_quit_after_screenshot {
            auto_screenshot_pending = None;
            info!("[AUTO] Screenshot complete, exiting");
            return FrameOutput { exit: true, ..Default::default() };
        }

        FrameOutput::default()
    });

    Ok(())
}

fn check_data_exists(id: &str, converter: &str, url: &str, data_paths: &DataPaths) -> bool {
    match converter {
        "audio" => data_paths.audio_file(id).is_some(),
        "dna" => data_paths.dna_file(url, id).exists(),
        "cosmos" => data_paths.cosmos_file(id).exists(),
        "finance" => data_paths.finance_file(url, id).exists(),
        c if c.starts_with("math.") => true, // Math is computed, always available
        "pdb_backbone" | "pdb_sequence" | "pdb_structure" => data_paths.protein_file(url, id).exists(),
        _ => false,
    }
}

/// Determine the audio source type for a given data source
fn get_audio_source_type(
    source: &crate::config::Source,
    data_paths: &DataPaths,
) -> anyhow::Result<SourceType> {
    if source.converter == "audio" {
        let path = data_paths
            .audio_file(&source.id)
            .ok_or_else(|| anyhow::anyhow!("No audio file found for {}", source.id))?;

        Ok(SourceType::AudioFile { path })
    } else {
        let digits = load_base12_digits(source, data_paths)?;
        Ok(SourceType::Synthesized { base_digits: digits })
    }
}

/// Load base-12 digits for a source (used for audio synthesis)
fn load_base12_digits(
    source: &crate::config::Source,
    data_paths: &DataPaths,
) -> anyhow::Result<Vec<u8>> {
    load_digits_for_audio_base(source, data_paths, 12)
}

fn load_digits_for_audio_base(
    source: &crate::config::Source,
    data_paths: &DataPaths,
    base: u32,
) -> anyhow::Result<Vec<u8>> {
    use crate::converters;

    let max_points = 5000; // Reasonable length for audio

    if source.converter.starts_with("math.") {
        let generator = MathGenerator::from_converter_string(&source.converter)
            .ok_or_else(|| anyhow::anyhow!("Unknown math converter '{}'", source.converter))?;
        let base12 = generator.generate(max_points);
        Ok(match base {
            4 => base12.iter().map(|&d| d % 4).collect(),
            6 => base12.iter().map(|&d| d % 6).collect(),
            _ => base12,
        })
    } else {
        match source.converter.as_str() {
            "audio" => {
                let path = data_paths
                    .audio_file(&source.id)
                    .ok_or_else(|| anyhow::anyhow!("No audio file found for {}", source.id))?;
                converters::load_audio_raw(&path, base)
                    .map_err(|e| anyhow::anyhow!("Failed to load audio for synthesis {}: {}", source.id, e))
            }
            "dna" => {
                let path = data_paths.dna_file(&source.url, &source.id);
                converters::load_dna_raw(&path, base)
                    .map_err(|e| anyhow::anyhow!("Failed to load DNA for synthesis {}: {}", source.id, e))
            }
            "cosmos" => {
                let path = data_paths.cosmos_file(&source.id);
                converters::load_cosmos_raw(&path, base)
                    .map_err(|e| anyhow::anyhow!("Failed to load cosmos data for synthesis {}: {}", source.id, e))
            }
            "finance" => {
                let path = data_paths.finance_file(&source.url, &source.id);
                converters::load_finance_raw(&path, base)
                    .map_err(|e| anyhow::anyhow!("Failed to load finance data for synthesis {}: {}", source.id, e))
            }
            "pdb_backbone" => {
                let path = data_paths.protein_file(&source.url, &source.id);
                converters::load_pdb_backbone_raw(&path, base)
                    .map_err(|e| anyhow::anyhow!("Failed to load PDB backbone for synthesis {}: {}", source.id, e))
            }
            "pdb_sequence" => {
                let path = data_paths.protein_file(&source.url, &source.id);
                converters::load_pdb_sequence_raw(&path, base)
                    .map_err(|e| anyhow::anyhow!("Failed to load PDB sequence for synthesis {}: {}", source.id, e))
            }
            "pdb_structure" => Err(anyhow::anyhow!(
                "Converter '{}' does not produce base-12 digits for synthesis",
                source.converter
            )),
            _ => Err(anyhow::anyhow!(
                "Unsupported converter '{}' for audio synthesis",
                source.converter
            )),
        }
    }
}

fn queue_walk_load(
    walk_load_tx: &std::sync::mpsc::Sender<WalkLoadResult>,
    pending_walk_loads: &mut BTreeMap<String, PendingWalkLoad>,
    next_walk_load_request_id: &mut u64,
    source: crate::config::Source,
    config: &Config,
    data_paths: &DataPaths,
    max_points: usize,
    mapping_name: &str,
    base: u32,
    color: [f32; 3],
    prepare_audio: bool,
) {
    let request_id = *next_walk_load_request_id;
    *next_walk_load_request_id += 1;
    pending_walk_loads.insert(
        source.id.clone(),
        PendingWalkLoad {
            request_id,
            source_name: source.name.clone(),
        },
    );

    let tx = walk_load_tx.clone();
    let config = config.clone();
    let data_paths = data_paths.clone();
    let mapping_name = mapping_name.to_string();

    std::thread::spawn(move || {
        let walk_data = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            load_walk_data(
                &source,
                &config,
                &data_paths,
                max_points,
                &mapping_name,
                base,
                color,
            )
        })) {
            Ok(walk_data) => walk_data,
            Err(e) => {
                error!("[GUI] PANIC in load_walk_data for {}: {:?}", source.id, e);
                None
            }
        };

        if tx
            .send(WalkLoadResult {
                request_id,
                source,
                walk_data,
                prepare_audio,
            })
            .is_err()
        {
            debug!("[GUI] Walk load receiver dropped");
        }
    });
}

fn prepare_audio_for_source(
    source: &crate::config::Source,
    data_paths: &DataPaths,
    engine: &mut AudioEngine,
    audio_settings: &AudioSettings,
    selected_base: u32,
    flight_mode: bool,
    flight_playing: bool,
    flight_speed: f32,
    walk_len: usize,
    audio_prep_tx: &std::sync::mpsc::Sender<AudioPrepResult>,
    pending_audio_preps: &mut BTreeMap<String, PendingAudioPrep>,
    next_audio_prep_request_id: &mut u64,
) {
    debug!("[GUI] Preparing audio for source: {}", source.id);
    let audio_source_type = if audio_settings.force_synthesis {
        let digits = match load_digits_for_audio_base(source, data_paths, selected_base) {
            Ok(digits) => digits,
            Err(error) => {
                warn!("[GUI] Skipping generated audio prep for {}: {}", source.id, error);
                return;
            }
        };
        debug!(
            "[GUI] Using synthesized digits for {} ({} digits, base {})",
            source.id,
            digits.len(),
            selected_base
        );
        SourceType::Synthesized { base_digits: digits }
    } else {
        debug!("[GUI] Getting audio source type for {}", source.id);
        match get_audio_source_type(source, data_paths) {
            Ok(source_type) => source_type,
            Err(e) => {
                warn!("[GUI] Skipping audio prep for {}: {}", source.id, e);
                return;
            }
        }
    };
    debug!(
        "[GUI] Preparing audio for source: {} (type: {:?})",
        source.id,
        audio_source_type
    );

    if let Err(error) = validate_digit_playback_rule(audio_settings, &audio_source_type) {
        error!("[GUI] {}", error);
        return;
    }

    if let Err(error) = validate_step_trigger_playback_rule(audio_settings, &audio_source_type, flight_mode) {
        error!("[GUI] {}", error);
        return;
    }

    if let Err(error) = validate_zero_tolerance_rules(audio_settings, flight_mode) {
        error!("[GUI] {}", error);
        return;
    }

    let speed_is_ssot = flight_mode;
    let flight_duration = if speed_is_ssot {
        match flight_duration_from_speed_hz(walk_len, flight_speed) {
            Ok(duration) => duration,
            Err(error) => {
                error!("[GUI] {}", error);
                return;
            }
        }
    } else if walk_len > 0 && flight_speed > 0.0 {
        walk_len as f32 / flight_speed
    } else {
        30.0
    };

    pending_audio_preps.remove(&source.id);

    let result = if speed_is_ssot || audio_settings.sync_to_flight {
        if let SourceType::AudioFile { path } = &audio_source_type {
            debug!(
                "[GUI] Queueing background audio stretch for {} to {:.1}s",
                source.id,
                flight_duration
            );
            queue_audio_file_prep(
                audio_prep_tx,
                pending_audio_preps,
                next_audio_prep_request_id,
                source.clone(),
                path.clone(),
                flight_duration,
            );
            return;
        }
        debug!(
            "[GUI] Syncing generated audio to point rate: {:.3} Hz (walk: {} pts, duration: {:.1}s)",
            flight_speed,
            walk_len,
            flight_duration,
        );
        engine.prepare_source_synced_to_points(&source.id, audio_source_type, flight_speed)
    } else {
        engine.prepare_source(&source.id, audio_source_type)
    };

    match result {
        Ok(_) => {
            debug!("[GUI] Audio source prepared: {}", source.id);
            if flight_mode && flight_playing && audio_settings.enabled {
                debug!("[GUI] Auto-starting audio for newly selected source");
                engine.play(audio_settings);
            }
        }
        Err(e) => error!("[GUI] Failed to prepare audio source {}: {}", source.id, e),
    }
}

fn refresh_selected_audio_sources(
    selected_sources: &std::collections::HashSet<String>,
    walks: &BTreeMap<String, WalkData>,
    config: &Config,
    data_paths: &DataPaths,
    audio_engine: Option<&mut AudioEngine>,
    audio_settings: &AudioSettings,
    selected_base: u32,
    flight_mode: bool,
    flight_playing: bool,
    flight_speed: f32,
    audio_prep_tx: &std::sync::mpsc::Sender<AudioPrepResult>,
    pending_audio_preps: &mut BTreeMap<String, PendingAudioPrep>,
    next_audio_prep_request_id: &mut u64,
) {
    let Some(engine) = audio_engine else {
        if !selected_sources.is_empty() {
            warn!("[GUI] No audio engine available while refreshing audio sources");
        }
        return;
    };

    for source_id in selected_sources {
        let Some(source) = config.sources.iter().find(|s| &s.id == source_id) else {
            continue;
        };
        let walk_len = walks.get(source_id).map(|w| w.points.len()).unwrap_or(0);
        prepare_audio_for_source(
            source,
            data_paths,
            engine,
            audio_settings,
            selected_base,
            flight_mode,
            flight_playing,
            flight_speed,
            walk_len,
            audio_prep_tx,
            pending_audio_preps,
            next_audio_prep_request_id,
        );
    }
}

fn queue_audio_file_prep(
    audio_prep_tx: &std::sync::mpsc::Sender<AudioPrepResult>,
    pending_audio_preps: &mut BTreeMap<String, PendingAudioPrep>,
    next_audio_prep_request_id: &mut u64,
    source: crate::config::Source,
    path: PathBuf,
    target_duration_secs: f32,
) {
    let request_id = *next_audio_prep_request_id;
    *next_audio_prep_request_id += 1;
    pending_audio_preps.insert(
        source.id.clone(),
        PendingAudioPrep {
            request_id,
            source_name: source.name.clone(),
        },
    );

    let tx = audio_prep_tx.clone();
    std::thread::spawn(move || {
        let result = crate::audio::load_and_stretch(&path, target_duration_secs).map(
            |(samples, sample_rate, channels)| PreparedAudioFile {
                path,
                samples,
                sample_rate,
                channels,
            },
        );

        if tx
            .send(AudioPrepResult {
                request_id,
                source,
                result,
            })
            .is_err()
        {
            debug!("[GUI] Audio prep receiver dropped");
        }
    });
}

fn format_pending_audio(pending_audio_preps: &BTreeMap<String, PendingAudioPrep>) -> String {
    if pending_audio_preps.is_empty() {
        return "sources".to_string();
    }

    let count = pending_audio_preps.len();
    let mut names = pending_audio_preps
        .values()
        .map(|pending| pending.source_name.as_str());

    if count == 1 {
        let name = names.next().unwrap_or("source");
        return format!("1 source: {}", name);
    }

    let first = names.next().unwrap_or("source");
    let second = names.next().unwrap_or("source");
    if count == 2 {
        return format!("2 sources: {}, {}", first, second);
    }

    format!("{} sources: {}, {}...", count, first, second)
}

fn format_pending_walks(pending_walk_loads: &BTreeMap<String, PendingWalkLoad>) -> String {
    if pending_walk_loads.is_empty() {
        return "walks".to_string();
    }

    let count = pending_walk_loads.len();
    let mut names = pending_walk_loads
        .values()
        .map(|pending| pending.source_name.as_str());

    if count == 1 {
        let name = names.next().unwrap_or("walk");
        return format!("1 walk: {}", name);
    }

    let first = names.next().unwrap_or("walk");
    let second = names.next().unwrap_or("walk");
    if count == 2 {
        return format!("2 walks: {}, {}", first, second);
    }

    format!("{} walks: {}, {}...", count, first, second)
}

fn load_walk_data(
    source: &crate::config::Source,
    config: &Config,
    data_paths: &DataPaths,
    max_points: usize,
    mapping_name: &str,
    base: u32,
    color: [f32; 3],
) -> Option<WalkData> {
    use crate::converters;
    use crate::walk::{walk_base4, walk_base6};

    // PDB structure mode: raw Cα coordinates, bypasses walk engine entirely
    if source.converter == "pdb_structure" {
        let path = data_paths.protein_file(&source.url, &source.id);

        if !path.exists() {
            warn!("No PDB file found for {}: {:?}", source.id, path);
            return None;
        }

        match converters::load_pdb_structure(&path) {
            Ok((points, residues, chain_breaks)) => {
                info!(
                    "Loaded PDB structure {} with {} Cα atoms, {} chain breaks",
                    source.id,
                    points.len(),
                    chain_breaks.len()
                );

                // Center the structure around the origin
                let n = points.len() as f32;
                let cx = points.iter().map(|p| p[0]).sum::<f32>() / n;
                let cy = points.iter().map(|p| p[1]).sum::<f32>() / n;
                let cz = points.iter().map(|p| p[2]).sum::<f32>() / n;
                let centered: Vec<[f32; 3]> = points.iter()
                    .map(|p| [p[0] - cx, p[1] - cy, p[2] - cz])
                    .collect();

                // Per-residue colors
                let point_colors: Vec<[f32; 3]> = residues.iter()
                    .map(|r| converters::residue_color(r))
                    .collect();
                // Pad if residue count doesn't match point count
                let point_colors = if point_colors.len() < centered.len() {
                    let mut pc = point_colors;
                    pc.resize(centered.len(), [0.5, 0.5, 0.5]);
                    pc
                } else {
                    point_colors
                };

                // No revisit counting for structure mode - each position is unique
                let (revisit_counts, point_positions) = build_point_visit_maps(&centered);

                return Some(WalkData {
                    name: source.name.clone(),
                    points: centered,
                    color,
                    visible: true,
                    revisit_counts,
                    point_positions,
                    point_colors: Some(point_colors),
                    chain_breaks: if chain_breaks.is_empty() { None } else { Some(chain_breaks) },
                });
            }
            Err(e) => {
                warn!("Failed to load PDB structure {}: {}", source.id, e);
                return None;
            }
        }
    }

    // All conversion happens on-the-fly - no pre-computed storage
    let digits = if source.converter.starts_with("math.") {
        // Math always generates base-12; reduce mod target base
        let base12 = MathGenerator::from_converter_string(&source.converter)?.generate(max_points);
        match base {
            4 => base12.iter().map(|&d| d % 4).collect(),
            6 => base12.iter().map(|&d| d % 6).collect(),
            _ => base12,
        }
    } else {
        match source.converter.as_str() {
            "audio" => {
                let path = match data_paths.audio_file(&source.id) {
                    Some(path) => path,
                    None => {
                        warn!("No audio file found for {}", source.id);
                        return None;
                    }
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
                let path = data_paths.dna_file(&source.url, &source.id);

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
                let path = data_paths.cosmos_file(&source.id);

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
                let path = data_paths.finance_file(&source.url, &source.id);

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
            "pdb_backbone" => {
                let path = data_paths.protein_file(&source.url, &source.id);

                if !path.exists() {
                    warn!("No PDB file found for {}: {:?}", source.id, path);
                    return None;
                }

                match converters::load_pdb_backbone_raw(&path, base) {
                    Ok(data) => data,
                    Err(e) => {
                        warn!("Failed to convert PDB backbone {}: {}", source.id, e);
                        return None;
                    }
                }
            }
            "pdb_sequence" => {
                let path = data_paths.protein_file(&source.url, &source.id);

                if !path.exists() {
                    warn!("No PDB file found for {}: {:?}", source.id, path);
                    return None;
                }

                match converters::load_pdb_sequence_raw(&path, base) {
                    Ok(data) => data,
                    Err(e) => {
                        warn!("Failed to convert PDB sequence {}: {}", source.id, e);
                        return None;
                    }
                }
            }
            _ => return None,
        }
    };

    info!("Loaded {} base-{} digits for {}", digits.len(), base, source.id);

    let points = match base {
        4 => walk_base4(&digits, max_points),
        6 => {
            let mapping = match config.get_mapping_base6(mapping_name) {
                Ok(mapping) => mapping,
                Err(e) => {
                    warn!("Failed to load base-6 mapping '{}' for {}: {}", mapping_name, source.id, e);
                    return None;
                }
            };
            walk_base6(&digits, &mapping, max_points)
        }
        _ => {
            let mapping = match config.get_mapping(mapping_name) {
                Ok(mapping) => mapping,
                Err(e) => {
                    warn!("Failed to load mapping '{}' for {}: {}", mapping_name, source.id, e);
                    return None;
                }
            };
            walk_base12(&digits, &mapping, max_points)
        }
    };
    info!("Generated {} walk points for {}", points.len(), source.id);

    let (revisit_counts, point_positions) = build_point_visit_maps(&points);
    let max_revisits = revisit_counts.values().max().copied().unwrap_or(1);
    info!("Max revisits for {}: {} at {} unique positions", source.id, max_revisits, revisit_counts.len());

    Some(WalkData {
        name: source.name.clone(),
        points,
        color,
        visible: true,
        revisit_counts,
        point_positions,
        point_colors: None,
        chain_breaks: None,
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

fn apply_color_scheme_to_walks(
    walks: &mut BTreeMap<String, WalkData>,
    color_pool: &mut ColorPool,
) {
    for (source_id, walk) in walks.iter_mut() {
        walk.color = color_pool.get_color(source_id);
    }
}


const DISTINCT_COLORS: [[f32; 3]; 12] = [
    [1.00, 0.30, 0.30],
    [0.30, 0.85, 0.40],
    [0.40, 0.60, 1.00],
    [1.00, 0.85, 0.20],
    [0.90, 0.40, 0.90],
    [0.20, 0.90, 0.90],
    [1.00, 0.60, 0.20],
    [0.70, 0.50, 1.00],
    [0.60, 1.00, 0.60],
    [1.00, 0.50, 0.60],
    [0.50, 0.80, 1.00],
    [0.95, 0.75, 0.50],
];

const WONG_COLORS: [[f32; 3]; 12] = [
    [0.90, 0.62, 0.00],
    [0.34, 0.71, 0.91],
    [0.00, 0.62, 0.45],
    [0.94, 0.89, 0.26],
    [0.00, 0.45, 0.70],
    [0.84, 0.37, 0.00],
    [0.80, 0.47, 0.65],
    [0.55, 0.55, 0.55],
    [0.44, 0.56, 0.00],
    [0.70, 0.44, 0.86],
    [0.20, 0.80, 0.80],
    [1.00, 0.70, 0.30],
];

const TOL_COLORS: [[f32; 3]; 12] = [
    [0.20, 0.13, 0.53],
    [0.53, 0.80, 0.93],
    [0.27, 0.67, 0.60],
    [0.07, 0.47, 0.20],
    [0.60, 0.60, 0.20],
    [0.87, 0.80, 0.47],
    [0.80, 0.40, 0.47],
    [0.67, 0.27, 0.60],
    [0.88, 0.52, 0.28],
    [0.38, 0.38, 0.80],
    [0.55, 0.72, 0.30],
    [0.94, 0.64, 0.76],
];

const PASTEL_COLORS: [[f32; 3]; 12] = [
    [0.97, 0.64, 0.64],
    [0.64, 0.85, 0.72],
    [0.67, 0.77, 0.97],
    [0.98, 0.88, 0.55],
    [0.86, 0.68, 0.95],
    [0.61, 0.89, 0.91],
    [0.99, 0.74, 0.51],
    [0.75, 0.73, 0.98],
    [0.74, 0.95, 0.66],
    [0.98, 0.69, 0.78],
    [0.72, 0.86, 0.99],
    [0.95, 0.81, 0.70],
];

const NEON_COLORS: [[f32; 3]; 12] = [
    [1.00, 0.18, 0.41],
    [0.18, 1.00, 0.56],
    [0.22, 0.73, 1.00],
    [1.00, 0.95, 0.18],
    [0.92, 0.25, 1.00],
    [0.12, 1.00, 0.98],
    [1.00, 0.55, 0.08],
    [0.58, 0.34, 1.00],
    [0.67, 1.00, 0.20],
    [1.00, 0.39, 0.64],
    [0.42, 0.90, 1.00],
    [1.00, 0.74, 0.18],
];

const EARTH_COLORS: [[f32; 3]; 12] = [
    [0.74, 0.37, 0.22],
    [0.35, 0.57, 0.31],
    [0.24, 0.45, 0.62],
    [0.76, 0.63, 0.24],
    [0.55, 0.40, 0.60],
    [0.24, 0.58, 0.55],
    [0.86, 0.54, 0.26],
    [0.45, 0.34, 0.68],
    [0.56, 0.68, 0.31],
    [0.78, 0.47, 0.41],
    [0.47, 0.66, 0.74],
    [0.67, 0.52, 0.38],
];

/// Color pool for dynamic assignment with maximum contrast
#[derive(Default)]
struct ColorPool {
    scheme: ColorScheme,
    /// Maps source ID to assigned color index
    assignments: std::collections::HashMap<String, usize>,
    /// Tracks which color indices are in use
    in_use: std::collections::HashSet<usize>,
}

impl ColorPool {
    fn new(scheme: ColorScheme) -> Self {
        Self {
            scheme,
            assignments: Default::default(),
            in_use: Default::default(),
        }
    }

    fn palette(&self) -> &'static [[f32; 3]; 12] {
        match self.scheme {
            ColorScheme::Distinct => &DISTINCT_COLORS,
            ColorScheme::Wong => &WONG_COLORS,
            ColorScheme::Tol => &TOL_COLORS,
            ColorScheme::Pastel => &PASTEL_COLORS,
            ColorScheme::Neon => &NEON_COLORS,
            ColorScheme::Earth => &EARTH_COLORS,
        }
    }

    fn set_scheme(&mut self, scheme: ColorScheme) {
        self.scheme = scheme;
    }

    /// Get or assign a color for a source ID
    /// Returns the RGB color array
    fn get_color(&mut self, source_id: &str) -> [f32; 3] {
        let palette = self.palette();
        // Return existing assignment
        if let Some(&idx) = self.assignments.get(source_id) {
            return palette[idx % palette.len()];
        }

        // Find first unused color index
        let mut idx = 0;
        while self.in_use.contains(&idx) && idx < palette.len() {
            idx += 1;
        }

        // If all colors used, find color with maximum distance to currently used colors
        if idx >= palette.len() {
            idx = self.find_best_reuse_color();
        }

        self.assignments.insert(source_id.to_string(), idx);
        self.in_use.insert(idx);
        palette[idx % palette.len()]
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
            .filter(|&&i| i < self.palette().len())
            .map(|&i| self.palette()[i])
            .collect();

        if used_colors.is_empty() {
            return 0;
        }

        // Find palette color with maximum minimum distance to used colors
        let mut best_idx = 0;
        let mut best_min_dist = 0.0f32;

        for (idx, color) in self.palette().iter().enumerate() {
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
