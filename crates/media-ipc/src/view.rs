//! Iced presents an owned GPU copy; it never waits for decoder fences or maps pixels.
use crate::{gpu, receiver::Frame};
use iced::{
    Rectangle, mouse,
    widget::shader::{self, Pipeline, Primitive, Viewport},
};
use std::sync::{Arc, atomic::Ordering};

#[derive(Debug, Clone)]
pub struct Video {
    pub frame: Arc<Frame>,
    pub opacity: f32,
}
impl<Message> shader::Program<Message> for Video {
    type State = ();
    type Primitive = Self;
    fn draw(&self, _: &(), _: mouse::Cursor, _: Rectangle) -> Self {
        self.clone()
    }
}

pub struct VideoPipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    texture: Option<wgpu::Texture>,
    bind: Option<wgpu::BindGroup>,
    uniforms: wgpu::Buffer,
    srgb: bool,
    active: std::sync::Weak<Frame>,
}

impl Pipeline for VideoPipeline {
    fn new(device: &wgpu::Device, _: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("LazyBuddy shared video"),
            source: wgpu::ShaderSource::Wgsl(include_str!("video.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("LazyBuddy shared video"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            layout,
            sampler,
            texture: None,
            bind: None,
            uniforms,
            srgb: format.is_srgb(),
            active: std::sync::Weak::new(),
        }
    }

    fn trim(&mut self) {
        if self.active.upgrade().is_none() {
            self.bind = None;
            self.texture = None;
        }
    }
}

impl Primitive for Video {
    type Pipeline = VideoPipeline;
    fn prepare(
        &self,
        pipeline: &mut VideoPipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        _: &Viewport,
    ) {
        pipeline.active = Arc::downgrade(&self.frame);
        if !self.frame.uploaded.swap(true, Ordering::AcqRel) {
            match gpu::texture(device, &self.frame.handle) {
                Ok(source) => {
                    let size = source.size();
                    if pipeline
                        .texture
                        .as_ref()
                        .is_none_or(|texture| texture.size() != size)
                    {
                        let texture = device.create_texture(&wgpu::TextureDescriptor {
                            label: Some("LazyBuddy retained video frame"),
                            size,
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            usage: wgpu::TextureUsages::COPY_DST
                                | wgpu::TextureUsages::TEXTURE_BINDING,
                            view_formats: &[],
                        });
                        let view = texture.create_view(&Default::default());
                        pipeline.bind =
                            Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                                label: None,
                                layout: &pipeline.layout,
                                entries: &[
                                    wgpu::BindGroupEntry {
                                        binding: 0,
                                        resource: wgpu::BindingResource::TextureView(&view),
                                    },
                                    wgpu::BindGroupEntry {
                                        binding: 1,
                                        resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
                                    },
                                    wgpu::BindGroupEntry {
                                        binding: 2,
                                        resource: pipeline.uniforms.as_entire_binding(),
                                    },
                                ],
                            }));
                        pipeline.texture = Some(texture);
                    }
                    let mut encoder = device.create_command_encoder(&Default::default());
                    encoder.copy_texture_to_texture(
                        source.as_image_copy(),
                        pipeline.texture.as_ref().unwrap().as_image_copy(),
                        size,
                    );
                    queue.submit([encoder.finish()]);
                    let frame = self.frame.clone();
                    queue.on_submitted_work_done(move || frame.release());
                }
                Err(_) => self.frame.failed(),
            }
        }
        let aspect = pipeline
            .texture
            .as_ref()
            .map(|texture| texture.width() as f32 / texture.height() as f32)
            .unwrap_or(1.);
        let target = bounds.width.max(1.) / bounds.height.max(1.);
        let values = [
            self.opacity.clamp(0., 1.),
            if pipeline.srgb { 1. } else { 0. },
            (aspect / target).min(1.),
            (target / aspect).min(1.),
        ];
        let bytes: Vec<u8> = values.into_iter().flat_map(f32::to_ne_bytes).collect();
        queue.write_buffer(&pipeline.uniforms, 0, &bytes);
    }
    fn draw(&self, pipeline: &VideoPipeline, pass: &mut wgpu::RenderPass<'_>) -> bool {
        if let Some(bind) = &pipeline.bind {
            pass.set_pipeline(&pipeline.pipeline);
            pass.set_bind_group(0, bind, &[]);
            pass.draw(0..6, 0..1);
        }
        true
    }
}
