use std::time::{Duration, Instant};

use wgpu::util::DeviceExt;

const CURSOR_WIDTH: f32 = 2.0;
const CURSOR_COLOR: [f32; 4] = [0.9, 0.9, 0.95, 1.0];
const BLINK_INTERVAL: Duration = Duration::from_millis(530);

const UNIT_QUAD: [[f32; 2]; 6] = [
    [0.0, 0.0],
    [1.0, 0.0],
    [0.0, 1.0],
    [1.0, 0.0],
    [1.0, 1.0],
    [0.0, 1.0],
];

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CursorUniform {
    rect: [f32; 4],
    color: [f32; 4],
    screen_size: [f32; 2],
    _padding: [f32; 2],
}

const SHADER_SOURCE: &str = r#"
struct CursorUniform {
    rect: vec4<f32>,
    color: vec4<f32>,
    screen_size: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> u: CursorUniform;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(@location(0) unit_pos: vec2<f32>) -> VertexOutput {
    let pixel_pos = u.rect.xy + unit_pos * u.rect.zw;
    let ndc_x = (pixel_pos.x / u.screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel_pos.y / u.screen_size.y) * 2.0;
    var out: VertexOutput;
    out.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.color = u.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

/// 点滅するカーソルを単色矩形として描画するための最小パイプライン。
pub struct CursorLayer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    rect: [f32; 4],
    last_blink: Instant,
    visible: bool,
}

impl CursorLayer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("menfis cursor shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SOURCE.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("menfis cursor bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("menfis cursor pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("menfis cursor pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x2,
                    }],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("menfis cursor vertex buffer"),
            contents: bytemuck::cast_slice(&UNIT_QUAD),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("menfis cursor uniform buffer"),
            size: std::mem::size_of::<CursorUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("menfis cursor bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            vertex_buffer,
            uniform_buffer,
            bind_group,
            rect: [0.0, 0.0, CURSOR_WIDTH, 0.0],
            last_blink: Instant::now(),
            visible: true,
        }
    }

    /// カーソルの左上位置と高さ(pixel)を設定する。編集・移動のたびに呼び出し、
    /// 点滅を「表示中」にリセットして視認しやすくする。
    pub fn set_position(&mut self, x: f32, top: f32, height: f32) {
        self.rect = [x, top, CURSOR_WIDTH, height];
        self.visible = true;
        self.last_blink = Instant::now();
    }

    fn update_blink(&mut self) {
        if self.last_blink.elapsed() >= BLINK_INTERVAL {
            self.visible = !self.visible;
            self.last_blink = Instant::now();
        }
    }

    pub fn prepare(&mut self, queue: &wgpu::Queue, screen_width: f32, screen_height: f32) {
        self.update_blink();

        let uniform = CursorUniform {
            rect: self.rect,
            color: CURSOR_COLOR,
            screen_size: [screen_width, screen_height],
            _padding: [0.0, 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    pub fn render<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        if !self.visible {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..UNIT_QUAD.len() as u32, 0..1);
    }
}
