//! Connect only to an explicitly supplied private producer. Prints no OS handles.
use lazybuddy_media_ipc::{
    Packet, finish, frame_received, read_packet, release_frame, request_frame,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("private named pipe required")?;
    let expected_pid: u32 = args
        .next()
        .ok_or("expected producer PID required")?
        .parse()?;
    #[cfg(feature = "wgpu")]
    let expected: [u8; 4] = if let Some(value) = args.next() {
        value
            .split(',')
            .map(str::parse)
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .map_err(|_| "four reference channels required")?
    } else {
        [255, 0, 0, 255]
    };
    if !path.starts_with(r"\\.\pipe\lazybuddy-media-") {
        return Err("unexpected pipe namespace".into());
    }
    let mut pipe = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    let Packet::Config {
        pid,
        caps,
        adapter_luid,
        fence_handle,
    } = read_packet(&mut pipe)?
    else {
        return Err("configuration required".into());
    };
    if pid != expected_pid {
        return Err("producer identity mismatch".into());
    }
    #[cfg(all(feature = "gpu", not(feature = "wgpu")))]
    let gpu = lazybuddy_media_ipc::gpu::Importer::new(pid, adapter_luid, fence_handle)?;
    #[cfg(feature = "wgpu")]
    let (device, queue) = futures::executor::block_on(async {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12,
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        Ok::<_, Box<dyn std::error::Error>>(
            adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await?,
        )
    })?;
    #[cfg(feature = "wgpu")]
    let gpu = lazybuddy_media_ipc::gpu::Importer::for_wgpu(&device, pid, fence_handle)?;
    #[cfg(feature = "wgpu")]
    let _ = adapter_luid;
    #[cfg(not(feature = "gpu"))]
    let _ = (adapter_luid, fence_handle);
    println!(
        "configuration_received format_rgba={}",
        caps.contains("RGBA")
    );
    for _ in 0..3 {
        request_frame(&mut pipe)?;
        match read_packet(&mut pipe)? {
            Packet::Frame {
                texture_handle,
                pitch,
                pts_ns,
                fence_value,
                ..
            } => {
                frame_received(&mut pipe)?;
                #[cfg(feature = "gpu")]
                {
                    let texture = gpu.open_texture(texture_handle, fence_value)?;
                    let description = unsafe { texture.GetDesc() };
                    println!(
                        "texture_opened width={} height={}",
                        description.Width, description.Height
                    );
                    #[cfg(feature = "wgpu")]
                    read_pixels(
                        &device,
                        &queue,
                        texture,
                        description.Width as u32,
                        description.Height,
                        expected,
                    )?;
                }
                #[cfg(not(feature = "gpu"))]
                let _ = fence_value;
                release_frame(&mut pipe, texture_handle)?;
                println!("frame_metadata_received pitch={pitch} pts_ns={pts_ns}");
            }
            Packet::Eos => break,
            _ => return Err("unexpected producer packet".into()),
        }
    }
    finish(&mut pipe)?;
    Ok(())
}

#[cfg(feature = "wgpu")]
fn read_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    resource: windows::Win32::Graphics::Direct3D12::ID3D12Resource,
    width: u32,
    height: u32,
    expected: [u8; 4],
) -> Result<(), Box<dyn std::error::Error>> {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let raw = unsafe {
        wgpu::hal::dx12::Device::texture_from_raw(
            resource,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureDimension::D2,
            size,
            1,
            1,
        )
    };
    let texture = unsafe {
        device.create_texture_from_hal::<wgpu::hal::api::Dx12>(
            raw,
            &wgpu::TextureDescriptor {
                label: Some("private shared frame probe"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
        )
    };
    let pitch = (width * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(pitch) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pitch),
                rows_per_image: Some(height),
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
        if bytes[..4]
            .iter()
            .zip(expected)
            .any(|(actual, expected)| actual.abs_diff(expected) > 5)
        {
            return Err(format!(
                "shared texture pixels differ from independent reference: {:?}",
                &bytes[..4]
            )
            .into());
        }
    }
    buffer.unmap();
    println!("shared_texture_pixels_verified");
    Ok(())
}
