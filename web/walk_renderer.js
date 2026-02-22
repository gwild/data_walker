/**
 * Client-side turtle walker and Three.js renderer.
 * Used by the gallery to re-render walks with any mapping.
 *
 * Requires: Three.js r128+ loaded as global THREE
 */

const NAMED_MAPPINGS = {
    'Optimal':   [0,1,2,3,4,5,6,7,10,9,8,11],
    'Spiral':    [0,2,4,6,8,10,1,3,5,7,9,11],
    'Identity':  [0,1,2,3,4,5,6,7,8,9,10,11],
    'LCG':       [3,7,11,10,4,0,9,6,5,1,2,8],
    'Stock-opt': [1,0,2,4,10,5,6,9,8,7,3,11],
};

const WALK_COLORS = [
    0xE91E63, 0x9C27B0, 0x2196F3, 0x4CAF50, 0xFFC107, 0xFF5722,
    0x00BCD4, 0x8BC34A, 0xFF9800, 0x673AB7, 0x3F51B5, 0xCDDC39
];

// ============================================================
// Turtle3D — matches Python scipy-based implementation exactly
// ============================================================
class Turtle3D {
    constructor() {
        this.px = 0; this.py = 0; this.pz = 0;
        this.qx = 0; this.qy = 0; this.qz = 0; this.qw = 1; // identity quaternion
    }

    // Rotate vector (vx,vy,vz) by quaternion (qx,qy,qz,qw)
    _applyQuat(vx, vy, vz) {
        const qx = this.qx, qy = this.qy, qz = this.qz, qw = this.qw;
        const ix = qw*vx + qy*vz - qz*vy;
        const iy = qw*vy + qz*vx - qx*vz;
        const iz = qw*vz + qx*vy - qy*vx;
        const iw = -qx*vx - qy*vy - qz*vz;
        return [
            ix*qw + iw*(-qx) + iy*(-qz) - iz*(-qy),
            iy*qw + iw*(-qy) + iz*(-qx) - ix*(-qz),
            iz*qw + iw*(-qz) + ix*(-qy) - iy*(-qx),
        ];
    }

    // Multiply: result = (ax,ay,az,aw) * (bx,by,bz,bw)
    _mulQuat(ax, ay, az, aw, bx, by, bz, bw) {
        return [
            aw*bx + ax*bw + ay*bz - az*by,
            aw*by - ax*bz + ay*bw + az*bx,
            aw*bz + ax*by - ay*bx + az*bw,
            aw*bw - ax*bx - ay*by - az*bz,
        ];
    }

    move(raw, mapping) {
        const d = mapping[raw % 12];
        if (d < 6) {
            const DIRS = [[1,0,0],[-1,0,0],[0,1,0],[0,-1,0],[0,0,1],[0,0,-1]];
            const [wx, wy, wz] = this._applyQuat(DIRS[d][0], DIRS[d][1], DIRS[d][2]);
            this.px += wx; this.py += wy; this.pz += wz;
        } else {
            const AXES = [[1,0,0],[-1,0,0],[0,1,0],[0,-1,0],[0,0,1],[0,0,-1]];
            const ax = AXES[d-6];
            const half = (15 * Math.PI / 180) / 2;
            const s = Math.sin(half), c = Math.cos(half);
            const [rx, ry, rz, rw] = this._mulQuat(
                ax[0]*s, ax[1]*s, ax[2]*s, c,
                this.qx, this.qy, this.qz, this.qw
            );
            this.qx = rx; this.qy = ry; this.qz = rz; this.qw = rw;
        }
    }
}

/**
 * Walk a base-12 sequence with a given mapping.
 * @param {number[]} base12 - array of ints 0-11
 * @param {number[]} mapping - permutation array length 12
 * @param {number} [step=1] - downsample: walk every step'th value (matches Python generator)
 * @returns {Float32Array} - flat xyz array
 */
function walkSequence(base12, mapping, step) {
    const s = step || 1;
    const nWalked = Math.ceil(base12.length / s);
    const out = new Float32Array((nWalked + 1) * 3);
    const t = new Turtle3D();
    out[0] = 0; out[1] = 0; out[2] = 0;
    let idx = 0;
    for (let i = 0; i < base12.length; i += s) {
        t.move(base12[i], mapping);
        idx++;
        const j = idx * 3;
        out[j] = t.px; out[j+1] = t.py; out[j+2] = t.pz;
    }
    return out.subarray(0, (idx + 1) * 3);
}

// ============================================================
// Off-screen Three.js renderer
// ============================================================
let _glRenderer = null, _glScene = null, _glCamera = null;

function _initGL(size) {
    if (_glRenderer && _glRenderer._sz === size) return;
    if (_glRenderer) _glRenderer.dispose();
    _glRenderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
    _glRenderer.setSize(size, size);
    _glRenderer.setClearColor(0x0a0a12);
    _glRenderer._sz = size;
    _glScene = new THREE.Scene();
    _glCamera = new THREE.PerspectiveCamera(50, 1, 0.1, 1000000);
}

function _clearGL() {
    while (_glScene.children.length > 0) {
        const o = _glScene.children[0];
        if (o.geometry) o.geometry.dispose();
        if (o.material) o.material.dispose();
        _glScene.remove(o);
    }
}

/**
 * Render walk points to a PNG data URI.
 * @param {Float32Array} positions - flat xyz from walkSequence
 * @param {number} colorIdx
 * @param {number} size - canvas pixels (default 200)
 * @returns {string} data URI
 */
function renderWalkToDataURI(positions, colorIdx, size) {
    size = size || 200;
    const nPts = positions.length / 3;
    if (nPts < 2) return null;

    _initGL(size);
    _clearGL();

    const color = WALK_COLORS[(colorIdx || 0) % WALK_COLORS.length];

    // Bounding box
    let minX=Infinity, maxX=-Infinity, minY=Infinity, maxY=-Infinity, minZ=Infinity, maxZ=-Infinity;
    for (let i = 0; i < positions.length; i += 3) {
        const x = positions[i], y = positions[i+1], z = positions[i+2];
        if (x < minX) minX = x; if (x > maxX) maxX = x;
        if (y < minY) minY = y; if (y > maxY) maxY = y;
        if (z < minZ) minZ = z; if (z > maxZ) maxZ = z;
    }

    // Line
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    _glScene.add(new THREE.Line(geo, new THREE.LineBasicMaterial({ color })));

    // Markers
    const rawRange = Math.max(maxX-minX, maxY-minY, maxZ-minZ);
    const isDegenerate = rawRange < 0.01;
    const sphereSize = isDegenerate ? 0.15 : Math.max(0.5, rawRange / 80);

    const startGeo = new THREE.SphereGeometry(sphereSize);
    const startMesh = new THREE.Mesh(startGeo, new THREE.MeshBasicMaterial({ color: 0x4CAF50 }));
    startMesh.position.set(positions[0], positions[1], positions[2]);
    _glScene.add(startMesh);

    const endIdx = (nPts - 1) * 3;
    const endGeo = new THREE.SphereGeometry(sphereSize);
    const endMesh = new THREE.Mesh(endGeo, new THREE.MeshBasicMaterial({ color: 0xE91E63 }));
    endMesh.position.set(positions[endIdx], positions[endIdx+1], positions[endIdx+2]);
    _glScene.add(endMesh);

    // Camera
    const cx = (minX+maxX)/2, cy = (minY+maxY)/2, cz = (minZ+maxZ)/2;
    const dx = (maxX-minX)||0.01, dy = (maxY-minY)||0.01, dz = (maxZ-minZ)||0.01;
    const bboxR = Math.sqrt(dx*dx + dy*dy + dz*dz) / 2;
    const effR = isDegenerate ? 1 : bboxR;
    const fovRad = _glCamera.fov * Math.PI / 180;
    const camDist = (effR / Math.sin(fovRad / 2)) * 0.85;

    _glCamera.position.set(cx + camDist*0.58, cy + camDist*0.44, cz + camDist*0.58);
    _glCamera.lookAt(cx, cy, cz);

    _glRenderer.render(_glScene, _glCamera);
    return _glRenderer.domElement.toDataURL('image/png');
}

/**
 * Batch re-render all walks with a new mapping.
 * @param {Object} base12Index - { "file::walk": [base12 array], ... }
 * @param {number[]} mapping - permutation array
 * @param {Function} onProgress - callback(current, total)
 * @param {Object} [pointCounts] - { "file::walk": originalPointCount } for downsampling
 * @returns {Object} - { "file::walk": dataURI, ... }
 */
async function batchRender(base12Index, mapping, onProgress, pointCounts) {
    const keys = Object.keys(base12Index);
    const result = {};
    for (let i = 0; i < keys.length; i++) {
        const key = keys[i];
        const b12 = base12Index[key];
        if (!b12 || b12.length < 2) continue;
        // Compute downsampling step from original point count
        let step = 1;
        if (pointCounts && pointCounts[key] > 1 && b12.length > pointCounts[key]) {
            step = Math.round(b12.length / (pointCounts[key] - 1));
        }
        const positions = walkSequence(b12, mapping, step);
        result[key] = renderWalkToDataURI(positions, i, 200);
        if (onProgress) onProgress(i + 1, keys.length);
        if (i % 10 === 9) await new Promise(r => setTimeout(r, 0));
    }
    return result;
}
