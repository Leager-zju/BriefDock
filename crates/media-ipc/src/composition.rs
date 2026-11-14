//! D3D12 shared frames below a transparent WebView, using stable DirectComposition.
use crate::{gpu, receiver::Frame};
use std::{io, time::Duration};
use windows::{
    Win32::{Foundation::HWND, Graphics::DirectComposition::*},
    core::{IUnknown, Interface},
};

type Error = Box<dyn std::error::Error + Send + Sync>;
fn fail(message: &str) -> Error {
    io::Error::other(message).into()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
impl Layout {
    pub fn valid(self) -> bool {
        self.width > 0 && self.height > 0 && self.width <= 8192 && self.height <= 8192
    }
}
fn aspect(source: (u32, u32), target: (u32, u32)) -> [f32; 2] {
    let source = source.0 as f32 / source.1 as f32;
    let target = target.0 as f32 / target.1 as f32;
    [(source / target).min(1.), (target / source).min(1.)]
}
pub struct Compositor {
    // Surface and visual are retained together; a window must outlive this object.
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    composition: IDCompositionDevice,
    target: IDCompositionTarget,
    visual: IDCompositionVisual,
    pipeline: wgpu::RenderPipeline,
    bind_layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    sampler: wgpu::Sampler,
    texture: Option<wgpu::Texture>,
    bind: Option<wgpu::BindGroup>,
    layout: Layout,
    pub frames: u64,
    pub last_pts_ns: u64,
}
impl Compositor {
    /// The caller keeps its owned HWND alive until this compositor is dropped.
    pub unsafe fn new(hwnd: usize, adapter_luid: i64, layout: Layout) -> Result<Self, Error> {
        if !layout.valid() {
            return Err(fail("invalid video layout"));
        }
        let composition: IDCompositionDevice =
            unsafe { DCompositionCreateDevice2(None::<&IUnknown>) }?;
        let target = unsafe { composition.CreateTargetForHwnd(HWND(hwnd as *mut _), false) }?;
        let visual = unsafe { composition.CreateVisual() }?;
        unsafe {
            target.SetRoot(&visual)?;
        }
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12,
            // The receiver already owns a D3D12 device. Enabling the debug layer
            // afterwards removes existing devices, including the shared importer.
            flags: wgpu::InstanceFlags::empty(),
            ..Default::default()
        });
        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CompositionVisual(
                visual.as_raw(),
            ))
        }?;
        let adapters = instance.enumerate_adapters(wgpu::Backends::DX12);
        let adapter = adapters
            .into_iter()
            .find(|adapter| {
                let Some(raw) = (unsafe { adapter.as_hal::<wgpu::hal::api::Dx12>() }) else {
                    return false;
                };
                let Ok(desc) = (unsafe { raw.raw_adapter().GetDesc1() }) else {
                    return false;
                };
                (i64::from(desc.AdapterLuid.HighPart) << 32) | i64::from(desc.AdapterLuid.LowPart)
                    == adapter_luid
            })
            .ok_or_else(|| fail("decoder GPU unavailable"))?;
        let (device, queue) = futures::executor::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor::default()),
        )?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format: wgpu::TextureFormat::Bgra8Unorm,
            width: layout.width,
            height: layout.height,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::PreMultiplied,
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        // The visual variant installs the swapchain without owning our composition commit.
        unsafe {
            visual.SetOffsetX2(layout.x as f32)?;
            visual.SetOffsetY2(layout.y as f32)?;
            composition.Commit()?;
        }
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("native video"),
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
            bind_group_layouts: &[&bind_layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(include_str!("video.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("native video composition"),
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
                    format: config.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
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
        Ok(Self {
            surface,
            device,
            queue,
            config,
            composition,
            target,
            visual,
            pipeline,
            bind_layout,
            uniform,
            sampler,
            texture: None,
            bind: None,
            layout,
            frames: 0,
            last_pts_ns: 0,
        })
    }
    pub fn size(&self) -> Option<(u32, u32)> {
        self.texture.as_ref().map(|t| (t.width(), t.height()))
    }
    pub fn layout(&mut self, layout: Layout) -> Result<(), Error> {
        if !layout.valid() {
            return Err(fail("invalid video layout"));
        }
        if layout.width != self.config.width || layout.height != self.config.height {
            self.config.width = layout.width;
            self.config.height = layout.height;
            self.surface.configure(&self.device, &self.config);
        }
        self.layout = layout;
        unsafe {
            self.visual.SetOffsetX2(layout.x as f32)?;
            self.visual.SetOffsetY2(layout.y as f32)?;
            self.composition.Commit()?;
        }
        Ok(())
    }
    pub fn visible(&self, visible: bool) -> Result<(), Error> {
        unsafe {
            self.target
                .SetRoot(if visible { Some(&self.visual) } else { None })?;
            self.composition.Commit()?;
        }
        Ok(())
    }
    pub fn frame(&mut self, frame: &Frame) -> Result<(), Error> {
        let copied = (|| {
            let source = gpu::texture(&self.device, &frame.handle)
                .map_err(|_| fail("shared frame import failed"))?;
            let size = source.size();
            if size.width > self.device.limits().max_texture_dimension_2d
                || size.height > self.device.limits().max_texture_dimension_2d
            {
                return Err(fail("decoded dimensions unsupported"));
            }
            if self.texture.as_ref().is_none_or(|t| t.size() != size) {
                let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("retained native frame"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let view = texture.create_view(&Default::default());
                self.bind = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &self.bind_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: self.uniform.as_entire_binding(),
                        },
                    ],
                }));
                self.texture = Some(texture);
            }
            let mut encoder = self.device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_texture(
                source.as_image_copy(),
                self.texture.as_ref().unwrap().as_image_copy(),
                size,
            );
            let submission = self.queue.submit([encoder.finish()]);
            self.device.poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(5)),
            })?;
            self.last_pts_ns = frame.pts_ns;
            self.frames += 1;
            Ok::<_, Error>(())
        })();
        if copied.is_ok() {
            frame.release();
        } else {
            frame.failed();
        }
        copied
    }
    /// Readback is opt-in for synthetic verification; normal playback never maps pixels.
    pub fn present(&self, opacity: f32, probe: bool) -> Result<Option<[u8; 4]>, Error> {
        if !opacity.is_finite() || !(0. ..=1.).contains(&opacity) {
            return Err(fail("invalid video opacity"));
        }
        let Some(bind) = &self.bind else {
            return Ok(None);
        };
        let scale = aspect(
            self.size().unwrap(),
            (self.layout.width, self.layout.height),
        );
        let values = [opacity, 0., scale[0], scale[1]];
        let bytes: Vec<u8> = values.into_iter().flat_map(f32::to_ne_bytes).collect();
        self.queue.write_buffer(&self.uniform, 0, &bytes);
        let target = self.surface.get_current_texture()?;
        let view = target.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.,
                            g: 0.,
                            b: 0.,
                            a: opacity as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bind, &[]);
            pass.draw(0..6, 0..1);
        }
        let buffer = probe.then(|| {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("explicit synthetic pixel probe"),
                size: 256,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            })
        });
        if let Some(buffer) = &buffer {
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &target.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: self.layout.width / 2,
                        y: self.layout.height / 2,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(1),
                    },
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
        }
        let submission = self.queue.submit([encoder.finish()]);
        self.device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(5)),
        })?;
        let pixel = if let Some(buffer) = buffer {
            let (sender, receiver) = std::sync::mpsc::channel();
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let _ = sender.send(result);
                });
            self.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(5)),
            })?;
            receiver.recv_timeout(Duration::from_secs(5))??;
            let mapped = buffer.slice(..).get_mapped_range();
            let value = [mapped[2], mapped[1], mapped[0], mapped[3]];
            drop(mapped);
            buffer.unmap();
            Some(value)
        } else {
            None
        };
        target.present();
        Ok(pixel)
    }
}
impl Drop for Compositor {
    fn drop(&mut self) {
        unsafe {
            let _ = self.target.SetRoot(None::<&IDCompositionVisual>);
            let _ = self.composition.Commit();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aspect_preserves_source_without_stretching() {
        assert_eq!(aspect((1920, 1080), (1000, 1000)), [1., 0.5625]);
        assert_eq!(aspect((100, 100), (200, 100)), [0.5, 1.]);
    }
    #[test]
    fn dimensions_are_bounded_before_surface_configuration() {
        assert!(
            !Layout {
                x: 0,
                y: 0,
                width: 0,
                height: 100
            }
            .valid()
        );
        assert!(
            !Layout {
                x: 0,
                y: 0,
                width: 9000,
                height: 100
            }
            .valid()
        );
        assert!(
            Layout {
                x: 0,
                y: 62,
                width: 780,
                height: 400
            }
            .valid()
        );
    }
}
