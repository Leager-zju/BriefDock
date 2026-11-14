//! Exercise the production Iced primitive without creating a window.
#[cfg(feature = "ui")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use futures::StreamExt;
    use iced::{
        Rectangle, Size,
        widget::shader::{Pipeline, Primitive, Viewport},
    };
    use lazybuddy_media_ipc::{
        receiver::Receiver,
        view::{Video, VideoPipeline},
    };
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("pipe required")?;
    let pid = args.next().ok_or("producer PID required")?.parse()?;
    let reference: Vec<u8> = args
        .next()
        .unwrap_or_else(|| "255,0,0,255".into())
        .split(',')
        .map(str::parse)
        .collect::<Result<_, _>>()?;
    if reference.len() != 4 {
        return Err("RGBA reference required".into());
    }
    let (_receiver, mut frames) = Receiver::start(path, pid);
    let (device, queue) = futures::executor::block_on(async {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12,
            ..Default::default()
        });
        let adapter = instance.request_adapter(&Default::default()).await?;
        Ok::<_, Box<dyn std::error::Error>>(adapter.request_device(&Default::default()).await?)
    })?;
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ] {
        let frame = futures::executor::block_on(frames.next()).ok_or("media ended")??;
        let mut pipeline = VideoPipeline::new(&device, &queue, format);
        let size = wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        };
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 16384,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        for opacity in [0., 0.5, 1.] {
            let video = Video {
                frame: frame.clone(),
                opacity,
            };
            video.prepare(
                &mut pipeline,
                &device,
                &queue,
                &Rectangle::with_size(Size::new(64., 64.)),
                &Viewport::with_physical_size(Size::new(64, 64), 1.),
            );
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                video.draw(&pipeline, &mut pass);
            }
            encoder.copy_texture_to_buffer(
                target.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(64),
                    },
                },
                size,
            );
            queue.submit([encoder.finish()]);
            let (sender, receiver) = std::sync::mpsc::channel();
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let _ = sender.send(result);
                });
            device.poll(wgpu::PollType::wait_indefinitely())?;
            receiver.recv_timeout(std::time::Duration::from_secs(5))??;
            {
                let bytes = buffer.slice(..).get_mapped_range();
                let expected: Vec<u8> = reference
                    .iter()
                    .enumerate()
                    .map(|(index, &channel)| {
                        let value = f32::from(channel) / 255.;
                        let result = if index == 3 {
                            opacity
                        } else if format.is_srgb() {
                            let linear = if value <= 0.04045 {
                                value / 12.92
                            } else {
                                ((value + 0.055) / 1.055).powf(2.4)
                            } * opacity;
                            if linear <= 0.0031308 {
                                linear * 12.92
                            } else {
                                1.055 * linear.powf(1. / 2.4) - 0.055
                            }
                        } else {
                            value * opacity
                        };
                        (result * 255.).round() as u8
                    })
                    .collect();
                let center = 32 * 256 + 32 * 4;
                if bytes[center..center + 4]
                    .iter()
                    .zip(&expected)
                    .any(|(actual, expected)| actual.abs_diff(*expected) > 5)
                {
                    return Err(format!(
                        "video primitive mismatch: {:?}, expected {expected:?}",
                        &bytes[center..center + 4]
                    )
                    .into());
                }
                if bytes[..4] != [0, 0, 0, 0] {
                    return Err("video aspect ratio was stretched into the letterbox".into());
                }
            }
            buffer.unmap();
            println!(
                "production_primitive_verified srgb={} opacity={opacity}",
                format.is_srgb()
            );
        }
    }
    Ok(())
}
#[cfg(not(feature = "ui"))]
fn main() {
    panic!("build with --features ui");
}
