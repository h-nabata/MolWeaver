use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec3, Vec4};
use wgpu::util::DeviceExt;
use winit::event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::Key;
use winit::window::{Window, WindowBuilder};

use molweaver::{
    bond_instance_from_positions, element_color, Atom, AtomId, BondId, Command, CommandHistory,
    Molecule,
};

const SAMPLE_PATH: &str = "assets/sample.xyz";
const SPHERE_SEGMENTS: u32 = 32;
const SPHERE_RINGS: u32 = 16;
const CYLINDER_SEGMENTS: u32 = 24;
const ATOM_RADIUS: f32 = 0.5;
const SPACE_FILL_RADIUS: f32 = 0.9;
const BOND_RADIUS: f32 = 0.15;
const HISTORY_CAPACITY: usize = 100;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
}

impl Vertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct InstanceData {
    position: [f32; 3],
    radius: f32,
    color: [f32; 3],
    flags: u32,
}

impl InstanceData {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceData>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 28,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BondInstanceData {
    midpoint: [f32; 3],
    direction: [f32; 3],
    length: f32,
    radius: f32,
    color: [f32; 3],
    flags: u32,
}

impl BondInstanceData {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<BondInstanceData>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: 28,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 44,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 4],
}

struct Camera {
    yaw: f32,
    pitch: f32,
    distance: f32,
    target: Vec3,
}

impl Camera {
    fn position(&self) -> Vec3 {
        let (yaw_sin, yaw_cos) = self.yaw.sin_cos();
        let (pitch_sin, pitch_cos) = self.pitch.sin_cos();
        Vec3::new(
            self.distance * pitch_cos * yaw_cos,
            self.distance * pitch_sin,
            self.distance * pitch_cos * yaw_sin,
        ) + self.target
    }

    fn view_proj(&self, aspect: f32) -> Mat4 {
        let position = self.position();
        let view = Mat4::look_at_rh(position, self.target, Vec3::Y);
        let proj = Mat4::perspective_rh(45.0_f32.to_radians(), aspect, 0.1, 200.0);
        proj * view
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    AddAtom,
    AddBond,
    Move,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Representation {
    BallAndStick,
    SpaceFilling,
}

struct UiState {
    camera: Camera,
    dragging: bool,
    last_cursor: Option<Vec2>,
    drag_distance: f32,
    camera_dirty: bool,
    selection: Option<AtomId>,
    frame_timer: Instant,
    fps: f32,
    file_name: String,
    tool: Tool,
    edit_element: String,
    move_step: f32,
    bond_target: Option<AtomId>,
    status_message: String,
    modifiers: winit::keyboard::ModifiersState,
    representation: Representation,
}

impl UiState {
    fn new() -> Self {
        Self {
            camera: Camera {
                yaw: 0.8,
                pitch: 0.3,
                distance: 8.0,
                target: Vec3::ZERO,
            },
            dragging: false,
            last_cursor: None,
            drag_distance: 0.0,
            camera_dirty: true,
            selection: None,
            frame_timer: Instant::now(),
            fps: 0.0,
            file_name: SAMPLE_PATH.to_string(),
            tool: Tool::Select,
            edit_element: "C".to_string(),
            move_step: 0.25,
            bond_target: None,
            status_message: String::new(),
            modifiers: winit::keyboard::ModifiersState::default(),
            representation: Representation::BallAndStick,
        }
    }

    fn update_cursor(&mut self, position: Vec2) {
        if self.dragging {
            if let Some(last) = self.last_cursor {
                let delta = position - last;
                self.drag_distance += delta.length();
                self.orbit(delta);
            }
        }
        self.last_cursor = Some(position);
    }

    fn orbit(&mut self, delta: Vec2) {
        let speed = 0.01;
        self.camera.yaw -= delta.x * speed;
        self.camera.pitch = (self.camera.pitch - delta.y * speed).clamp(-1.4, 1.4);
        self.camera_dirty = true;
    }

    fn zoom(&mut self, delta: f32) {
        self.camera.distance = (self.camera.distance * (1.0 - delta)).clamp(2.0, 60.0);
        self.camera_dirty = true;
    }

    fn begin_drag(&mut self) {
        self.dragging = true;
        self.drag_distance = 0.0;
    }

    fn end_drag(&mut self) {
        self.dragging = false;
        self.drag_distance = 0.0;
    }

    fn update_fps(&mut self) {
        let now = Instant::now();
        let dt = now - self.frame_timer;
        self.frame_timer = now;
        let frame_seconds = dt.as_secs_f32();
        if frame_seconds > 0.0 {
            let fps = 1.0 / frame_seconds;
            self.fps = if self.fps == 0.0 {
                fps
            } else {
                self.fps * 0.9 + fps * 0.1
            };
        }
    }
}

struct RenderState<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    atom_pipeline: wgpu::RenderPipeline,
    bond_pipeline: wgpu::RenderPipeline,
    sphere_vertex_buffer: wgpu::Buffer,
    sphere_index_buffer: wgpu::Buffer,
    sphere_index_count: u32,
    cylinder_vertex_buffer: wgpu::Buffer,
    cylinder_index_buffer: wgpu::Buffer,
    cylinder_index_count: u32,
    atom_instance_buffer: Option<wgpu::Buffer>,
    atom_instance_data: Vec<InstanceData>,
    atom_instance_ids: Vec<AtomId>,
    atom_lookup: HashMap<AtomId, usize>,
    bond_instance_buffer: Option<wgpu::Buffer>,
    bond_instance_data: Vec<BondInstanceData>,
    bond_instance_ids: Vec<BondId>,
    bond_lookup: HashMap<BondId, usize>,
    atom_to_bonds: HashMap<AtomId, Vec<BondId>>,
    atom_instance_capacity: usize,
    bond_instance_capacity: usize,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    depth_texture: Texture,
    representation: Representation,
}

struct Texture {
    view: wgpu::TextureView,
}

impl Texture {
    fn new_depth(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> Self {
        let size = wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth_texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24Plus,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self { view }
    }
}

impl<'a> RenderState<'a> {
    async fn new(window: &'a Window) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = unsafe { instance.create_surface(window) }.expect("create surface");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("request adapter");
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .expect("request device");

        let surface_caps = surface.get_capabilities(&adapter);
        let format = surface_caps.formats[0];
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            desired_maximum_frame_latency: 2,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let (sphere_vertices, sphere_indices) = create_sphere_mesh(SPHERE_SEGMENTS, SPHERE_RINGS);
        let sphere_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sphere_vertices"),
            contents: bytemuck::cast_slice(&sphere_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let sphere_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sphere_indices"),
            contents: bytemuck::cast_slice(&sphere_indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let (cylinder_vertices, cylinder_indices) = create_cylinder_mesh(CYLINDER_SEGMENTS);
        let cylinder_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cylinder_vertices"),
            contents: bytemuck::cast_slice(&cylinder_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let cylinder_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cylinder_indices"),
            contents: bytemuck::cast_slice(&cylinder_indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let camera_uniform = CameraUniform {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            camera_pos: [0.0; 4],
        };
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera_buffer"),
            contents: bytemuck::bytes_of(&camera_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera_bind_group_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera_bind_group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline_layout"),
            bind_group_layouts: &[&camera_bind_group_layout],
            push_constant_ranges: &[],
        });
        let atom_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sphere_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc(), InstanceData::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        let bond_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bond_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_bond",
                buffers: &[Vertex::desc(), BondInstanceData::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_bond",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let depth_texture = Texture::new_depth(&device, &config);

        Self {
            surface,
            device,
            queue,
            config,
            size,
            atom_pipeline,
            bond_pipeline,
            sphere_vertex_buffer,
            sphere_index_buffer,
            sphere_index_count: sphere_indices.len() as u32,
            cylinder_vertex_buffer,
            cylinder_index_buffer,
            cylinder_index_count: cylinder_indices.len() as u32,
            atom_instance_buffer: None,
            atom_instance_data: Vec::new(),
            atom_instance_ids: Vec::new(),
            atom_lookup: HashMap::new(),
            bond_instance_buffer: None,
            bond_instance_data: Vec::new(),
            bond_instance_ids: Vec::new(),
            bond_lookup: HashMap::new(),
            atom_to_bonds: HashMap::new(),
            atom_instance_capacity: 0,
            bond_instance_capacity: 0,
            camera_buffer,
            camera_bind_group,
            depth_texture,
            representation: Representation::BallAndStick,
        }
    }

    fn resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.size = size;
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        self.depth_texture = Texture::new_depth(&self.device, &self.config);
    }

    fn set_molecule(&mut self, molecule: &Molecule) {
        self.atom_instance_data = molecule
            .atoms_in_order()
            .map(|atom| InstanceData {
                position: atom.position,
                radius: self.atom_radius(),
                color: element_color(&atom.element),
                flags: 0,
            })
            .collect();
        self.atom_instance_ids = molecule.atom_ids();
        self.atom_lookup = self
            .atom_instance_ids
            .iter()
            .enumerate()
            .map(|(idx, id)| (*id, idx))
            .collect();
        self.ensure_atom_capacity(self.atom_instance_data.len());
        if let Some(buffer) = &self.atom_instance_buffer {
            if !self.atom_instance_data.is_empty() {
                self.queue
                    .write_buffer(buffer, 0, bytemuck::cast_slice(&self.atom_instance_data));
            }
        }

        self.rebuild_bond_instances(molecule);
    }

    fn set_representation(&mut self, representation: Representation, molecule: &Molecule) {
        if self.representation == representation {
            return;
        }
        self.representation = representation;
        let radius = self.atom_radius();
        for instance in &mut self.atom_instance_data {
            instance.radius = radius;
        }
        if let Some(buffer) = &self.atom_instance_buffer {
            if !self.atom_instance_data.is_empty() {
                self.queue
                    .write_buffer(buffer, 0, bytemuck::cast_slice(&self.atom_instance_data));
            }
        }
        self.rebuild_bond_instances(molecule);
    }

    fn atom_radius(&self) -> f32 {
        match self.representation {
            Representation::BallAndStick => ATOM_RADIUS,
            Representation::SpaceFilling => SPACE_FILL_RADIUS,
        }
    }

    fn rebuild_bond_instances(&mut self, molecule: &Molecule) {
        self.bond_instance_data.clear();
        self.bond_instance_ids.clear();
        self.bond_lookup.clear();
        self.atom_to_bonds.clear();
        if self.representation == Representation::SpaceFilling {
            self.ensure_bond_capacity(0);
            return;
        }
        for bond in molecule.bonds() {
            if let (Some(atom_a), Some(atom_b)) =
                (molecule.get_atom(bond.a), molecule.get_atom(bond.b))
            {
                let instance = bond_instance_from_positions(atom_a.position, atom_b.position);
                self.bond_instance_ids.push(bond.id);
                self.bond_lookup
                    .insert(bond.id, self.bond_instance_data.len());
                self.bond_instance_data.push(BondInstanceData {
                    midpoint: instance.midpoint,
                    direction: instance.direction,
                    length: instance.length,
                    radius: BOND_RADIUS,
                    color: [0.7, 0.7, 0.7],
                    flags: 0,
                });
                self.atom_to_bonds.entry(bond.a).or_default().push(bond.id);
                self.atom_to_bonds.entry(bond.b).or_default().push(bond.id);
            }
        }
        self.ensure_bond_capacity(self.bond_instance_data.len());
        if let Some(buffer) = &self.bond_instance_buffer {
            if !self.bond_instance_data.is_empty() {
                self.queue
                    .write_buffer(buffer, 0, bytemuck::cast_slice(&self.bond_instance_data));
            }
        }
    }

    fn ensure_atom_capacity(&mut self, needed: usize) {
        if needed <= self.atom_instance_capacity {
            return;
        }
        let new_capacity = needed.next_power_of_two().max(1);
        let buffer_size =
            (new_capacity * std::mem::size_of::<InstanceData>()) as wgpu::BufferAddress;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("atom_instance_buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !self.atom_instance_data.is_empty() {
            self.queue
                .write_buffer(&buffer, 0, bytemuck::cast_slice(&self.atom_instance_data));
        }
        self.atom_instance_buffer = Some(buffer);
        self.atom_instance_capacity = new_capacity;
    }

    fn ensure_bond_capacity(&mut self, needed: usize) {
        if needed <= self.bond_instance_capacity {
            return;
        }
        let new_capacity = needed.next_power_of_two().max(1);
        let buffer_size =
            (new_capacity * std::mem::size_of::<BondInstanceData>()) as wgpu::BufferAddress;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bond_instance_buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !self.bond_instance_data.is_empty() {
            self.queue
                .write_buffer(&buffer, 0, bytemuck::cast_slice(&self.bond_instance_data));
        }
        self.bond_instance_buffer = Some(buffer);
        self.bond_instance_capacity = new_capacity;
    }

    fn add_atom_instance(&mut self, atom: &Atom) {
        let index = self.atom_instance_data.len();
        self.atom_instance_data.push(InstanceData {
            position: atom.position,
            radius: self.atom_radius(),
            color: element_color(&atom.element),
            flags: 0,
        });
        self.atom_instance_ids.push(atom.id);
        self.atom_lookup.insert(atom.id, index);
        self.ensure_atom_capacity(self.atom_instance_data.len());
        if let Some(buffer) = &self.atom_instance_buffer {
            let offset = (index * std::mem::size_of::<InstanceData>()) as wgpu::BufferAddress;
            self.queue.write_buffer(
                buffer,
                offset,
                bytemuck::bytes_of(&self.atom_instance_data[index]),
            );
        }
    }

    fn remove_atom_instance(&mut self, atom_id: AtomId) {
        let Some(index) = self.atom_lookup.get(&atom_id).copied() else {
            return;
        };
        let last_index = self.atom_instance_data.len().saturating_sub(1);
        self.atom_instance_data.swap_remove(index);
        self.atom_instance_ids.swap_remove(index);
        self.atom_lookup.remove(&atom_id);
        if index != last_index {
            if let Some(swapped_id) = self.atom_instance_ids.get(index).copied() {
                self.atom_lookup.insert(swapped_id, index);
                if let Some(buffer) = &self.atom_instance_buffer {
                    let offset =
                        (index * std::mem::size_of::<InstanceData>()) as wgpu::BufferAddress;
                    self.queue.write_buffer(
                        buffer,
                        offset,
                        bytemuck::bytes_of(&self.atom_instance_data[index]),
                    );
                }
            }
        }
        if let Some(bonds) = self.atom_to_bonds.remove(&atom_id) {
            for bond_id in bonds {
                self.remove_bond_instance(bond_id);
            }
        }
    }

    fn update_atom_position(&mut self, atom_id: AtomId, position: [f32; 3]) {
        let Some(index) = self.atom_lookup.get(&atom_id).copied() else {
            return;
        };
        if let Some(instance) = self.atom_instance_data.get_mut(index) {
            instance.position = position;
            if let Some(buffer) = &self.atom_instance_buffer {
                let offset = (index * std::mem::size_of::<InstanceData>()) as wgpu::BufferAddress;
                self.queue
                    .write_buffer(buffer, offset, bytemuck::bytes_of(instance));
            }
        }
    }

    fn update_bonds_for_atom(&mut self, atom_id: AtomId, molecule: &Molecule) {
        let Some(bond_ids) = self.atom_to_bonds.get(&atom_id).cloned() else {
            return;
        };
        for bond_id in bond_ids {
            self.update_bond_instance(bond_id, molecule);
        }
    }

    fn add_bond_instance(&mut self, bond_id: BondId, molecule: &Molecule) {
        if self.representation == Representation::SpaceFilling {
            return;
        }
        let Some(bond) = molecule.bonds().find(|bond| bond.id == bond_id) else {
            return;
        };
        let (Some(atom_a), Some(atom_b)) = (molecule.get_atom(bond.a), molecule.get_atom(bond.b))
        else {
            return;
        };
        let instance = bond_instance_from_positions(atom_a.position, atom_b.position);
        let index = self.bond_instance_data.len();
        self.bond_instance_data.push(BondInstanceData {
            midpoint: instance.midpoint,
            direction: instance.direction,
            length: instance.length,
            radius: BOND_RADIUS,
            color: [0.7, 0.7, 0.7],
            flags: 0,
        });
        self.bond_instance_ids.push(bond_id);
        self.bond_lookup.insert(bond_id, index);
        self.atom_to_bonds.entry(bond.a).or_default().push(bond_id);
        self.atom_to_bonds.entry(bond.b).or_default().push(bond_id);
        self.ensure_bond_capacity(self.bond_instance_data.len());
        if let Some(buffer) = &self.bond_instance_buffer {
            let offset = (index * std::mem::size_of::<BondInstanceData>()) as wgpu::BufferAddress;
            self.queue.write_buffer(
                buffer,
                offset,
                bytemuck::bytes_of(&self.bond_instance_data[index]),
            );
        }
    }

    fn remove_bond_instance(&mut self, bond_id: BondId) {
        let Some(index) = self.bond_lookup.get(&bond_id).copied() else {
            return;
        };
        let last_index = self.bond_instance_data.len().saturating_sub(1);
        self.bond_instance_data.swap_remove(index);
        self.bond_instance_ids.swap_remove(index);
        self.bond_lookup.remove(&bond_id);
        if index != last_index {
            if let Some(swapped_id) = self.bond_instance_ids.get(index).copied() {
                self.bond_lookup.insert(swapped_id, index);
                if let Some(buffer) = &self.bond_instance_buffer {
                    let offset =
                        (index * std::mem::size_of::<BondInstanceData>()) as wgpu::BufferAddress;
                    self.queue.write_buffer(
                        buffer,
                        offset,
                        bytemuck::bytes_of(&self.bond_instance_data[index]),
                    );
                }
            }
        }
        for bonds in self.atom_to_bonds.values_mut() {
            bonds.retain(|id| *id != bond_id);
        }
    }

    fn update_bond_instance(&mut self, bond_id: BondId, molecule: &Molecule) {
        let Some(index) = self.bond_lookup.get(&bond_id).copied() else {
            return;
        };
        let Some(bond) = molecule.bonds().find(|bond| bond.id == bond_id) else {
            return;
        };
        let (Some(atom_a), Some(atom_b)) = (molecule.get_atom(bond.a), molecule.get_atom(bond.b))
        else {
            return;
        };
        let instance = bond_instance_from_positions(atom_a.position, atom_b.position);
        if let Some(data) = self.bond_instance_data.get_mut(index) {
            data.midpoint = instance.midpoint;
            data.direction = instance.direction;
            data.length = instance.length;
            if let Some(buffer) = &self.bond_instance_buffer {
                let offset =
                    (index * std::mem::size_of::<BondInstanceData>()) as wgpu::BufferAddress;
                self.queue
                    .write_buffer(buffer, offset, bytemuck::bytes_of(data));
            }
        }
    }

    fn update_selection(&mut self, previous: Option<AtomId>, next: Option<AtomId>) {
        if let Some(prev) = previous {
            if let Some(index) = self.atom_lookup.get(&prev).copied() {
                let updated = self.atom_instance_data.get_mut(index).map(|data| {
                    data.flags &= !1;
                    *data
                });
                if let Some(data) = updated {
                    self.write_atom_instance(index, data);
                }
            }
        }
        if let Some(next) = next {
            if let Some(index) = self.atom_lookup.get(&next).copied() {
                let updated = self.atom_instance_data.get_mut(index).map(|data| {
                    data.flags |= 1;
                    *data
                });
                if let Some(data) = updated {
                    self.write_atom_instance(index, data);
                }
            }
        }
    }

    fn write_atom_instance(&self, index: usize, data: InstanceData) {
        if let Some(buffer) = &self.atom_instance_buffer {
            let offset = (index * std::mem::size_of::<InstanceData>()) as wgpu::BufferAddress;
            self.queue
                .write_buffer(buffer, offset, bytemuck::bytes_of(&data));
        }
    }

    fn update_camera(&self, camera: &Camera, aspect: f32) {
        let view_proj = camera.view_proj(aspect).to_cols_array_2d();
        let position = camera.position();
        let uniform = CameraUniform {
            view_proj,
            camera_pos: [position.x, position.y, position.z, 1.0],
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    fn pick_atom(
        &self,
        cursor: Vec2,
        camera: &Camera,
        size: winit::dpi::PhysicalSize<u32>,
    ) -> Option<AtomId> {
        if size.width == 0 || size.height == 0 {
            return None;
        }
        let ndc = Vec2::new(
            (2.0 * cursor.x / size.width as f32) - 1.0,
            1.0 - (2.0 * cursor.y / size.height as f32),
        );

        let aspect = size.width as f32 / size.height as f32;
        let view_proj = camera.view_proj(aspect);
        let inv_view_proj = view_proj.inverse();
        let near_point = inv_view_proj * Vec4::new(ndc.x, ndc.y, 0.0, 1.0);
        let far_point = inv_view_proj * Vec4::new(ndc.x, ndc.y, 1.0, 1.0);
        let near = near_point.truncate() / near_point.w;
        let far = far_point.truncate() / far_point.w;
        let ray_dir = (far - near).normalize();
        let ray_origin = near;

        let mut best: Option<(AtomId, f32)> = None;
        for (index, instance) in self.atom_instance_data.iter().enumerate() {
            let center = Vec3::from_array(instance.position);
            let to_center = center - ray_origin;
            let t = ray_dir.dot(to_center);
            if t < 0.0 {
                continue;
            }
            let closest = ray_origin + ray_dir * t;
            let dist_sq = center.distance_squared(closest);
            let radius_sq = instance.radius * instance.radius;
            if dist_sq <= radius_sq {
                let atom_id = self.atom_instance_ids[index];
                match best {
                    Some((_, best_t)) if t >= best_t => {}
                    _ => best = Some((atom_id, t)),
                }
            }
        }
        best.map(|(atom_id, _)| atom_id)
    }

    fn render(
        &mut self,
        egui_renderer: &mut egui_wgpu::Renderer,
        paint_jobs: &[egui::ClippedPrimitive],
        screen_descriptor: &egui_wgpu::ScreenDescriptor,
    ) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render_encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            if let Some(bond_buffer) = &self.bond_instance_buffer {
                if !self.bond_instance_data.is_empty() {
                    render_pass.set_pipeline(&self.bond_pipeline);
                    render_pass.set_vertex_buffer(0, self.cylinder_vertex_buffer.slice(..));
                    render_pass.set_vertex_buffer(1, bond_buffer.slice(..));
                    render_pass.set_index_buffer(
                        self.cylinder_index_buffer.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    render_pass.draw_indexed(
                        0..self.cylinder_index_count,
                        0,
                        0..self.bond_instance_data.len() as u32,
                    );
                }
            }

            render_pass.set_pipeline(&self.atom_pipeline);
            render_pass.set_vertex_buffer(0, self.sphere_vertex_buffer.slice(..));
            if let Some(instance_buffer) = &self.atom_instance_buffer {
                render_pass.set_vertex_buffer(1, instance_buffer.slice(..));
                render_pass.set_index_buffer(
                    self.sphere_index_buffer.slice(..),
                    wgpu::IndexFormat::Uint32,
                );
                render_pass.draw_indexed(
                    0..self.sphere_index_count,
                    0,
                    0..self.atom_instance_data.len() as u32,
                );
            }
        }

        egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            paint_jobs,
            screen_descriptor,
        );
        {
            let mut egui_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            egui_renderer.render(&mut egui_pass, paint_jobs, screen_descriptor);
        }

        self.queue.submit(Some(encoder.finish()));
        output.present();
        Ok(())
    }
}

fn create_sphere_mesh(segments: u32, rings: u32) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for ring in 0..=rings {
        let v = ring as f32 / rings as f32;
        let theta = v * std::f32::consts::PI;
        let (sin_theta, cos_theta) = theta.sin_cos();
        for segment in 0..=segments {
            let u = segment as f32 / segments as f32;
            let phi = u * std::f32::consts::TAU;
            let (sin_phi, cos_phi) = phi.sin_cos();
            let position = Vec3::new(sin_theta * cos_phi, cos_theta, sin_theta * sin_phi);
            vertices.push(Vertex {
                position: position.to_array(),
                normal: position.normalize_or_zero().to_array(),
            });
        }
    }

    let stride = segments + 1;
    for ring in 0..rings {
        for segment in 0..segments {
            let i0 = ring * stride + segment;
            let i1 = i0 + 1;
            let i2 = i0 + stride;
            let i3 = i2 + 1;
            indices.extend_from_slice(&[i0, i2, i1, i1, i2, i3]);
        }
    }

    (vertices, indices)
}

fn create_cylinder_mesh(segments: u32) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let angle = t * std::f32::consts::TAU;
        let (sin, cos) = angle.sin_cos();
        let normal = Vec3::new(cos, 0.0, sin);
        vertices.push(Vertex {
            position: [cos, -0.5, sin],
            normal: normal.to_array(),
        });
        vertices.push(Vertex {
            position: [cos, 0.5, sin],
            normal: normal.to_array(),
        });
    }

    for i in 0..segments {
        let base = i * 2;
        indices.extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
    }

    (vertices, indices)
}

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    let window = WindowBuilder::new()
        .with_title("MolWeaver")
        .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0))
        .build(&event_loop)
        .expect("window");

    let mut render_state = pollster::block_on(RenderState::new(&window));
    let viewport_id = egui_ctx.viewport_id();
    let mut egui_state = egui_winit::State::new(
        egui_ctx.clone(),
        viewport_id,
        &window,
        Some(window.scale_factor() as f32),
        None,
    );
    let egui_ctx = egui::Context::default();
    let mut egui_renderer =
        egui_wgpu::Renderer::new(&render_state.device, render_state.config.format, None, 1);

    let (tx, rx) = mpsc::channel::<Result<Molecule, String>>();
    let file_name = SAMPLE_PATH.to_string();
    thread::spawn(move || {
        let result = std::fs::read_to_string(&file_name)
            .map_err(|err| err.to_string())
            .and_then(|contents| molweaver::parse_xyz(&contents).map_err(|err| err.to_string()));
        let _ = tx.send(result);
    });

    let mut molecule: Option<Molecule> = None;
    let mut ui_state = UiState::new();
    let mut history = CommandHistory::new(HISTORY_CAPACITY);

    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop
        .run(move |event, target| match event {
            Event::WindowEvent { event, window_id } if window_id == window.id() => {
                if egui_state.on_window_event(&window, &event).consumed {
                    return;
                }
                match event {
                    WindowEvent::CloseRequested => target.exit(),
                    WindowEvent::Resized(size) => render_state.resize(size),
                    WindowEvent::ScaleFactorChanged {
                        inner_size_writer, ..
                    } => {
                        let new_size = window.inner_size();
                        inner_size_writer.request_inner_size(new_size);
                        render_state.resize(new_size);
                    }
                    WindowEvent::ModifiersChanged(modifiers) => {
                        ui_state.modifiers = modifiers.state();
                    }
                    WindowEvent::KeyboardInput { event, .. } => {
                        if event.state == ElementState::Pressed {
                            if handle_shortcuts(&event.logical_key, &ui_state.modifiers) {
                                if let Some(molecule_ref) = molecule.as_mut() {
                                    match &event.logical_key {
                                        Key::Character(key) if key.eq_ignore_ascii_case("z") => {
                                            if ui_state.modifiers.shift_key() {
                                                redo_command(
                                                    &mut history,
                                                    molecule_ref,
                                                    &mut render_state,
                                                    &mut ui_state,
                                                );
                                            } else {
                                                undo_command(
                                                    &mut history,
                                                    molecule_ref,
                                                    &mut render_state,
                                                    &mut ui_state,
                                                );
                                            }
                                        }
                                        Key::Character(key) if key.eq_ignore_ascii_case("y") => {
                                            redo_command(
                                                &mut history,
                                                molecule_ref,
                                                &mut render_state,
                                                &mut ui_state,
                                            );
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        ui_state.update_cursor(Vec2::new(position.x as f32, position.y as f32));
                    }
                    WindowEvent::MouseInput { state, button, .. } => {
                        if button == MouseButton::Left {
                            match state {
                                ElementState::Pressed => ui_state.begin_drag(),
                                ElementState::Released => {
                                    if ui_state.drag_distance < 4.0 {
                                        if let Some(cursor) = ui_state.last_cursor {
                                            let picked = render_state.pick_atom(
                                                cursor,
                                                &ui_state.camera,
                                                render_state.size,
                                            );
                                            handle_click(
                                                picked,
                                                &mut render_state,
                                                &mut ui_state,
                                                molecule.as_mut(),
                                                &mut history,
                                            );
                                        }
                                    }
                                    ui_state.end_drag();
                                }
                            }
                        }
                    }
                    WindowEvent::MouseWheel { delta, .. } => {
                        let scroll = match delta {
                            MouseScrollDelta::LineDelta(_, y) => y,
                            MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 100.0,
                        };
                        if scroll.abs() > f32::EPSILON {
                            ui_state.zoom(scroll * 0.1);
                        }
                    }
                    _ => {}
                }
            }
            Event::AboutToWait => {
                window.request_redraw();
            }
            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                window_id,
            } if window_id == window.id() => {
                if let Ok(result) = rx.try_recv() {
                    match result {
                        Ok(loaded) => {
                            ui_state.file_name = format!("{SAMPLE_PATH} ({})", loaded.name);
                            render_state.set_molecule(&loaded);
                            molecule = Some(loaded);
                            ui_state.selection = None;
                            ui_state.bond_target = None;
                            history = CommandHistory::new(HISTORY_CAPACITY);
                        }
                        Err(err) => {
                            ui_state.file_name = format!("load failed: {err}");
                        }
                    }
                }

                let aspect =
                    render_state.size.width as f32 / render_state.size.height.max(1) as f32;
                if ui_state.camera_dirty {
                    render_state.update_camera(&ui_state.camera, aspect);
                    ui_state.camera_dirty = false;
                }
                ui_state.update_fps();

                let atom_count = molecule.as_ref().map(|mol| mol.atom_count()).unwrap_or(0);
                let bond_count = molecule
                    .as_ref()
                    .map(|mol| mol.bonds().count())
                    .unwrap_or(0);
                let atom_ids = molecule
                    .as_ref()
                    .map(|mol| mol.atom_ids())
                    .unwrap_or_default();
                let mut pending_representation = None;

                let raw_input = egui_state.take_egui_input(&window);
                let output = egui_ctx.run(raw_input, |ctx| {
                    egui::Window::new("MolWeaver Status")
                        .default_pos(egui::pos2(10.0, 10.0))
                        .show(ctx, |ui| {
                            ui.label(format!("Atoms: {atom_count}"));
                            ui.label(format!("Bonds: {bond_count}"));
                            ui.label(format!("FPS: {:.1}", ui_state.fps));
                            ui.label(format!("File: {}", ui_state.file_name));
                            if let Some(selection) = ui_state.selection {
                                ui.label(format!("Selected: {}", selection.value()));
                            } else {
                                ui.label("Selected: none");
                            }
                        });

                    egui::Window::new("Edit")
                        .default_pos(egui::pos2(10.0, 220.0))
                        .show(ctx, |ui| {
                            ui.label("Representation");
                            let mut representation = ui_state.representation;
                            ui.horizontal(|ui| {
                                ui.radio_value(
                                    &mut representation,
                                    Representation::BallAndStick,
                                    "Ball & Stick",
                                );
                                ui.radio_value(
                                    &mut representation,
                                    Representation::SpaceFilling,
                                    "Space Filling",
                                );
                            });
                            if representation != ui_state.representation {
                                pending_representation = Some(representation);
                            }

                            ui.separator();
                            ui.label("Tool");
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut ui_state.tool, Tool::Select, "Select");
                                ui.radio_value(&mut ui_state.tool, Tool::AddAtom, "Add Atom");
                                ui.radio_value(&mut ui_state.tool, Tool::AddBond, "Add Bond");
                                ui.radio_value(&mut ui_state.tool, Tool::Move, "Move");
                            });

                            ui.separator();
                            ui.horizontal(|ui| {
                                let undo_clicked = ui
                                    .add_enabled(history.can_undo(), egui::Button::new("Undo"))
                                    .clicked();
                                let redo_clicked = ui
                                    .add_enabled(history.can_redo(), egui::Button::new("Redo"))
                                    .clicked();
                                if let Some(molecule_ref) = molecule.as_mut() {
                                    if undo_clicked {
                                        undo_command(
                                            &mut history,
                                            molecule_ref,
                                            &mut render_state,
                                            &mut ui_state,
                                        );
                                    }
                                    if redo_clicked {
                                        redo_command(
                                            &mut history,
                                            molecule_ref,
                                            &mut render_state,
                                            &mut ui_state,
                                        );
                                    }
                                }
                            });

                            ui.separator();
                            ui.label("Add Atom");
                            ui.horizontal(|ui| {
                                ui.label("Element:");
                                ui.text_edit_singleline(&mut ui_state.edit_element);
                            });
                            let add_clicked = ui
                                .add_enabled(molecule.is_some(), egui::Button::new("Insert Atom"))
                                .clicked();
                            if add_clicked {
                                if let Some(molecule_ref) = molecule.as_mut() {
                                    let position = if let Some(selection) = ui_state.selection {
                                        molecule_ref
                                            .get_atom(selection)
                                            .map(|atom| Vec3::from_array(atom.position))
                                            .unwrap_or(ui_state.camera.target)
                                            + Vec3::new(1.0, 0.0, 0.0)
                                    } else {
                                        let direction = (ui_state.camera.position()
                                            - ui_state.camera.target)
                                            .normalize_or_zero();
                                        ui_state.camera.target + direction * 1.5
                                    };
                                    let command = Command::InsertAtom {
                                        element: ui_state.edit_element.trim().to_string(),
                                        position: position.to_array(),
                                        atom_id: None,
                                        order_index: None,
                                    };
                                    apply_command(
                                        command,
                                        molecule_ref,
                                        &mut history,
                                        &mut render_state,
                                        &mut ui_state,
                                    );
                                }
                            }

                            ui.separator();
                            ui.label("Bond");
                            let mut bond_target = ui_state.bond_target;
                            egui::ComboBox::from_label("Bond target")
                                .selected_text(
                                    bond_target.map_or("None".into(), |id| id.value().to_string()),
                                )
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut bond_target, None, "None");
                                    for id in &atom_ids {
                                        ui.selectable_value(
                                            &mut bond_target,
                                            Some(*id),
                                            id.value().to_string(),
                                        );
                                    }
                                });
                            ui_state.bond_target = bond_target;
                            let add_bond_clicked = ui
                                .add_enabled(
                                    ui_state.selection.is_some() && ui_state.bond_target.is_some(),
                                    egui::Button::new("Add Bond"),
                                )
                                .clicked();
                            let remove_bond_clicked = ui
                                .add_enabled(
                                    ui_state.selection.is_some() && ui_state.bond_target.is_some(),
                                    egui::Button::new("Remove Bond"),
                                )
                                .clicked();
                            if let Some(molecule_ref) = molecule.as_mut() {
                                if let (Some(a), Some(b)) =
                                    (ui_state.selection, ui_state.bond_target)
                                {
                                    if add_bond_clicked {
                                        let command = Command::AddBond {
                                            atom_a: a,
                                            atom_b: b,
                                            bond_id: None,
                                        };
                                        apply_command(
                                            command,
                                            molecule_ref,
                                            &mut history,
                                            &mut render_state,
                                            &mut ui_state,
                                        );
                                    }
                                    if remove_bond_clicked {
                                        if let Some(bond_id) = molecule_ref.bond_between(a, b) {
                                            let command = Command::RemoveBond {
                                                bond_id,
                                                removed: None,
                                            };
                                            apply_command(
                                                command,
                                                molecule_ref,
                                                &mut history,
                                                &mut render_state,
                                                &mut ui_state,
                                            );
                                        } else {
                                            ui_state.status_message = "bond not found".to_string();
                                        }
                                    }
                                }
                            }

                            ui.separator();
                            ui.label("Move Atom");
                            ui.add(
                                egui::Slider::new(&mut ui_state.move_step, 0.05..=2.0).text("step"),
                            );
                            if let Some(molecule_ref) = molecule.as_mut() {
                                if let Some(selection) = ui_state.selection {
                                    let step = ui_state.move_step;
                                    if ui.button("+X").clicked() {
                                        apply_move(
                                            selection,
                                            Vec3::X * step,
                                            molecule_ref,
                                            &mut history,
                                            &mut render_state,
                                            &mut ui_state,
                                        );
                                    }
                                    if ui.button("-X").clicked() {
                                        apply_move(
                                            selection,
                                            -Vec3::X * step,
                                            molecule_ref,
                                            &mut history,
                                            &mut render_state,
                                            &mut ui_state,
                                        );
                                    }
                                    if ui.button("+Y").clicked() {
                                        apply_move(
                                            selection,
                                            Vec3::Y * step,
                                            molecule_ref,
                                            &mut history,
                                            &mut render_state,
                                            &mut ui_state,
                                        );
                                    }
                                    if ui.button("-Y").clicked() {
                                        apply_move(
                                            selection,
                                            -Vec3::Y * step,
                                            molecule_ref,
                                            &mut history,
                                            &mut render_state,
                                            &mut ui_state,
                                        );
                                    }
                                    if ui.button("+Z").clicked() {
                                        apply_move(
                                            selection,
                                            Vec3::Z * step,
                                            molecule_ref,
                                            &mut history,
                                            &mut render_state,
                                            &mut ui_state,
                                        );
                                    }
                                    if ui.button("-Z").clicked() {
                                        apply_move(
                                            selection,
                                            -Vec3::Z * step,
                                            molecule_ref,
                                            &mut history,
                                            &mut render_state,
                                            &mut ui_state,
                                        );
                                    }
                                } else {
                                    ui.label("Select an atom to move.");
                                }
                            }

                            if !ui_state.status_message.is_empty() {
                                ui.separator();
                                ui.label(format!("Status: {}", ui_state.status_message));
                            }
                        });
                });
                egui_state.handle_platform_output(&window, output.platform_output);
                if let Some(representation) = pending_representation {
                    ui_state.representation = representation;
                    if let Some(molecule_ref) = molecule.as_ref() {
                        render_state.set_representation(representation, molecule_ref);
                    }
                }
                let paint_jobs = egui_ctx.tessellate(output.shapes, output.pixels_per_point);
                let screen_descriptor = egui_wgpu::ScreenDescriptor {
                    size_in_pixels: [render_state.config.width, render_state.config.height],
                    pixels_per_point: output.pixels_per_point,
                };

                for (id, image_delta) in &output.textures_delta.set {
                    egui_renderer.update_texture(
                        &render_state.device,
                        &render_state.queue,
                        *id,
                        image_delta,
                    );
                }

                let render_result =
                    render_state.render(&mut egui_renderer, &paint_jobs, &screen_descriptor);
                match render_result {
                    Ok(()) => {}
                    Err(wgpu::SurfaceError::Lost) => render_state.resize(render_state.size),
                    Err(wgpu::SurfaceError::OutOfMemory) => target.exit(),
                    Err(wgpu::SurfaceError::Timeout) => {
                        std::thread::sleep(Duration::from_millis(16));
                    }
                    Err(wgpu::SurfaceError::Outdated) => {}
                }

                for id in &output.textures_delta.free {
                    egui_renderer.free_texture(id);
                }
            }
            _ => {}
        })
        .expect("event loop run");
}

fn handle_shortcuts(key: &Key, modifiers: &winit::keyboard::ModifiersState) -> bool {
    let ctrl_or_cmd = modifiers.control_key() || modifiers.super_key();
    if !ctrl_or_cmd {
        return false;
    }
    matches!(
        key,
        Key::Character(key) if key.eq_ignore_ascii_case("z") || key.eq_ignore_ascii_case("y")
    )
}

fn handle_click(
    picked: Option<AtomId>,
    render_state: &mut RenderState,
    ui_state: &mut UiState,
    molecule: Option<&mut Molecule>,
    history: &mut CommandHistory,
) {
    if let Some(picked_id) = picked {
        let previous = ui_state.selection;
        ui_state.selection = Some(picked_id);
        render_state.update_selection(previous, ui_state.selection);
    }

    if ui_state.tool == Tool::AddBond {
        if let (Some(picked_id), Some(molecule_ref)) = (picked, molecule) {
            match ui_state.bond_target {
                None => {
                    ui_state.bond_target = Some(picked_id);
                }
                Some(target_id) if target_id != picked_id => {
                    let command = Command::AddBond {
                        atom_a: target_id,
                        atom_b: picked_id,
                        bond_id: None,
                    };
                    apply_command(command, molecule_ref, history, render_state, ui_state);
                    ui_state.bond_target = None;
                }
                _ => {}
            }
        }
    }
}

fn apply_command(
    command: Command,
    molecule: &mut Molecule,
    history: &mut CommandHistory,
    render_state: &mut RenderState,
    ui_state: &mut UiState,
) {
    match history.execute(command, molecule) {
        Ok(applied) => {
            ui_state.status_message.clear();
            apply_render_delta(&applied, false, molecule, render_state, ui_state);
        }
        Err(err) => {
            ui_state.status_message = err;
        }
    }
}

fn undo_command(
    history: &mut CommandHistory,
    molecule: &mut Molecule,
    render_state: &mut RenderState,
    ui_state: &mut UiState,
) {
    match history.undo(molecule) {
        Ok(Some(command)) => {
            apply_render_delta(&command, true, molecule, render_state, ui_state);
        }
        Ok(None) => {}
        Err(err) => ui_state.status_message = err,
    }
}

fn redo_command(
    history: &mut CommandHistory,
    molecule: &mut Molecule,
    render_state: &mut RenderState,
    ui_state: &mut UiState,
) {
    match history.redo(molecule) {
        Ok(Some(command)) => {
            apply_render_delta(&command, false, molecule, render_state, ui_state);
        }
        Ok(None) => {}
        Err(err) => ui_state.status_message = err,
    }
}

fn apply_render_delta(
    command: &Command,
    is_undo: bool,
    molecule: &Molecule,
    render_state: &mut RenderState,
    ui_state: &mut UiState,
) {
    match command {
        Command::InsertAtom {
            element,
            position,
            atom_id: Some(atom_id),
            ..
        } => {
            if is_undo {
                render_state.remove_atom_instance(*atom_id);
                if ui_state.selection == Some(*atom_id) {
                    ui_state.selection = None;
                }
            } else {
                let atom = Atom {
                    id: *atom_id,
                    element: element.clone(),
                    position: *position,
                };
                render_state.add_atom_instance(&atom);
            }
        }
        Command::DeleteAtom { atom_id, removed } => {
            if is_undo {
                if let Some(removed) = removed {
                    render_state.add_atom_instance(&removed.atom);
                    render_state.update_bonds_for_atom(removed.atom.id, molecule);
                    if ui_state.selection.is_none() {
                        render_state.update_selection(None, Some(removed.atom.id));
                        ui_state.selection = Some(removed.atom.id);
                    }
                }
            } else {
                render_state.remove_atom_instance(*atom_id);
                if ui_state.selection == Some(*atom_id) {
                    ui_state.selection = None;
                }
            }
        }
        Command::MoveAtom { atom_id, from, to } => {
            let position = if is_undo { *from } else { *to };
            render_state.update_atom_position(*atom_id, position);
            render_state.update_bonds_for_atom(*atom_id, molecule);
        }
        Command::AddBond {
            bond_id: Some(bond_id),
            ..
        } => {
            if is_undo {
                render_state.remove_bond_instance(*bond_id);
            } else {
                render_state.add_bond_instance(*bond_id, molecule);
            }
        }
        Command::RemoveBond { bond_id, .. } => {
            if is_undo {
                render_state.add_bond_instance(*bond_id, molecule);
            } else {
                render_state.remove_bond_instance(*bond_id);
            }
        }
        _ => {}
    }
}

fn apply_move(
    atom_id: AtomId,
    delta: Vec3,
    molecule: &mut Molecule,
    history: &mut CommandHistory,
    render_state: &mut RenderState,
    ui_state: &mut UiState,
) {
    if let Some(atom) = molecule.get_atom(atom_id) {
        let from = atom.position;
        let to = (Vec3::from_array(atom.position) + delta).to_array();
        let command = Command::MoveAtom { atom_id, from, to };
        apply_command(command, molecule, history, render_state, ui_state);
    }
}
