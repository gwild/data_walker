//! GUI Automation API for autonomous testing
//!
//! Provides:
//! - CLI flags for automated startup actions
//! - IPC command channel for real-time control
//! - State reporting for test validation

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use tracing::{debug, error, info};

/// Automation settings from CLI
#[derive(Clone, Debug, Default)]
pub struct AutomationConfig {
    /// Sources to auto-select on startup
    pub auto_select: Vec<String>,
    /// Auto-enable flight mode
    pub auto_flight: bool,
    /// Auto-start flight playback
    pub auto_play: bool,
    /// Quit after N seconds (0 = never)
    pub quit_after_secs: f64,
    /// Take screenshot and quit
    pub screenshot_and_quit: Option<String>,
    /// Enable IPC server on this port (0 = disabled)
    pub ipc_port: u16,
    /// Log all GUI events to stdout as JSON
    pub json_events: bool,
}

/// Commands that can be sent via IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum AutoCommand {
    /// Select a data source
    SelectSource { id: String },
    /// Deselect a data source
    DeselectSource { id: String },
    /// Deselect all sources
    DeselectAll,
    /// Enable/disable flight mode
    SetFlightMode { enabled: bool },
    /// Start/stop flight playback
    SetFlightPlaying { playing: bool },
    /// Set flight progress (0.0-1.0)
    SetFlightProgress { progress: f32 },
    /// Set flight speed (Hz)
    SetFlightSpeed { speed: f32 },
    /// Change base (4, 6, or 12)
    SetBase { base: u32 },
    /// Change mapping
    SetMapping { name: String },
    /// Take a screenshot
    Screenshot { path: Option<String> },
    /// Get current state
    GetState,
    /// Quit the application
    Quit,
}

/// Response from IPC commands
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AutoResponse {
    Ok { message: String },
    Error { message: String },
    State(GuiState),
}

/// Snapshot of GUI state for testing/validation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GuiState {
    pub selected_sources: Vec<String>,
    pub loaded_walks: Vec<WalkInfo>,
    pub flight_mode: bool,
    pub flight_playing: bool,
    pub flight_progress: f32,
    pub flight_speed: f32,
    pub selected_base: u32,
    pub selected_mapping: String,
    pub camera_position: [f32; 3],
    pub camera_target: [f32; 3],
    pub frame_count: u64,
    pub uptime_secs: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalkInfo {
    pub id: String,
    pub name: String,
    pub num_points: usize,
    pub color: [f32; 3],
}

/// GUI event for JSON logging
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum GuiEvent {
    Started { timestamp: f64 },
    SourceSelected { id: String, timestamp: f64 },
    SourceDeselected { id: String, timestamp: f64 },
    FlightModeChanged { enabled: bool, timestamp: f64 },
    FlightPlayingChanged { playing: bool, timestamp: f64 },
    BaseChanged { base: u32, timestamp: f64 },
    MappingChanged { mapping: String, timestamp: f64 },
    ScreenshotTaken { path: String, timestamp: f64 },
    Error { message: String, timestamp: f64 },
    Crashed { message: String, location: String, timestamp: f64 },
    Quit { timestamp: f64 },
}

/// Shared command queue for IPC -> GUI communication
pub type CommandQueue = Arc<Mutex<Vec<AutoCommand>>>;
pub type SharedGuiState = Arc<Mutex<GuiState>>;

/// Create a new command queue
pub fn new_command_queue() -> CommandQueue {
    Arc::new(Mutex::new(Vec::new()))
}

pub fn new_shared_state() -> SharedGuiState {
    Arc::new(Mutex::new(GuiState::default()))
}

/// Start IPC server on given port
pub fn start_ipc_server(
    port: u16,
    cmd_queue: CommandQueue,
    shared_state: SharedGuiState,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port))?;
    info!("[IPC] Server listening on port {}", port);

    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let queue = cmd_queue.clone();
                    let state = shared_state.clone();
                    thread::spawn(move || handle_ipc_client(stream, queue, state));
                }
                Err(e) => {
                    error!("[IPC] Accept error: {}", e);
                }
            }
        }
    });

    Ok(())
}

fn handle_ipc_client(
    mut stream: TcpStream,
    cmd_queue: CommandQueue,
    shared_state: SharedGuiState,
) {
    let peer = stream.peer_addr().ok();
    debug!("[IPC] Client connected: {:?}", peer);

    let reader = BufReader::new(stream.try_clone().unwrap());

    for line in reader.lines() {
        match line {
            Ok(json) => {
                debug!("[IPC] Received: {}", json);
                match serde_json::from_str::<AutoCommand>(&json) {
                    Ok(cmd) => {
                        if matches!(cmd, AutoCommand::GetState) {
                            let response = match shared_state.lock() {
                                Ok(state) => AutoResponse::State(state.clone()),
                                Err(e) => AutoResponse::Error {
                                    message: format!("State lock error: {}", e),
                                },
                            };
                            let _ = writeln!(stream, "{}", serde_json::to_string(&response).unwrap());
                        } else {
                            if let Ok(mut queue) = cmd_queue.lock() {
                                queue.push(cmd.clone());
                            }
                            let response = AutoResponse::Ok {
                                message: format!("Queued: {:?}", cmd)
                            };
                            let _ = writeln!(stream, "{}", serde_json::to_string(&response).unwrap());
                        }
                    }
                    Err(e) => {
                        let response = AutoResponse::Error {
                            message: format!("Parse error: {}", e)
                        };
                        let _ = writeln!(stream, "{}", serde_json::to_string(&response).unwrap());
                    }
                }
            }
            Err(e) => {
                debug!("[IPC] Read error: {}", e);
                break;
            }
        }
    }

    debug!("[IPC] Client disconnected: {:?}", peer);
}

/// Log a GUI event as JSON to stdout
pub fn log_event(event: &GuiEvent) {
    if let Ok(json) = serde_json::to_string(event) {
        println!("GUI_EVENT: {}", json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_serialization() {
        let cmd = AutoCommand::SelectSource { id: "pi".to_string() };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("select_source"));
        assert!(json.contains("pi"));

        let parsed: AutoCommand = serde_json::from_str(&json).unwrap();
        match parsed {
            AutoCommand::SelectSource { id } => assert_eq!(id, "pi"),
            _ => panic!("Wrong command type"),
        }
    }

    #[test]
    fn test_state_serialization() {
        let state = GuiState {
            selected_sources: vec!["pi".to_string()],
            flight_mode: true,
            flight_progress: 0.5,
            ..Default::default()
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("pi"));
        assert!(json.contains("0.5"));
    }

    #[test]
    fn test_state_response_serialization() {
        let response = AutoResponse::State(GuiState {
            selected_sources: vec!["pi".to_string()],
            selected_mapping: "Identity".to_string(),
            ..Default::default()
        });
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"status\":\"state\""));
        assert!(json.contains("Identity"));
    }
}
