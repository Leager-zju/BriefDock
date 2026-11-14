//! Shared GPU resources imported through Windows APIs, without GStreamer linkage.
use std::{
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
};
use windows::Win32::{
    Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, LUID, WAIT_OBJECT_0},
    Graphics::{
        Direct3D::D3D_FEATURE_LEVEL_11_0,
        Direct3D12::*,
        Dxgi::{CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory4},
    },
    System::Threading::{
        CreateEventW, GetCurrentProcess, OpenProcess, PROCESS_DUP_HANDLE, WaitForSingleObject,
    },
};

pub struct Importer {
    device: ID3D12Device,
    producer: OwnedHandle,
    fence: ID3D12Fence,
}

impl Importer {
    #[cfg(feature = "wgpu")]
    pub fn for_wgpu(
        device: &wgpu::Device,
        pid: u32,
        fence: u64,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let raw =
            unsafe { device.as_hal::<wgpu::hal::api::Dx12>() }.ok_or("D3D12 renderer required")?;
        Self::from_device(raw.raw_device().clone(), pid, fence)
    }

    /// First compare the supplied PID with the application's owned media child.
    pub fn new(pid: u32, luid: i64, fence: u64) -> Result<Self, Box<dyn std::error::Error>> {
        unsafe {
            let factory: IDXGIFactory4 = CreateDXGIFactory1()?;
            let adapter: IDXGIAdapter1 = factory.EnumAdapterByLuid(LUID {
                LowPart: luid as u32,
                HighPart: (luid >> 32) as i32,
            })?;
            let mut device: Option<ID3D12Device> = None;
            D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_11_0, &mut device)?;
            Self::from_device(device.ok_or("D3D12 device missing")?, pid, fence)
        }
    }

    pub fn from_device(
        device: ID3D12Device,
        pid: u32,
        fence_handle: u64,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        unsafe {
            let producer =
                OwnedHandle::from_raw_handle(OpenProcess(PROCESS_DUP_HANDLE, false, pid)?.0);
            let handle = duplicate(&producer, fence_handle)?;
            let mut fence: Option<ID3D12Fence> = None;
            device.OpenSharedHandle(HANDLE(handle.as_raw_handle()), &mut fence)?;
            Ok(Self {
                device,
                producer,
                fence: fence.ok_or("shared fence missing")?,
            })
        }
    }

    /// Called off the GUI thread; rendering never waits for decoding here.
    pub fn open_texture(
        &self,
        server_handle: u64,
        fence_value: u64,
    ) -> Result<ID3D12Resource, Box<dyn std::error::Error>> {
        let handle = self.receive_handle(server_handle, fence_value)?;
        unsafe {
            let mut resource: Option<ID3D12Resource> = None;
            self.device
                .OpenSharedHandle(HANDLE(handle.as_raw_handle()), &mut resource)?;
            resource.ok_or_else(|| "shared texture missing".into())
        }
    }

    pub fn receive_handle(
        &self,
        server_handle: u64,
        fence_value: u64,
    ) -> Result<OwnedHandle, Box<dyn std::error::Error>> {
        unsafe {
            let handle = duplicate(&self.producer, server_handle)?;
            if self.fence.GetCompletedValue() < fence_value {
                let event = OwnedHandle::from_raw_handle(CreateEventW(None, false, false, None)?.0);
                self.fence
                    .SetEventOnCompletion(fence_value, HANDLE(event.as_raw_handle()))?;
                if WaitForSingleObject(HANDLE(event.as_raw_handle()), 5000) != WAIT_OBJECT_0 {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "shared frame fence timeout",
                    )
                    .into());
                }
            }
            Ok(handle)
        }
    }
}

#[cfg(feature = "wgpu")]
pub fn texture(
    device: &wgpu::Device,
    handle: &OwnedHandle,
) -> Result<wgpu::Texture, Box<dyn std::error::Error>> {
    unsafe {
        let raw = device
            .as_hal::<wgpu::hal::api::Dx12>()
            .ok_or("D3D12 renderer required")?;
        let mut resource: Option<ID3D12Resource> = None;
        raw.raw_device()
            .OpenSharedHandle(HANDLE(handle.as_raw_handle()), &mut resource)?;
        let resource = resource.ok_or("shared texture missing")?;
        let description = resource.GetDesc();
        if description.Width == 0
            || description.Width > 16384
            || description.Height == 0
            || description.Height > 16384
            || description.Format
                != windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM
            || description.SampleDesc.Count != 1
            || description.DepthOrArraySize != 1
        {
            return Err("unsupported shared texture".into());
        }
        let size = wgpu::Extent3d {
            width: description.Width as u32,
            height: description.Height,
            depth_or_array_layers: 1,
        };
        let raw = wgpu::hal::dx12::Device::texture_from_raw(
            resource,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureDimension::D2,
            size,
            1,
            1,
        );
        Ok(device.create_texture_from_hal::<wgpu::hal::api::Dx12>(
            raw,
            &wgpu::TextureDescriptor {
                label: Some("private decoder texture"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            },
        ))
    }
}

unsafe fn duplicate(producer: &OwnedHandle, handle: u64) -> windows::core::Result<OwnedHandle> {
    let mut local = HANDLE::default();
    unsafe {
        DuplicateHandle(
            HANDLE(producer.as_raw_handle()),
            HANDLE(handle as usize as *mut _),
            GetCurrentProcess(),
            &mut local,
            0,
            false,
            DUPLICATE_SAME_ACCESS,
        )?;
        Ok(OwnedHandle::from_raw_handle(local.0))
    }
}
