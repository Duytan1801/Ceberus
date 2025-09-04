// Material bound at group(2), binding(0)
struct Lensing3dMaterial {
    camera_pos: vec3<f32>,
    cam_right: vec3<f32>,
    cam_up: vec3<f32>,
    cam_forward: vec3<f32>,

    bh_pos: vec3<f32>,

    fov_y: f32,
    aspect: f32,
    screen_size: vec2<f32>,

    r_g: f32,
    spin_a: f32,

    brightness: f32,
    bg_tiling: f32,

    pattern: u32,
    beam_angle: f32,
    beam_spacing: f32,
    beam_width: f32,
    beam_intensity: f32,

    disk_x: vec3<f32>,
    disk_y: vec3<f32>,
    disk_n: vec3<f32>,
    disk_r_in: f32,
    disk_r_out: f32,
    disk_spin: f32,
    disk_opacity: f32,
    disk_thickness: f32,
    disk_color_hot: vec3<f32>,
    disk_color_cool: vec3<f32>,
};

@group(2) @binding(0)
var<uniform> material: Lensing3dMaterial;

const PI: f32 = 3.141592653589793;
const TAU: f32 = 6.283185307179586;

// Utility
fn hash21(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

fn dir_to_equirect_uv(d: vec3<f32>) -> vec2<f32> {
    let u = atan2(d.z, d.x) / TAU + 0.5;
    let v = 0.5 - asin(clamp(d.y, -1.0, 1.0)) / PI;
    return vec2<f32>(u, v);
}

fn starfield(uv: vec2<f32>, tiling: f32) -> vec3<f32> {
    let p = uv * tiling;
    let i = floor(p);
    let f = fract(p);
    var col = vec3<f32>(0.0);
    for (var dy: i32 = -1; dy <= 1; dy = dy + 1) {
        for (var dx: i32 = -1; dx <= 1; dx = dx + 1) {
            let cell = i + vec2<f32>(f32(dx), f32(dy));
            let rnd = hash21(cell);
            let star_prob = step(0.996, rnd);
            if (star_prob > 0.0) {
                let pos = fract(vec2<f32>(hash21(cell + 1.37), hash21(cell + 9.21)));
                let size = mix(0.0025, 0.012, hash21(cell + 3.77));
                let d = distance(f, pos);
                let core = smoothstep(size, 0.0, d);
                let halo = smoothstep(0.25, 0.0, d) * 0.05;
                let hue = hash21(cell + 5.31);
                let color = mix(vec3<f32>(0.75, 0.8, 1.0), vec3<f32>(1.0, 0.95, 0.8), hue);
                col += color * (core * 2.0 + halo);
            }
        }
    }
    return col + vec3<f32>(0.006, 0.006, 0.01);
}

fn stripe_gaussian_x(x: f32, spacing: f32, width: f32) -> f32 {
    let t = abs(fract(x / spacing) - 0.5) * spacing;
    let sigma = max(width, 1e-4);
    return exp(-0.5 * (t * t) / (sigma * sigma));
}

fn beams_color(uv: vec2<f32>, angle: f32, spacing: f32, width: f32, intensity: f32) -> vec3<f32> {
    let c = cos(angle);
    let s = sin(angle);
    let p = uv - vec2<f32>(0.5, 0.5);
    let pr = vec2<f32>(c * p.x - s * p.y, s * p.x + c * p.y);
    let i = stripe_gaussian_x(pr.x + 0.5, spacing, width) * intensity;
    let beam_color = vec3<f32>(1.0, 0.95, 0.85);
    let bg = vec3<f32>(0.03, 0.03, 0.04);
    return mix(bg, beam_color, clamp(i, 0.0, 1.0));
}

fn grid_color(uv: vec2<f32>, angle: f32, spacing: f32, width: f32, intensity: f32) -> vec3<f32> {
    let c = cos(angle);
    let s = sin(angle);
    let p = uv - vec2<f32>(0.5, 0.5);
    let pr = vec2<f32>(c * p.x - s * p.y, s * p.x + c * p.y);
    let ix = stripe_gaussian_x(pr.x + 0.5, spacing, width);
    let iy = stripe_gaussian_x(pr.y + 0.5, spacing, width);
    let i = clamp((ix + iy) * 0.8 * intensity, 0.0, 1.5);
    let beam_color = vec3<f32>(0.95, 0.95, 1.0);
    let bg = vec3<f32>(0.03, 0.03, 0.04);
    return mix(bg, beam_color, clamp(i, 0.0, 1.0));
}

struct FsIn {
    @builtin(position) pos: vec4<f32>,
};

// Approximate GR deflection for Schwarzschild + simple spin cue
fn deflection_angle(b: f32, r_g: f32, a: f32, spin_sign: f32) -> f32 {
    let eps = 1e-5;
    let u = r_g / max(b, eps);

    // 1st + 2nd order GR terms
    var alpha = 4.0 * u + (15.0 * PI / 4.0) * u * u;

    // Kerr-ish frame dragging term (stylized)
    alpha += spin_sign * a * 4.0 * u * u;

    // Soft clamp near critical b
    alpha = alpha / (1.0 + alpha * 0.25);
    return alpha;
}

// Sign for spin term based on handedness wrt spin axis
fn spin_handedness(q_cam: vec2<f32>, spin_axis: vec3<f32>) -> f32 {
    let b_dir_world = normalize(
        material.cam_right * q_cam.x +
        material.cam_up * q_cam.y
    );
    let s = dot(spin_axis, cross(material.cam_forward, b_dir_world));
    return sign(s);
}

// Disk sample with thickness + beaming + gravitational redshift
fn sample_disk(d_src: vec3<f32>) -> vec4<f32> {
    let denom = dot(d_src, material.disk_n);
    if (abs(denom) < 1e-4) { return vec4<f32>(0.0,0.0,0.0,0.0); }

    let t_plane = dot(material.bh_pos - material.camera_pos, material.disk_n) / denom;
    if (t_plane <= 0.0) { return vec4<f32>(0.0,0.0,0.0,0.0); }
    let p_hit = material.camera_pos + d_src * t_plane;

    // Signed distance from midplane along normal
    let h = dot(p_hit - material.bh_pos, material.disk_n);
    if (abs(h) > material.disk_thickness) { return vec4<f32>(0.0,0.0,0.0,0.0); }

    // Project to disk plane
    let r3 = p_hit - material.bh_pos - h * material.disk_n;
    let r = length(r3);
    if (r < material.disk_r_in || r > material.disk_r_out) {
        return vec4<f32>(0.0,0.0,0.0,0.0);
    }

    let r_hat = normalize(r3);
    let t_hat = normalize(cross(material.disk_n, r_hat));

    // Kepler-like orbit speed
    let v = clamp(material.disk_spin / sqrt(max(r, 1e-3)), 0.0, 0.95);
    let cos_theta = dot(t_hat, -d_src);
    let gamma = 1.0 / sqrt(max(1.0 - v*v, 1e-4));
    let D = 1.0 / max(gamma * (1.0 - v * cos_theta), 1e-3);

    // Gravitational redshift (approx)
    let g_grav = sqrt(max(1.0 - material.r_g / max(r, 1.001), 0.0));
    let g_total = D * g_grav;

    // Temperature ramp + Doppler hue
    let t_base = pow(clamp(material.disk_r_in / r, 0.0, 1.0), 0.75);
    let hue_mix = clamp(0.4 + 0.6 * (D - 1.0), 0.0, 1.0);
    var disk_col = mix(material.disk_color_cool, material.disk_color_hot, clamp(t_base + hue_mix*0.5, 0.0, 1.0));

    // Intensity scaling
    let intensity = pow(g_total, 3.0) * (1.0 / sqrt(max(r, 1e-3)));

    // Self-occlusion / facing
    let facing = clamp(dot(material.disk_n, -d_src) * 0.5 + 0.5, 0.0, 1.0);
    disk_col *= intensity * facing;

    // Soft edges by slab thickness
    let alpha = material.disk_opacity * smoothstep(material.disk_thickness, 0.0, abs(h));
    return vec4<f32>(disk_col, alpha);
}

@fragment
fn fragment(in: FsIn) -> @location(0) vec4<f32> {
    // Screen to NDC to camera-plane coordinates (z=1)
    let uv01 = vec2<f32>(
        in.pos.x / max(material.screen_size.x, 1.0),
        1.0 - in.pos.y / max(material.screen_size.y, 1.0),
    );
    let ndc = vec2<f32>(uv01.x * 2.0 - 1.0, uv01.y * 2.0 - 1.0);
    let tan_half_fovy = tan(material.fov_y * 0.5);
    let tan_half_fovx = material.aspect * tan_half_fovy;
    let x = ndc.x * tan_half_fovx;
    let y = ndc.y * tan_half_fovy;
    let q = vec2<f32>(x, y);

    // Camera -> BH vector in camera basis
// Camera -> BH vector in camera basis
    let to_bh_world = material.bh_pos - material.camera_pos;
    let x_bh = dot(to_bh_world, material.cam_right);
    let y_bh = dot(to_bh_world, material.cam_up);
    let z_bh = dot(to_bh_world, material.cam_forward);
    let center = vec2<f32>(x_bh / max(abs(z_bh), 1e-4), y_bh / max(abs(z_bh), 1e-4));

    // Ray angle relative to BH center on camera plane
    let qp = q - center;
    let r_cam = length(qp);
    let eps = 1e-5;

    // Shadow radius from GR: b_crit = 3√3 r_g; angular radius θ ≈ b_crit / |z_bh|
    let b_crit = 3.0 * sqrt(3.0) * material.r_g;
    let theta_shadow = b_crit / max(abs(z_bh), 1e-4);
    if (r_cam < theta_shadow) {
        return vec4<f32>(0.0,0.0,0.0,1.0);
    }

    // Current viewing angle (small-angle tan≈θ)
    let theta = r_cam;

    // Impact parameter: use lens-plane depth, not total distance
    let b = abs(z_bh) * theta;
    // Spin sign (which side gets dragged forward)
    let spin_sign = spin_handedness(qp / max(r_cam, eps), material.disk_n);

    // Deflection angle α(b)
    let alpha = deflection_angle(b, material.r_g, material.spin_a, spin_sign);

    // Source angle
    let theta_src = max(theta - alpha, 0.0);
    let scale = theta_src / max(theta, eps);
    let s = center + qp * scale;

    // Source direction in world
    let dir_cam_src = normalize(vec3<f32>(s.x, s.y, 1.0));
    let d_src = normalize(
        material.cam_right * dir_cam_src.x +
        material.cam_up * dir_cam_src.y +
        material.cam_forward * dir_cam_src.z
    );

    // Background
    let env_uv = dir_to_equirect_uv(d_src);
    var bg = vec3<f32>(0.0);
    if (material.pattern == 0u) {
        bg = starfield(env_uv, material.bg_tiling);
    } else if (material.pattern == 1u) {
        bg = beams_color(env_uv, material.beam_angle, material.beam_spacing, material.beam_width, material.beam_intensity);
    } else {
        bg = grid_color(env_uv, material.beam_angle, material.beam_spacing, material.beam_width, material.beam_intensity);
    }

    // Accretion disk
    let disk = sample_disk(d_src);
    var col = mix(bg, disk.xyz, clamp(disk.w, 0.0, 1.0));

    // Photon ring glow near the shadow boundary (artistic)
    let glow = smoothstep(theta_shadow * 1.02, theta_shadow * 1.1, r_cam)
             * smoothstep(theta_shadow * 1.8, theta_shadow * 1.2, r_cam);
    col += vec3<f32>(1.0, 0.95, 0.85) * glow * 0.18;

    col *= material.brightness;
    return vec4<f32>(col, 1.0);
}