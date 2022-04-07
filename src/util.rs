use crate::tessellator::{
    artifacts::TessellationData,
    types::{GpuPrimitive, GpuTransform, GpuVertex},
};

use super::types::{BufferState, GpuGlobals, SceneGlobals};

use wgpu::{include_wgsl, util::DeviceExt, RenderPipeline};
use winit::dpi::PhysicalSize;

pub const WINDOW_SIZE: f32 = 800.0;
pub const MSAA_SAMPLES: u32 = 4;
// These mush match the uniform buffer sizes in the vertex shader.
pub fn get_globals(svg_contents: &String) -> SceneGlobals {
    let opt = usvg::Options::default();
    let content: &[u8] = svg_contents.as_bytes();
    let rtree = usvg::Tree::from_data(content, &opt.to_ref()).unwrap();
    let view_box = rtree.svg_node().view_box;

    let vb_width = view_box.rect.size().width() as f32;
    let vb_height = view_box.rect.size().height() as f32;
    let scale = vb_width / vb_height;

    let (width, height) = if scale < 1.0 {
        (WINDOW_SIZE, WINDOW_SIZE * scale)
    } else {
        (WINDOW_SIZE, WINDOW_SIZE / scale)
    };

    let pan = [vb_width / -2.0, vb_height / -2.0];
    let zoom = 2.0 / f32::max(vb_width, vb_height);
    let scene = SceneGlobals {
        zoom,
        pan,
        window_size: PhysicalSize::new(width as u32, height as u32),
        wireframe: false,
        size_changed: true,
    };
    scene
}

pub fn build_pipeline(
    device: &wgpu::Device,
    buffers: &BufferState,
    wireframe: bool,
) -> RenderPipeline {
    // Get shaders
    let vert_module =
        device.create_shader_module(&include_wgsl!("shaders/vert.wgsl"));
    let frag_module =
        device.create_shader_module(&include_wgsl!("shaders/frag.wgsl"));

    // Make pipeline layout
    let pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[&buffers.bind_group_layout],
            push_constant_ranges: &[],
            label: None,
        });

    let render_pipeline_descriptor = wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &vert_module,
            entry_point: "main",
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<GpuVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        offset: 0,
                        format: wgpu::VertexFormat::Float32x2,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        offset: 8,
                        format: wgpu::VertexFormat::Uint32,
                        shader_location: 1,
                    },
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &frag_module,
            entry_point: "main",
            targets: &[wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Bgra8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            }],
        }),
        primitive: wgpu::PrimitiveState {
            topology: match wireframe {
                true => wgpu::PrimitiveTopology::LineList,
                false => wgpu::PrimitiveTopology::TriangleList,
            },
            polygon_mode: wgpu::PolygonMode::Fill,
            front_face: wgpu::FrontFace::Ccw,
            strip_index_format: None,
            cull_mode: None,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: MSAA_SAMPLES,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
    };

    device.create_render_pipeline(&render_pipeline_descriptor)
}

pub fn build_buffers(
    device: &wgpu::Device,
    data: &TessellationData,
) -> BufferState {
    // Create vertex buffer object
    let vbo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&data.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    // Create index buffer object
    let ibo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&data.indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    let prim_buffer_byte_size =
        (data.primitives.len() * std::mem::size_of::<GpuPrimitive>()) as u64;
    let transform_buffer_byte_size =
        (data.transforms.len() * std::mem::size_of::<GpuTransform>()) as u64;
    let globals_buffer_byte_size = std::mem::size_of::<GpuGlobals>() as u64;

    let prims_ssbo = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Prims ssbo"),
        size: prim_buffer_byte_size,
        usage: wgpu::BufferUsages::VERTEX
            | wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let transforms_ssbo = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Transforms ssbo"),
        size: transform_buffer_byte_size,
        usage: wgpu::BufferUsages::VERTEX
            | wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let globals_ubo = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Globals ubo"),
        size: globals_buffer_byte_size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            globals_buffer_byte_size,
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage {
                            read_only: true,
                        },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            prim_buffer_byte_size,
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage {
                            read_only: true,
                        },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            transform_buffer_byte_size,
                        ),
                    },
                    count: None,
                },
            ],
        });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Bind group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(
                    globals_ubo.as_entire_buffer_binding(),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Buffer(
                    prims_ssbo.as_entire_buffer_binding(),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Buffer(
                    transforms_ssbo.as_entire_buffer_binding(),
                ),
            },
        ],
    });

    BufferState {
        primitives: data.primitives.len() as u64,
        transforms: data.transforms.len() as u64,
        ibo,
        vbo,
        prims_ssbo,
        transforms_ssbo,
        globals_ubo,
        bind_group_layout,
        bind_group,
    }
}
