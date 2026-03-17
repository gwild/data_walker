//! Thumbnail Generator
//!
//! Opens a small three-d window, renders each source's walk,
//! captures pixels, and saves as PNG thumbnails.

use std::collections::HashMap;
use std::path::Path;
use three_d::*;
use tracing::{info, warn};

use crate::config::{Config, DataPaths};
use crate::converters;
use crate::converters::math::MathGenerator;
use crate::walk::{walk_base12, walk_base4};

/// Walk data for thumbnail rendering
struct WalkRender {
    points: Vec<[f32; 3]>,
    color: [f32; 3],
    revisit_counts: HashMap<(i64, i64, i64), u32>,
    point_positions: HashMap<(i64, i64, i64), [f32; 3]>,
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
) -> (HashMap<(i64, i64, i64), u32>, HashMap<(i64, i64, i64), [f32; 3]>) {
    let mut revisit_counts = HashMap::new();
    let mut point_positions = HashMap::new();

    for &point in points {
        let key = point_key(point);
        *revisit_counts.entry(key).or_insert(0) += 1;
        point_positions.entry(key).or_insert(point);
    }

    (revisit_counts, point_positions)
}

/// Generate thumbnails for all sources with available data
pub fn generate(config: &Config, data_dir: &Path, output_dir: &Path, size: u32) -> anyhow::Result<()> {
    std::fs::create_dir_all(output_dir)?;
    let data_paths = DataPaths::new(data_dir.to_path_buf());

    // Collect sources that have data available
    let mut sources_to_render: Vec<(crate::config::Source, String)> = Vec::new();

    for source in &config.sources {
        if check_data_exists(&source.id, &source.converter, &source.url, &data_paths) {
            let filename = format!("{}.png", source.id);
            sources_to_render.push((source.clone(), filename));
        } else {
            info!("Skipping {} - no data available", source.id);
        }
    }

    let total = sources_to_render.len();
    println!("Generating {} thumbnails ({}x{})...", total, size, size);

    // Open window
    let window = Window::new(WindowSettings {
        title: "Data Walker - Thumbnail Generator".to_string(),
        max_size: Some((size, size)),
        min_size: (size, size),
        ..Default::default()
    })?;

    let context = window.gl();

    // Camera
    let mut camera = Camera::new_perspective(
        Viewport {
            x: 0,
            y: 0,
            width: size,
            height: size,
        },
        vec3(0.0, 50.0, 200.0),
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 1.0, 0.0),
        degrees(45.0),
        0.1,
        10000.0,
    );

    let mut current_idx: usize = 0;
    let output_dir = output_dir.to_path_buf();
    let config_clone = config.clone();
    let mut index_entries: Vec<serde_json::Value> = Vec::new();

    window.render_loop(move |frame_input| {
        if current_idx >= sources_to_render.len() {
            // Write index.json
            let index = serde_json::json!({
                "generated": chrono::Local::now().to_rfc3339(),
                "thumbnails": index_entries,
            });
            let index_path = output_dir.join("index.json");
            if let Err(e) = std::fs::write(&index_path, serde_json::to_string_pretty(&index).unwrap_or_default()) {
                warn!("Failed to write index.json: {}", e);
            } else {
                info!("Wrote {}", index_path.display());
            }
            println!("\nDone! Generated {} thumbnails in {}", index_entries.len(), output_dir.display());
            return FrameOutput { exit: true, ..Default::default() };
        }

        let (source, filename) = &sources_to_render[current_idx];
        print!("\r[{}/{}] {}...", current_idx + 1, total, source.name);

        // Load walk data
        let walk = match load_walk_for_thumbnail(source, &config_clone, &data_paths) {
            Some(w) => w,
            None => {
                warn!("Failed to load walk data for {}", source.id);
                current_idx += 1;
                return FrameOutput::default();
            }
        };

        // Auto-fit camera to walk bounding box
        auto_fit_camera(&walk.points, &mut camera);
        camera.set_viewport(frame_input.viewport);

        // Build geometry
        let color = Srgba::new(
            (walk.color[0] * 255.0) as u8,
            (walk.color[1] * 255.0) as u8,
            (walk.color[2] * 255.0) as u8,
            255,
        );

        let mut renderables: Vec<Gm<InstancedMesh, ColorMaterial>> = Vec::new();

        // Lines
        if walk.points.len() >= 2 {
            let mut instances = Instances::default();
            instances.transformations = Vec::new();
            instances.colors = Some(Vec::new());

            let max_revisits = walk.revisit_counts.values().max().copied().unwrap_or(1) as f32;
            let ln_max = max_revisits.ln().max(1.0);
            let line_scale: f32 = 0.3;

            for i in 0..walk.points.len() - 1 {
                let p1 = vec3(walk.points[i][0], walk.points[i][1], walk.points[i][2]);
                let p2 = vec3(walk.points[i + 1][0], walk.points[i + 1][1], walk.points[i + 1][2]);

                let dir = p2 - p1;
                let length = dir.magnitude();

                if length > 0.001 {
                    let key1 = point_key(walk.points[i]);
                    let key2 = point_key(walk.points[i + 1]);
                    let count1 = *walk.revisit_counts.get(&key1).unwrap_or(&1) as f32;
                    let count2 = *walk.revisit_counts.get(&key2).unwrap_or(&1) as f32;
                    let avg_count = (count1 + count2) * 0.5;

                    let radius = line_scale * (0.15 + 0.85 * avg_count.ln().max(0.0) / ln_max);

                    // three-d generated cylinder meshes extend along X from 0 to 1.
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
                renderables.push(instanced);
            }
        }

        // Points (spheres)
        {
            let mut instances = Instances::default();
            instances.transformations = Vec::new();
            instances.colors = Some(Vec::new());

            let max_revisits = walk.revisit_counts.values().max().copied().unwrap_or(1) as f32;
            let point_scale: f32 = 0.5;

            for (key, &count) in &walk.revisit_counts {
                let position = walk.point_positions.get(key).copied().unwrap_or([
                    key.0 as f32 / POSITION_KEY_SCALE,
                    key.1 as f32 / POSITION_KEY_SCALE,
                    key.2 as f32 / POSITION_KEY_SCALE,
                ]);
                let base_size = 0.8 * point_scale;
                let scale_factor =
                    1.0 + (count as f32).ln().max(0.0) / max_revisits.ln().max(1.0) * 2.0;
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

            if !instances.transformations.is_empty() {
                let sphere = CpuMesh::sphere(8);
                let instanced = Gm::new(
                    InstancedMesh::new(&context, &instances, &sphere),
                    ColorMaterial::default(),
                );
                renderables.push(instanced);
            }
        }

        // Clear and render
        frame_input
            .screen()
            .clear(ClearState::color_and_depth(0.08, 0.08, 0.12, 1.0, 1.0));

        for obj in &renderables {
            obj.render(&camera, &[]);
        }

        // Capture pixels and save
        let vp = frame_input.viewport;
        let pixels: Vec<[u8; 4]> = frame_input.screen().read_color();
        let flat: Vec<u8> = pixels
            .iter()
            .flat_map(|p| p.iter().copied())
            .collect();

        if let Some(img) = image::RgbaImage::from_raw(vp.width, vp.height, flat) {
            let out_path = output_dir.join(&filename);
            match img.save(&out_path) {
                Ok(()) => {
                    info!("Saved {}", out_path.display());
                    index_entries.push(serde_json::json!({
                        "id": source.id,
                        "name": source.name,
                        "category": source.category,
                        "subcategory": source.subcategory,
                        "file": filename,
                    }));
                }
                Err(e) => warn!("Failed to save {}: {}", out_path.display(), e),
            }
        }

        current_idx += 1;
        FrameOutput::default()
    });

    Ok(())
}

/// Auto-fit camera to see all points in the walk
fn auto_fit_camera(points: &[[f32; 3]], camera: &mut Camera) {
    if points.is_empty() {
        return;
    }

    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];

    for p in points {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }

    let center = vec3(
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    );

    let extent = vec3(max[0] - min[0], max[1] - min[1], max[2] - min[2]);
    let diag = extent.magnitude().max(1.0);

    // Position camera at 45-degree angle, far enough to see everything
    let distance = diag * 1.2;
    let offset = vec3(0.3, 0.4, 1.0).normalize() * distance;

    camera.set_view(center + offset, center, vec3(0.0, 1.0, 0.0));
}

/// Load walk data for a single source (same logic as gui.rs load_walk_data)
fn load_walk_for_thumbnail(
    source: &crate::config::Source,
    config: &Config,
    data_paths: &DataPaths,
) -> Option<WalkRender> {
    let base: u32 = 12; // Always use base-12 for thumbnails
    let max_points: usize = 5000;

    let digits = if source.converter.starts_with("math.") {
        MathGenerator::from_converter_string(&source.converter)?.generate(max_points)
    } else {
        match source.converter.as_str() {
            "audio" => {
                let path = data_paths.audio_file(&source.id)?;
                converters::load_audio_raw(&path, base).ok()?
            }
            "dna" => {
                let path = data_paths.dna_file(&source.url, &source.id);
                if !path.exists() {
                    return None;
                }
                converters::load_dna_raw(&path, base).ok()?
            }
            "cosmos" => {
                let path = data_paths.cosmos_file(&source.id);
                if !path.exists() {
                    return None;
                }
                converters::load_cosmos_raw(&path, base).ok()?
            }
            "finance" => {
                let path = data_paths.finance_file(&source.url, &source.id);
                if !path.exists() {
                    return None;
                }
                converters::load_finance_raw(&path, base).ok()?
            }
            "pdb_backbone" => {
                let path = data_paths.protein_file(&source.url, &source.id);
                if !path.exists() {
                    return None;
                }
                converters::load_pdb_backbone_raw(&path, base).ok()?
            }
            "pdb_sequence" => {
                let path = data_paths.protein_file(&source.url, &source.id);
                if !path.exists() {
                    return None;
                }
                converters::load_pdb_sequence_raw(&path, base).ok()?
            }
            "pdb_structure" => {
                let path = data_paths.protein_file(&source.url, &source.id);
                if !path.exists() {
                    return None;
                }
                let (points, _) = converters::load_pdb_structure(&path).ok()?;

                let n = points.len() as f32;
                if n == 0.0 {
                    return None;
                }

                let cx = points.iter().map(|p| p[0]).sum::<f32>() / n;
                let cy = points.iter().map(|p| p[1]).sum::<f32>() / n;
                let cz = points.iter().map(|p| p[2]).sum::<f32>() / n;
                let centered: Vec<[f32; 3]> = points
                    .iter()
                    .map(|p| [p[0] - cx, p[1] - cy, p[2] - cz])
                    .collect();

                let (revisit_counts, point_positions) = build_point_visit_maps(&centered);

                let hash = source
                    .id
                    .bytes()
                    .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
                let hue = (hash % 360) as f32 / 360.0;
                let color = hsv_to_rgb(hue, 0.7, 0.9);

                return Some(WalkRender {
                    points: centered,
                    color,
                    revisit_counts,
                    point_positions,
                });
            }
            _ => return None,
        }
    };

    // Get mapping from source's default
    let mapping = match config.get_mapping(&source.mapping) {
        Ok(mapping) => mapping,
        Err(e) => {
            warn!("Skipping {} due to invalid mapping '{}': {}", source.id, source.mapping, e);
            return None;
        }
    };

    let points = if base == 4 {
        walk_base4(&digits, max_points)
    } else {
        walk_base12(&digits, &mapping, max_points)
    };

    let (revisit_counts, point_positions) = build_point_visit_maps(&points);

    // Color from hash
    let hash = source
        .id
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    let hue = (hash % 360) as f32 / 360.0;
    let color = hsv_to_rgb(hue, 0.7, 0.9);

    Some(WalkRender {
        points,
        color,
        revisit_counts,
        point_positions,
    })
}

/// Check if raw data exists for a source
fn check_data_exists(id: &str, converter: &str, url: &str, data_paths: &DataPaths) -> bool {
    match converter {
        "audio" => data_paths.audio_file(id).is_some(),
        "dna" => data_paths.dna_file(url, id).exists(),
        "cosmos" => data_paths.cosmos_file(id).exists(),
        "finance" => data_paths.finance_file(url, id).exists(),
        "pdb_backbone" | "pdb_sequence" | "pdb_structure" => data_paths.protein_file(url, id).exists(),
        c if c.starts_with("math.") => true,
        _ => false,
    }
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
