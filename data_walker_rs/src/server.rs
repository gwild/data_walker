//! HTTP Server - Serves walk data via REST API
//!
//! Endpoints:
//! - GET /api/config       → Full config as JSON
//! - GET /api/walks        → List all walks with metadata
//! - GET /api/walks/:id    → Base12 array for specific walk
//! - GET /api/walks/:id/points → Computed 3D points
//! - GET /api/mappings     → Available base12 mappings

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

use crate::state::{AppState, WalkMeta};
use crate::walk;

/// Start the HTTP server
pub async fn serve(state: AppState, port: u16) -> anyhow::Result<()> {
    tracing::info!("Initializing HTTP server on port {}", port);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    tracing::debug!("CORS layer configured: allow_origin=Any");

    // API routes
    let api = Router::new()
        .route("/config", get(get_config))
        .route("/walks", get(list_walks))
        .route("/walks/:id", get(get_walk))
        .route("/walks/:id/points", get(get_walk_points))
        .route("/mappings", get(get_mappings))
        .route("/categories", get(get_categories))
        .with_state(state.clone());
    tracing::debug!("API routes registered");

    // Static file serving from ./web directory
    let static_files = ServeDir::new("web");
    tracing::debug!("Static file serving from ./web");

    let app = Router::new()
        .nest("/api", api)
        .fallback_service(static_files)
        .layer(cors);

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Starting server on http://localhost:{}", port);
    tracing::info!("  API: http://localhost:{}/api/walks", port);
    tracing::info!("  Web: http://localhost:{}/", port);
    tracing::info!("  Available sources: {}", state.config.sources.len());

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Server bound to {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}

/// GET /api/config - Full config as JSON
async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    tracing::debug!("GET /api/config");
    Json((*state.config).clone())
}

/// GET /api/walks - List all walks
async fn list_walks(State(state): State<AppState>) -> impl IntoResponse {
    tracing::info!("GET /api/walks - listing {} sources", state.config.sources.len());
    let walks: Vec<WalkMeta> = state.config.sources.iter().map(WalkMeta::from).collect();
    tracing::debug!("Returning {} walk metadata entries", walks.len());
    Json(walks)
}

/// GET /api/walks/:id - Get base12 data for a walk
async fn get_walk(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    tracing::info!("GET /api/walks/{}", id);
    match state.load_walk(&id).await {
        Some(walk) => {
            tracing::debug!("Walk '{}' loaded: {} base12 digits", id, walk.base12.len());
            Ok(Json(WalkResponse {
                id: walk.id,
                name: walk.name,
                category: walk.category,
                subcategory: walk.subcategory,
                mapping: walk.mapping,
                base12: walk.base12,
            }))
        }
        None => {
            tracing::warn!("Walk '{}' not found", id);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

#[derive(Serialize)]
struct WalkResponse {
    id: String,
    name: String,
    category: String,
    subcategory: String,
    mapping: String,
    base12: Vec<u8>,
}

/// Query params for points endpoint
#[derive(Deserialize)]
struct PointsQuery {
    mapping: Option<String>,
    max_points: Option<usize>,
}

/// GET /api/walks/:id/points - Get computed 3D points
async fn get_walk_points(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<PointsQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    tracing::info!("GET /api/walks/{}/points mapping={:?} max_points={:?}",
        id, params.mapping, params.max_points);

    let walk = match state.load_walk(&id).await {
        Some(w) => {
            tracing::debug!("Walk '{}' found with {} base12 digits", id, w.base12.len());
            w
        }
        None => {
            tracing::error!("Walk '{}' NOT FOUND - returning 404", id);
            return Err(StatusCode::NOT_FOUND);
        }
    };

    let mapping_name = params.mapping.as_deref().unwrap_or(&walk.mapping);
    let mapping = state.get_mapping(mapping_name);
    let max_points = params.max_points.unwrap_or(10000);

    tracing::debug!("Computing walk with mapping '{}', max_points={}", mapping_name, max_points);
    let points = walk::walk_base12(&walk.base12, &mapping, max_points);
    tracing::info!("Walk '{}' computed: {} 3D points", id, points.len());

    Ok(Json(PointsResponse {
        id: walk.id,
        name: walk.name,
        mapping: mapping_name.to_string(),
        num_points: points.len(),
        points,
    }))
}

#[derive(Serialize)]
struct PointsResponse {
    id: String,
    name: String,
    mapping: String,
    num_points: usize,
    points: Vec<[f32; 3]>,
}

/// GET /api/mappings - List available mappings
async fn get_mappings(State(state): State<AppState>) -> impl IntoResponse {
    tracing::debug!("GET /api/mappings");
    let mappings: HashMap<String, Vec<u8>> = state
        .config
        .mappings
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    tracing::debug!("Returning {} mappings: {:?}", mappings.len(), mappings.keys().collect::<Vec<_>>());
    Json(mappings)
}

/// GET /api/categories - List categories
async fn get_categories(State(state): State<AppState>) -> impl IntoResponse {
    tracing::debug!("GET /api/categories");
    let categories = state.config.categories.clone();
    tracing::debug!("Returning {} categories", categories.len());
    Json(categories)
}
