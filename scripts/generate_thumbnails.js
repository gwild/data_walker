#!/usr/bin/env node
/**
 * Generate individual walk thumbnails using Puppeteer.
 * Creates one thumbnail per walk (not per file).
 */

const puppeteer = require('puppeteer');
const path = require('path');
const fs = require('fs');

const BASE_URL = 'http://localhost:8081/visualizations';
const DATA_DIR = path.join(__dirname, '..', 'visualizations', 'data');
const THUMB_DIR = path.join(__dirname, '..', 'visualizations', 'thumbnails');

// Normalize points: support both [x,y,z] arrays and {x,y,z} objects
function normalizePoints(points) {
    return points.map(pt => {
        if (Array.isArray(pt)) return pt;
        if (pt && typeof pt === 'object') return [pt.x || 0, pt.y || 0, pt.z || 0];
        return [0, 0, 0];
    });
}

async function main() {
    console.log('GENERATING INDIVIDUAL WALK THUMBNAILS');
    console.log('='.repeat(50));

    // Parse CLI flags
    const onlyArg = process.argv.find(a => a.startsWith('--only='));
    const onlyFiles = onlyArg ? onlyArg.split('=')[1].split(',') : null;
    const mergeManifest = process.argv.includes('--merge');

    if (onlyFiles) console.log(`Filtering to: ${onlyFiles.join(', ')}`);
    if (mergeManifest) console.log('Merge mode: will merge into existing manifest');

    // Ensure thumbnails directory exists
    if (!fs.existsSync(THUMB_DIR)) {
        fs.mkdirSync(THUMB_DIR, { recursive: true });
    }

    // Get all JS data files
    const dataFiles = fs.readdirSync(DATA_DIR)
        .filter(f => f.endsWith('.js'))
        .filter(f => !onlyFiles || onlyFiles.some(o => f.includes(o)))
        .sort();

    console.log(`Found ${dataFiles.length} data files\n`);

    const browser = await puppeteer.launch({
        headless: 'new',
        args: ['--no-sandbox', '--disable-setuid-sandbox']
    });

    // Load the renderer page
    const page = await browser.newPage();
    await page.setViewport({ width: 400, height: 400 });
    await page.goto(`${BASE_URL}/thumb_renderer.html`, { waitUntil: 'networkidle0' });
    await new Promise(r => setTimeout(r, 500));

    let totalGenerated = 0;
    const manifest = [];

    for (let fi = 0; fi < dataFiles.length; fi++) {
        const dataFile = dataFiles[fi];
        const dataPath = path.join(DATA_DIR, dataFile);
        const baseName = dataFile.replace('.js', '');

        process.stdout.write(`[${fi + 1}/${dataFiles.length}] ${baseName}: `);

        // Read and parse the data file
        let content = fs.readFileSync(dataPath, 'utf-8');

        // Extract the data object - find const XXX = { ... }
        const match = content.match(/const\s+(\w+)\s*=\s*(\{[\s\S]*\});?\s*$/m);
        if (!match) {
            console.log('skip (no data object)');
            continue;
        }

        let data;
        try {
            // Clean up for JSON parsing
            let jsonStr = match[2];
            jsonStr = jsonStr.replace(/,(\s*[}\]])/g, '$1');
            // Quote unquoted JS object keys (e.g. { walks: ... } -> { "walks": ... })
            jsonStr = jsonStr.replace(/(?<=[{,]\s*)([a-zA-Z_]\w*)(?=\s*:)/g, '"$1"');
            data = JSON.parse(jsonStr);
        } catch (e) {
            console.log(`skip (parse error: ${e.message.substring(0, 60)})`);
            continue;
        }

        // Extract walks from data
        const walks = [];
        function extractWalks(obj, prefix) {
            for (const [key, value] of Object.entries(obj)) {
                if (!value || typeof value !== 'object') continue;
                const name = prefix ? `${prefix} - ${key}` : key;
                // points array
                if (value.points && Array.isArray(value.points) && value.points.length > 1) {
                    walks.push({ name, points: value.points });
                }
                // path array (e.g. covid_walks_best, dna_pcpri_comparison)
                else if (value.path && Array.isArray(value.path) && value.path.length > 1) {
                    walks.push({ name, points: value.path });
                }
                // raw array of points
                else if (Array.isArray(value) && value.length > 1 && (Array.isArray(value[0]) || (value[0] && typeof value[0] === 'object'))) {
                    walks.push({ name, points: value });
                }
                // nested walks object (e.g. { walks: { Wuhan: { path: [...] } } })
                if (key === 'walks' && typeof value === 'object' && !Array.isArray(value)) {
                    extractWalks(value, prefix);
                }
            }
        }
        extractWalks(data, '');

        if (walks.length === 0) {
            console.log('skip (no walks found)');
            continue;
        }

        console.log(`${walks.length} walks`);

        // Render each walk
        for (let wi = 0; wi < walks.length; wi++) {
            const walk = walks[wi];
            const safeName = walk.name.replace(/[^a-zA-Z0-9_-]/g, '_').substring(0, 50);
            const thumbName = `${baseName}__${safeName}`;
            const thumbPath = path.join(THUMB_DIR, `${thumbName}.png`);

            // Normalize and downsample large walks
            let pts = normalizePoints(walk.points);
            if (pts.length > 5000) {
                const step = Math.ceil(pts.length / 5000);
                pts = pts.filter((_, i) => i % step === 0);
            }

            try {
                // Render in browser
                const dataUrl = await page.evaluate((points, colorIdx) => {
                    return window.renderWalk(points, colorIdx);
                }, pts, wi);

                if (dataUrl) {
                    // Save PNG
                    const base64 = dataUrl.replace(/^data:image\/png;base64,/, '');
                    fs.writeFileSync(thumbPath, Buffer.from(base64, 'base64'));

                    manifest.push({
                        file: dataFile,
                        walk: walk.name,
                        thumb: `${thumbName}.png`
                    });
                    totalGenerated++;
                    process.stdout.write('.');
                }
            } catch (e) {
                process.stdout.write('x');
            }
        }
        console.log();
    }

    await browser.close();

    // Save manifest (merge mode preserves entries for files we didn't regenerate)
    const manifestPath = path.join(THUMB_DIR, 'manifest.json');
    if (mergeManifest && fs.existsSync(manifestPath)) {
        const existing = JSON.parse(fs.readFileSync(manifestPath, 'utf-8'));
        const regeneratedFiles = new Set(manifest.map(m => m.file));
        const kept = existing.filter(m => !regeneratedFiles.has(m.file));
        const merged = [...kept, ...manifest];
        fs.writeFileSync(manifestPath, JSON.stringify(merged, null, 2));
        console.log(`Merged: kept ${kept.length} existing + ${manifest.length} new = ${merged.length} total`);
    } else {
        fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2));
    }

    console.log('='.repeat(50));
    console.log(`Generated: ${totalGenerated} thumbnails`);
    console.log(`Manifest: ${manifestPath}`);
    console.log(`Output: ${THUMB_DIR}`);
}

main().catch(console.error);
