// Kohbor website — the stage.
// One continuous 3D shot shared by the page (scroll-driven) and the film (time-driven).
// Deterministic: every frame is a pure function of the params handed to render().
// The field is an original, procedural MOBA-style battlefield (two corner bases on high ground,
// three lanes, a diagonal river, jungle between the lanes), drawn from code only: no game image,
// map file or asset is used, and positions are not measured from any game.
import * as THREE from '../vendor/three.module.min.js';

export const R = 5;          // half-size of the map square in world units
export const SEA = 0.15;     // height threshold: above = land

// Battlefield layout in grid space (-1..1); x to the right, z toward the viewer.
// Home base bottom-left, enemy base top-right, river from top-left to bottom-right.
// The same field is evaluated in GLSL (points) and in JS (marker heights); keep them in step.
export const BASES = [[-0.80, 0.80], [0.80, -0.80]];
export const LANES = [
  [[-0.80, 0.60], [-0.80, -0.80]], [[-0.80, -0.80], [0.60, -0.80]],   // top lane
  [[-0.60, 0.80], [0.80, 0.80]], [[0.80, 0.80], [0.80, -0.60]],       // bottom lane
  [[-0.64, 0.64], [0.64, -0.64]],                                     // middle lane
];

const sat = (x) => Math.min(1, Math.max(0, x));
const smooth = (a, b, x) => { const t = sat((x - a) / (b - a)); return t * t * (3 - 2 * t); };
function segDist(px, pz, [ax, az], [bx, bz]) {
  const vx = bx - ax, vz = bz - az, wx = px - ax, wz = pz - az;
  const t = sat((wx * vx + wz * vz) / (vx * vx + vz * vz));
  return Math.hypot(wx - vx * t, wz - vz * t);
}
export function laneDist(gx, gz) {
  return Math.min(...LANES.map(([a, b]) => segDist(gx, gz, a, b)));
}
export function riverDist(gx, gz) {
  return Math.abs(gz - gx - 0.10 * Math.sin((gx + gz) * 2.4)) / Math.SQRT2;
}
// Height without the jungle canopy (markers stand on lanes and bases, where there is none).
export function heightAt(gx, gz) {
  let h = 0.30;
  for (const [bx, bz] of BASES) {
    const d = Math.hypot(gx - bx, gz - bz);
    h += 0.30 * smooth(0.56, 0.36, d) + 0.55 * Math.exp(-(d * d) / 0.004);
  }
  h -= 0.24 * Math.exp(-((riverDist(gx, gz) / 0.075) ** 2));
  return h * smooth(1.0, 0.94, Math.max(Math.abs(gx), Math.abs(gz)));
}

// Sample markers, placed on lanes. Shapes follow the Command Deck rule: ally = ring, enemy = diamond, own = double ring.
export const MARKERS = [
  { id: 'own', kind: 'own', g: [-0.24, 0.24] },
  { id: 'A1', kind: 'ally', g: [-0.80, -0.10] },
  { id: 'A2', kind: 'ally', g: [0.20, 0.80] },
  { id: 'E1', kind: 'enemy', g: [0.14, -0.14] },
  { id: 'E2', kind: 'enemy', g: [-0.30, -0.80] },
  { id: 'E3', kind: 'enemy', g: [0.80, 0.18] },
  { id: 'E4', kind: 'enemy', g: [0.50, -0.80] },
];

const hex = (h) => new THREE.Color(h);
// Colours a room gives to what is drawn on it. c = Charcoal, b = Bone, r = Barber Red.
export const ROOMS = {
  c: { paper: '#141414', ink: hex('#ede6d8'), tint: hex('#d4453a'), own: hex('#ede6d8'), ally: hex('#7fb0e6'), enemy: hex('#f0914f'), missing: hex('#e9b44c'), gain: 1.0 },
  b: { paper: '#ede6d8', ink: hex('#1c1c1c'), tint: hex('#b3261e'), own: hex('#1c1c1c'), ally: hex('#1f5a9e'), enemy: hex('#96470a'), missing: hex('#7a5200'), gain: 1.25 },
  r: { paper: '#b3261e', ink: hex('#ede6d8'), tint: hex('#ede6d8'), own: hex('#ede6d8'), ally: hex('#ede6d8'), enemy: hex('#ede6d8'), missing: hex('#ede6d8'), gain: 1.1 },
};

const SPLIT = /* glsl */`
  uniform float uSplit;
  vec3 room(vec3 top, vec3 bot) { return gl_FragCoord.y > uSplit ? top : bot; }
  float roomf(float top, float bot) { return gl_FragCoord.y > uSplit ? top : bot; }
`;

const POINT_VERT = /* glsl */`
  uniform float uTime, uShape, uH, uPx, uSize, uIntro, uStale, uLost, uDim, uVoice, uOrbY, uGridN;
  uniform vec3 uAmp;
  uniform vec3 uPointer;            // local x, local z, strength
  attribute vec2 aGrid;
  attribute vec4 aRand;
  varying float vAlpha;
  varying float vTint;
  const float R = ${R.toFixed(1)};
  const float PI = 3.14159265;

  float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
  float vnoise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1, 0)), f.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), f.x), f.y);
  }
  // Same battlefield as BASES / LANES / heightAt() in JS.
  float segDist(vec2 p, vec2 a, vec2 b) {
    vec2 pa = p - a, ba = b - a;
    return length(pa - ba * clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0));
  }
  float laneDist(vec2 g) {
    float d = segDist(g, vec2(-0.80, 0.60), vec2(-0.80, -0.80));
    d = min(d, segDist(g, vec2(-0.80, -0.80), vec2(0.60, -0.80)));
    d = min(d, segDist(g, vec2(-0.60, 0.80), vec2(0.80, 0.80)));
    d = min(d, segDist(g, vec2(0.80, 0.80), vec2(0.80, -0.60)));
    return min(d, segDist(g, vec2(-0.64, 0.64), vec2(0.64, -0.64)));
  }
  float riverDist(vec2 g) { return abs(g.y - g.x - 0.10 * sin((g.x + g.y) * 2.4)) * 0.70710678; }
  float baseField(vec2 g) {
    float h = 0.0;
    vec2 b0 = vec2(-0.80, 0.80), b1 = vec2(0.80, -0.80);
    float d0 = length(g - b0), d1 = length(g - b1);
    h += 0.30 * smoothstep(0.56, 0.36, d0) + 0.55 * exp(-d0 * d0 / 0.004);
    h += 0.30 * smoothstep(0.56, 0.36, d1) + 0.55 * exp(-d1 * d1 / 0.004);
    return h;
  }

  void main() {
    vec2 g = aGrid;
    float lane = laneDist(g), river = riverDist(g);
    float bases = baseField(g);
    float edge = smoothstep(1.0, 0.94, max(abs(g.x), abs(g.y)));
    // jungle canopy: clumps of trees away from lanes, river and bases
    float clear = smoothstep(0.05, 0.11, lane) * smoothstep(0.09, 0.16, river) * (1.0 - smoothstep(0.02, 0.12, bases));
    float trees = smoothstep(0.48, 0.78, vnoise(g * 13.0)) * clear;
    float h = (0.30 + bases + trees * 0.20 - 0.24 * exp(-pow(river / 0.075, 2.0))) * edge
            + (vnoise(g * 9.0) - 0.5) * 0.02;
    float land = smoothstep(${(SEA - 0.02).toFixed(2)}, ${(SEA + 0.02).toFixed(2)}, h);
    float elev = max(h - ${SEA.toFixed(2)}, 0.0);
    float cl = abs(fract(h * 9.0 + 0.5) - 0.5);
    float contour = (1.0 - smoothstep(0.03, 0.09, cl)) * land;
    float laneLine = (1.0 - smoothstep(0.016, 0.04, lane)) * land;
    float seaKeep = step(0.55, aRand.w);

    // 0 battlefield relief
    vec3 p0 = vec3(g.x * R, elev * uH * 1.7 + (1.0 - land) * sin(g.x * 9.0 + g.y * 7.0 + uTime * 0.6) * 0.025, g.y * R);
    float a0 = mix(0.16 * seaKeep, 0.40, land) + contour * 0.35 + laneLine * 0.6 + trees * 0.12;
    // 1 flat map
    vec3 p1 = vec3(g.x * R, elev * 0.08, g.y * R);
    float a1 = mix(0.13 * seaKeep, 0.30, land) + contour * 0.45 + laneLine * 0.75;
    // 2 rings: three bands, one per mixer channel
    float ringT = g.y * 0.5 + 0.5;
    float ringI = floor(ringT * 30.0);
    float rowF = fract(ringT * 30.0);
    float band = ringI / 30.0;
    float rr = 0.7 + band * 4.4;
    float th = (g.x + rowF * 2.0 / uGridN) * PI + ringI * 0.21;
    float w0 = 1.0 - smoothstep(0.26, 0.40, band);
    float w2 = smoothstep(0.60, 0.74, band);
    float w1 = 1.0 - w0 - w2;
    float wave = w0 * uAmp.x * sin(th * 3.0 + uTime * 2.2 + ringI * 0.5) * 0.34
               + w1 * uAmp.y * sin(th * 7.0 - uTime * 3.1 + ringI * 0.9) * 0.26 * (0.6 + 0.4 * sin(uTime * 5.0 + ringI))
               + w2 * uAmp.z * sin(th * 2.0 + uTime * 1.2 + ringI * 0.3) * 0.42;
    vec3 p2 = vec3(cos(th) * rr, wave, sin(th) * rr);
    float lvl = w0 * uAmp.x + w1 * uAmp.y + w2 * uAmp.z;
    float a2 = 0.16 + 0.5 * lvl;
    // 3 voice sphere
    float lon = g.x * PI;
    float sy = clamp(g.y, -1.0, 1.0);
    float sr = sqrt(1.0 - sy * sy);
    vec3 dir = vec3(sr * cos(lon), sy, sr * sin(lon));
    float n = vnoise(dir.xy * 3.0 + dir.z * 2.0 + uTime * 0.8);
    float rad = 1.2 + 0.03 * sin(uTime * 1.3 + dir.x * 4.0) + uVoice * (0.34 * (n - 0.35) + 0.10 * sin(dir.y * 14.0 - uTime * 9.0));
    vec3 p3 = vec3(0.0, uOrbY, 0.0) + dir * rad;
    float a3 = 0.5 + 0.25 * uVoice;
    // 4 calm sea
    vec2 sg = g * 2.6;
    vec3 p4 = vec3(sg.x * R, sin(sg.x * 3.0 + uTime * 0.5) * 0.10 + sin(sg.y * 4.0 - uTime * 0.37) * 0.08 - 0.4, sg.y * R);
    float a4 = 0.30 * step(0.35, aRand.w);

    float s = clamp(uShape, 0.0, 5.0);
    float si = floor(min(s, 4.999));
    float sf = s - si;
    float st = smoothstep(0.0, 1.0, clamp((sf - aRand.x * 0.35) / 0.65, 0.0, 1.0));
    vec3 pa = si < 0.5 ? p0 : si < 1.5 ? p1 : si < 2.5 ? p2 : si < 3.5 ? p3 : p4;
    vec3 pb = si < 0.5 ? p1 : si < 1.5 ? p2 : si < 2.5 ? p3 : si < 3.5 ? p4 : p0;
    float aa = si < 0.5 ? a0 : si < 1.5 ? a1 : si < 2.5 ? a2 : si < 3.5 ? a3 : a4;
    float ab = si < 0.5 ? a1 : si < 1.5 ? a2 : si < 2.5 ? a3 : si < 3.5 ? a4 : a0;
    vec3 p = mix(pa, pb, st);
    p.y += sin(st * PI) * (aRand.y - 0.3) * 0.9;            // points travel on arcs while morphing
    float alpha = mix(aa, ab, st);

    float ringsW = si < 0.5 ? 0.0 : si < 1.5 ? st : si < 2.5 ? 1.0 - st : 0.0;
    vTint = ringsW * w1 * uAmp.y * 0.85;

    // opening: the field assembles out of scatter
    vec3 scatter = vec3((aRand.x - 0.5) * 34.0, aRand.y * 14.0 - 3.0, (aRand.z - 0.5) * 34.0);
    float it = smoothstep(0.0, 1.0, clamp(uIntro * 1.4 - aRand.w * 0.4, 0.0, 1.0));
    p = mix(scatter, p, it);
    alpha *= mix(0.25, 1.0, it);

    // the pointer leaves a ripple
    float pd = distance(p.xz, uPointer.xy);
    float rip = exp(-pd * pd * 1.3) * uPointer.z;
    p.y += rip * 0.26 * (0.55 + 0.45 * sin(pd * 8.0 - uTime * 5.0));
    alpha += rip * 0.35;

    // honest states: stale = dimmed and hatched, lost = almost nothing left
    float hatch = step(0.5, fract((g.x + g.y) * 11.0));
    alpha *= mix(1.0, mix(0.07, 0.40, hatch), uStale);
    alpha *= 1.0 - uLost * 0.95;
    vAlpha = alpha * uDim;

    vec4 mv = modelViewMatrix * vec4(p, 1.0);
    gl_Position = projectionMatrix * mv;
    gl_PointSize = uSize * (0.65 + aRand.z * 0.7) * (1.0 + contour * 0.35) * uPx * (9.0 / -mv.z);
  }
`;

const POINT_FRAG = /* glsl */`
  precision highp float;
  uniform vec3 uInkTop, uInkBot, uTintTop, uTintBot;
  uniform float uGainTop, uGainBot;
  varying float vAlpha;
  varying float vTint;
  ${SPLIT}
  void main() {
    float d = length(gl_PointCoord - 0.5);
    float a = smoothstep(0.5, 0.22, d) * vAlpha * roomf(uGainTop, uGainBot);
    if (a < 0.008) discard;
    vec3 col = mix(room(uInkTop, uInkBot), room(uTintTop, uTintBot), clamp(vTint, 0.0, 1.0));
    gl_FragColor = vec4(col * a, a);
  }
`;

const QUAD_VERT = /* glsl */`
  varying vec2 vUv;
  void main() { vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }
`;

// One fragment shader draws every marker: ring or diamond, solid or dashed, single or double.
const MARK_FRAG = /* glsl */`
  precision highp float;
  uniform vec3 uColTop, uColBot;
  uniform float uKind, uDash, uDouble, uDot, uPing, uAlpha;
  varying vec2 vUv;
  ${SPLIT}
  void main() {
    vec2 p = (vUv - 0.5) * 2.0;
    float ang = atan(p.y, p.x);
    float dC = length(p);
    float dD = (abs(p.x) + abs(p.y)) * 0.80;
    float d = mix(dC, dD, uKind);
    float aa = fwidth(d) * 1.3;
    float r0 = 0.44, w = 0.050;
    float ring = smoothstep(w + aa, w, abs(d - r0));
    float dash = mix(1.0, step(0.42, fract(ang / 6.2831853 * 12.0 + 0.125)), uDash);
    ring *= dash;
    float ring2 = uDouble * smoothstep(w * 0.75 + aa, w * 0.75, abs(d - (r0 + 0.19)));
    float dotc = uDot * smoothstep(0.10 + aa, 0.10, dC);
    float pr = mix(0.20, 0.96, uPing);
    float ping = smoothstep(0.035 + aa, 0.035, abs(dC - pr)) * (1.0 - uPing) * step(0.001, uPing);
    float fill = smoothstep(r0, r0 - aa, d) * 0.10 * (1.0 - uDash);
    float a = max(max(ring, ring2), max(dotc, max(ping, fill))) * uAlpha;
    if (a < 0.004) discard;
    gl_FragColor = vec4(room(uColTop, uColBot) * a, a);
  }
`;

// The map frame: solid when live, dashed when the picture is stale.
const FRAME_FRAG = /* glsl */`
  precision highp float;
  uniform vec3 uColTop, uColBot;
  uniform float uDash, uAlpha;
  varying vec2 vUv;
  ${SPLIT}
  void main() {
    vec2 q = abs(vUv - 0.5) * 2.0;
    float d = max(q.x, q.y);
    float aa = fwidth(d) * 1.2;
    float w = 0.0045;
    float line = smoothstep(w + aa, w, abs(d - 0.985));
    float t = q.x > q.y ? vUv.y : vUv.x;
    float dash = mix(1.0, step(0.45, fract(t * 26.0)), uDash);
    // corner ticks, like registration marks on a printed map
    float tick = smoothstep(w + aa, w, abs(d - 0.945)) * step(0.90, min(q.x, q.y));
    float a = max(line * dash, tick * 0.6) * uAlpha;
    if (a < 0.004) discard;
    gl_FragColor = vec4(room(uColTop, uColBot) * a, a);
  }
`;

const ORB_VERT = /* glsl */`
  varying vec3 vN; varying vec3 vV;
  void main() {
    vec4 mv = modelViewMatrix * vec4(position, 1.0);
    vN = normalize(normalMatrix * normal);
    vV = normalize(-mv.xyz);
    gl_Position = projectionMatrix * mv;
  }
`;
const ORB_FRAG = /* glsl */`
  precision highp float;
  uniform vec3 uCoreTop, uCoreBot, uRimTop, uRimBot;
  uniform float uAlpha;
  varying vec3 vN; varying vec3 vV;
  ${SPLIT}
  void main() {
    float f = pow(1.0 - max(dot(normalize(vN), normalize(vV)), 0.0), 1.6);
    vec3 col = mix(room(uCoreTop, uCoreBot), room(uRimTop, uRimBot), smoothstep(0.35, 0.95, f));
    gl_FragColor = vec4(col * uAlpha, uAlpha);
  }
`;
const HALO_FRAG = /* glsl */`
  precision highp float;
  uniform vec3 uColTop, uColBot;
  uniform float uAlpha, uHaloTop, uHaloBot;
  varying vec2 vUv;
  ${SPLIT}
  void main() {
    float d = length(vUv - 0.5) * 2.0;
    float a = pow(max(1.0 - d, 0.0), 2.4) * 0.34 * uAlpha * roomf(uHaloTop, uHaloBot);
    gl_FragColor = vec4(room(uColTop, uColBot) * a, a);
  }
`;

const premult = { transparent: true, depthTest: false, depthWrite: false, blending: THREE.CustomBlending, blendSrc: THREE.OneFactor, blendDst: THREE.OneMinusSrcAlphaFactor, blendSrcAlpha: THREE.OneFactor, blendDstAlpha: THREE.OneMinusSrcAlphaFactor };

export function createStage(canvas, { density = 256, antialias = true } = {}) {
  const renderer = new THREE.WebGLRenderer({ canvas, antialias, alpha: true, premultipliedAlpha: true, powerPreference: 'high-performance' });
  renderer.setClearColor(0x000000, 0);
  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(35, 1, 0.1, 300);
  const world = new THREE.Group();
  scene.add(world);
  const size = { w: 1, h: 1, dpr: 1 };

  // ---- the point field -----------------------------------------------------
  const N = density, count = N * N;
  const grid = new Float32Array(count * 2), rand = new Float32Array(count * 4);
  let seed = 20261121;
  const rnd = () => ((seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) / 4294967296);
  for (let j = 0, k = 0; j < N; j++) {
    for (let i = 0; i < N; i++, k++) {
      grid[k * 2] = ((i + 0.5 + (rnd() - 0.5) * 0.3) / N) * 2 - 1;
      grid[k * 2 + 1] = ((j + 0.5 + (rnd() - 0.5) * 0.3) / N) * 2 - 1;
      rand[k * 4] = rnd(); rand[k * 4 + 1] = rnd(); rand[k * 4 + 2] = rnd(); rand[k * 4 + 3] = rnd();
    }
  }
  const pg = new THREE.BufferGeometry();
  pg.setAttribute('position', new THREE.BufferAttribute(new Float32Array(count * 3), 3));
  pg.setAttribute('aGrid', new THREE.BufferAttribute(grid, 2));
  pg.setAttribute('aRand', new THREE.BufferAttribute(rand, 4));
  const split = { value: -1e6 };
  const pu = {
    uTime: { value: 0 }, uShape: { value: 0 }, uH: { value: 1 }, uPx: { value: 1 }, uSize: { value: 1 }, uIntro: { value: 1 },
    uStale: { value: 0 }, uLost: { value: 0 }, uDim: { value: 1 }, uVoice: { value: 0 }, uOrbY: { value: 1.7 }, uGridN: { value: N },
    uAmp: { value: new THREE.Vector3(0.5, 0.5, 0.5) }, uPointer: { value: new THREE.Vector3(0, 0, 0) },
    uInkTop: { value: new THREE.Color() }, uInkBot: { value: new THREE.Color() },
    uTintTop: { value: new THREE.Color() }, uTintBot: { value: new THREE.Color() },
    uGainTop: { value: 1 }, uGainBot: { value: 1 }, uSplit: split,
  };
  const points = new THREE.Points(pg, new THREE.ShaderMaterial({ uniforms: pu, vertexShader: POINT_VERT, fragmentShader: POINT_FRAG, ...premult }));
  points.frustumCulled = false;
  world.add(points);

  // ---- map frame and markers ----------------------------------------------
  const quad = new THREE.PlaneGeometry(1, 1);
  const frameU = { uColTop: { value: new THREE.Color() }, uColBot: { value: new THREE.Color() }, uDash: { value: 0 }, uAlpha: { value: 0 }, uSplit: split };
  const frame = new THREE.Mesh(quad, new THREE.ShaderMaterial({ uniforms: frameU, vertexShader: QUAD_VERT, fragmentShader: FRAME_FRAG, ...premult }));
  frame.rotation.x = -Math.PI / 2;
  frame.scale.setScalar(R * 2 + 0.5);
  frame.position.y = 0.01;
  frame.renderOrder = 1;
  world.add(frame);

  const marks = MARKERS.map((m, i) => {
    const u = {
      uColTop: { value: new THREE.Color() }, uColBot: { value: new THREE.Color() },
      uKind: { value: m.kind === 'enemy' ? 1 : 0 }, uDash: { value: 0 }, uDouble: { value: m.kind === 'own' ? 1 : 0 },
      uDot: { value: 1 }, uPing: { value: 0 }, uAlpha: { value: 0 }, uSplit: split,
    };
    const mesh = new THREE.Mesh(quad, new THREE.ShaderMaterial({ uniforms: u, vertexShader: QUAD_VERT, fragmentShader: MARK_FRAG, ...premult }));
    mesh.rotation.x = -Math.PI / 2;
    mesh.scale.setScalar(m.kind === 'own' ? 0.86 : m.kind === 'ally' ? 0.6 : 0.74);
    mesh.renderOrder = 2 + i;
    world.add(mesh);
    return { ...m, u, mesh, elev: Math.max(heightAt(m.g[0], m.g[1]) - SEA, 0) };
  });

  // ---- the companion: a small orb with a lit-red ribbon -------------------
  const orb = new THREE.Group();
  const orbU = { uCoreTop: { value: new THREE.Color() }, uCoreBot: { value: new THREE.Color() }, uRimTop: { value: new THREE.Color() }, uRimBot: { value: new THREE.Color() }, uAlpha: { value: 1 }, uSplit: split };
  const ball = new THREE.Mesh(new THREE.SphereGeometry(0.15, 48, 32), new THREE.ShaderMaterial({ uniforms: orbU, vertexShader: ORB_VERT, fragmentShader: ORB_FRAG, transparent: true, depthTest: false, depthWrite: false, blending: THREE.CustomBlending, blendSrc: THREE.OneFactor, blendDst: THREE.OneMinusSrcAlphaFactor }));
  const ribbonU = { uColTop: { value: new THREE.Color() }, uColBot: { value: new THREE.Color() }, uAlpha: { value: 1 }, uSplit: split };
  const ribbon = new THREE.Mesh(new THREE.TorusGeometry(0.27, 0.007, 8, 96), new THREE.ShaderMaterial({
    uniforms: ribbonU, vertexShader: QUAD_VERT,
    fragmentShader: `precision highp float; uniform vec3 uColTop, uColBot; uniform float uAlpha; ${SPLIT} void main(){ gl_FragColor = vec4(room(uColTop,uColBot) * uAlpha, uAlpha); }`,
    ...premult,
  }));
  const haloU = { uColTop: { value: new THREE.Color() }, uColBot: { value: new THREE.Color() }, uAlpha: { value: 1 }, uHaloTop: { value: 1 }, uHaloBot: { value: 1 }, uSplit: split };
  const halo = new THREE.Mesh(quad, new THREE.ShaderMaterial({ uniforms: haloU, vertexShader: QUAD_VERT, fragmentShader: HALO_FRAG, ...premult }));
  halo.scale.setScalar(2.2);
  halo.renderOrder = 20; ball.renderOrder = 21; ribbon.renderOrder = 22;
  orb.add(halo, ball, ribbon);
  scene.add(orb);

  const look = new THREE.Vector3(), camPos = new THREE.Vector3(), tmp = new THREE.Vector3();
  const raycaster = new THREE.Raycaster();
  const plane = new THREE.Plane(new THREE.Vector3(0, 1, 0), 0);
  let rot = 0;

  function resize(w, h, dpr = 1) {
    size.w = w; size.h = h; size.dpr = dpr;
    renderer.setPixelRatio(dpr);
    renderer.setSize(w, h, false);
  }

  const setPair = (top, bot, key, t, b) => { top.value.copy(t[key]); bot.value.copy(b[key]); };

  /**
   * P = { time, cam:[x,y,z], look:[x,y,z], fov, shift:[sx,sy], shape, height, psize, intro, dim, rot,
   *       stale, lost, amp:[g,a,m], voice, orb:[x,y,z], orbA, orbS, markers, pop, e3, frame, frameDash,
   *       pointer:{x,z,s}, rooms:{top,bottom,split} }
   */
  function render(P) {
    const { w, h, dpr } = size;
    const aspect = w / h;
    const fit = Math.max(1, Math.min(2.6, (P.fit ?? 1.3) / aspect));       // keep the subject inside a narrow viewport
    look.fromArray(P.look);
    camPos.fromArray(P.cam).sub(look).multiplyScalar(fit).add(look);
    camera.position.copy(camPos);
    camera.fov = P.fov ?? 35;
    camera.aspect = aspect;
    camera.lookAt(look);
    const sx = P.shift?.[0] ?? 0, sy = P.shift?.[1] ?? 0;
    if (sx || sy) camera.setViewOffset(w, h, -sx * w, -sy * h, w, h); else camera.clearViewOffset();
    camera.updateProjectionMatrix();
    camera.updateMatrixWorld();

    rot = P.rot ?? 0;
    world.rotation.y = rot;
    world.updateMatrixWorld();

    const top = ROOMS[P.rooms?.top ?? 'c'], bot = ROOMS[P.rooms?.bottom ?? P.rooms?.top ?? 'c'];
    split.value = P.rooms?.split == null ? -1e6 : (h - P.rooms.split) * dpr;

    pu.uTime.value = P.time; pu.uShape.value = P.shape; pu.uH.value = P.height ?? 1; pu.uIntro.value = P.intro ?? 1;
    pu.uPx.value = dpr * (h / 900); pu.uSize.value = (P.psize ?? 1) * fit;
    pu.uStale.value = P.stale ?? 0; pu.uLost.value = P.lost ?? 0; pu.uDim.value = P.dim ?? 1; pu.uVoice.value = P.voice ?? 0;
    pu.uAmp.value.fromArray(P.amp ?? [0.5, 0.5, 0.5]);
    const orbP = P.orb ?? [0, 1.7, 0];
    pu.uOrbY.value = P.sphereY ?? 1.7;
    if (P.pointer) {
      const c = Math.cos(-rot), s = Math.sin(-rot);
      pu.uPointer.value.set(P.pointer.x * c + P.pointer.z * s, -P.pointer.x * s + P.pointer.z * c, P.pointer.s);
    } else pu.uPointer.value.set(0, 0, 0);
    setPair(pu.uInkTop, pu.uInkBot, 'ink', top, bot);
    setPair(pu.uTintTop, pu.uTintBot, 'tint', top, bot);
    pu.uGainTop.value = top.gain; pu.uGainBot.value = bot.gain;

    // frame
    setPair(frameU.uColTop, frameU.uColBot, 'ink', top, bot);
    frameU.uAlpha.value = (P.frame ?? 0) * 0.8;
    frameU.uDash.value = P.frameDash ?? 0;
    frame.visible = frameU.uAlpha.value > 0.004;

    // markers
    const flat = Math.min(Math.max(P.shape, 0), 1);
    const mA = P.markers ?? 0, pop = P.pop ?? 1, e3 = P.e3 ?? 0;
    marks.forEach((m, i) => {
      const t = Math.min(Math.max(pop * (marks.length + 2) - i, 0), 1);       // staggered arrival
      const isE3 = m.id === 'E3';
      const miss = isE3 ? e3 : 0;
      const key = m.kind === 'own' ? 'own' : m.kind === 'ally' ? 'ally' : 'enemy';
      m.u.uColTop.value.copy(top[key]).lerp(top.missing, miss);
      m.u.uColBot.value.copy(bot[key]).lerp(bot.missing, miss);
      m.u.uDash.value = miss > 0.5 ? 1 : 0;
      m.u.uDot.value = 1 - miss;
      m.u.uAlpha.value = mA * (t > 0 ? 1 : 0);
      m.u.uPing.value = t > 0 && t < 1 ? t : 0;
      const y = m.elev * ((P.height ?? 1) * 1.7 * (1 - flat) + 0.08 * flat) + 0.07;
      m.mesh.position.set(m.g[0] * R, y, m.g[1] * R);
      m.mesh.visible = m.u.uAlpha.value > 0.004;
    });

    // orb
    const oa = P.orbA ?? 0;
    orb.visible = oa > 0.004;
    if (orb.visible) {
      orb.position.fromArray(orbP);
      const os = P.orbS ?? 1;
      ball.scale.setScalar(os);
      ribbon.scale.setScalar(os * (1 + (P.voice ?? 0) * 0.5));
      ribbon.rotation.set(1.15 + Math.sin(P.time * 0.7) * 0.25, P.time * 0.9, 0.4);
      halo.quaternion.copy(camera.quaternion);
      halo.scale.setScalar(2.2 * os * (1 + (P.voice ?? 0) * 0.6));
      setPair(orbU.uCoreTop, orbU.uCoreBot, 'ink', top, bot);
      orbU.uRimTop.value.set('#d4453a'); orbU.uRimBot.value.set('#d4453a');
      if (top === ROOMS.r) orbU.uRimTop.value.set('#141414');
      if (bot === ROOMS.r) orbU.uRimBot.value.set('#141414');
      orbU.uAlpha.value = oa;
      ribbonU.uColTop.value.copy(top === ROOMS.r ? top.ink : top.tint);
      ribbonU.uColBot.value.copy(bot === ROOMS.r ? bot.ink : bot.tint);
      ribbonU.uAlpha.value = oa;
      setPair(haloU.uColTop, haloU.uColBot, 'ink', top, bot);
      haloU.uAlpha.value = oa;
      haloU.uHaloTop.value = top === ROOMS.c ? 1 : 0;
      haloU.uHaloBot.value = bot === ROOMS.c ? 1 : 0;
    }
    renderer.render(scene, camera);
  }

  /** World point (map-local, before rotation) to CSS pixels inside the canvas. */
  function projectLocal(x, y, z) {
    tmp.set(x, y, z).applyMatrix4(world.matrixWorld).project(camera);
    return { x: (tmp.x * 0.5 + 0.5) * size.w, y: (-tmp.y * 0.5 + 0.5) * size.h, behind: tmp.z > 1 };
  }
  function projectMarker(id) {
    const m = marks.find((k) => k.id === id);
    return m ? projectLocal(m.mesh.position.x, m.mesh.position.y, m.mesh.position.z) : null;
  }
  /** Pointer in NDC to a point on the ground plane (world space). */
  function ground(nx, ny, y = 0.25) {
    plane.constant = -y;
    raycaster.setFromCamera({ x: nx, y: ny }, camera);
    return raycaster.ray.intersectPlane(plane, tmp) ? { x: tmp.x, z: tmp.z } : null;
  }
  return { render, resize, projectLocal, projectMarker, ground, renderer, size, dispose: () => { renderer.dispose(); pg.dispose(); quad.dispose(); } };
}
