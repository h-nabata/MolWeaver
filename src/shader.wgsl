struct Camera {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) instance_pos: vec3<f32>,
    @location(3) instance_radius: f32,
    @location(4) instance_color: vec3<f32>,
    @location(5) instance_flags: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) flags: u32,
};

struct BondVertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) bond_midpoint: vec3<f32>,
    @location(3) bond_direction: vec3<f32>,
    @location(4) bond_length: f32,
    @location(5) bond_radius: f32,
    @location(6) bond_color: vec3<f32>,
    @location(7) bond_flags: u32,
};

struct BondVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) flags: u32,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = input.instance_pos + input.position * input.instance_radius;
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.world_normal = normalize(input.normal);
    out.color = input.instance_color;
    out.flags = input.instance_flags;
    return out;
}

fn build_basis(direction: vec3<f32>) -> mat3x3<f32> {
    let dir = normalize(direction);
    let helper = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(dir.y) > 0.99);
    let right = normalize(cross(helper, dir));
    let up = cross(dir, right);
    return mat3x3<f32>(right, up, dir);
}

@vertex
fn vs_bond(input: BondVertexInput) -> BondVertexOutput {
    var out: BondVertexOutput;
    let basis = build_basis(input.bond_direction);
    let scaled = vec3<f32>(
        input.position.x * input.bond_radius,
        input.position.y * (input.bond_length * 0.5),
        input.position.z * input.bond_radius
    );
    let world_pos = input.bond_midpoint + basis * scaled;
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.world_normal = normalize(basis * input.normal);
    out.color = input.bond_color;
    out.flags = input.bond_flags;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let light_dir = normalize(vec3<f32>(0.4, 0.8, 0.6));
    let diffuse = max(dot(input.world_normal, light_dir), 0.2);
    var color = input.color * diffuse;
    if ((input.flags & 1u) == 1u) {
        color = mix(color, vec3<f32>(1.0, 0.8, 0.2), 0.6);
    }
    return vec4<f32>(color, 1.0);
}

@fragment
fn fs_bond(input: BondVertexOutput) -> @location(0) vec4<f32> {
    let light_dir = normalize(vec3<f32>(0.4, 0.8, 0.6));
    let diffuse = max(dot(input.world_normal, light_dir), 0.2);
    var color = input.color * diffuse;
    if ((input.flags & 1u) == 1u) {
        color = mix(color, vec3<f32>(1.0, 0.8, 0.2), 0.6);
    }
    return vec4<f32>(color, 1.0);
}
