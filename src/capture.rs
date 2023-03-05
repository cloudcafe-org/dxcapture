use std::sync::{
    Arc,
    Mutex
};
use std::thread;
use std::time::Duration;
use winapi::{
    shared::dxgiformat::{
        DXGI_FORMAT_B8G8R8A8_UNORM,
    },
    um::d3d11::{
        D3D11_CPU_ACCESS_READ,
        D3D11_MAP_READ,
        D3D11_USAGE_STAGING,
    }
};
use windows::{
    Graphics::{
        Capture::{
            Direct3D11CaptureFramePool,
            GraphicsCaptureSession,
        },
        DirectX::{
            Direct3D11::{
                IDirect3DSurface,
            },
            DirectXPixelFormat
        },
    },
    Win32::{
        Graphics::{
            Direct3D11::{
                ID3D11Device,
                ID3D11DeviceContext,
                ID3D11Texture2D,
                D3D11_TEXTURE2D_DESC,
            },
        },
    }
};

type FrameArrivedHandler =
    windows::Foundation::TypedEventHandler<Direct3D11CaptureFramePool, windows::core::IInspectable>;

use crate::d3d::*;


#[derive(Debug, PartialEq, thiserror::Error)]
pub enum CaptureError {
    // did't init or already closed.
    #[error("Capture is not active.")]
    NotActive,

    // async, so sometimes it's not there.
    #[error("No texture.")]
    NoTexture,

    #[error("CPU read access required.")]
    DeniedAccessCpuRead,

    #[error("Error:")]
    DirectxError(windows::core::Error),

    #[error("Error:")]
    OpencvError(String),

    #[error("Unsupported buffer type. Must be a staging buffer.")]
    UnsupportedBufferType,

    #[error("Unsupported pixel format.")]
    UnsupportedPixelFormat(u32),
}


#[derive(Clone, Debug, Default)]
pub struct RawFrameData {
    pub width: i32,
    pub height: i32,
    pub data: Vec<u8>,
}


#[derive(Clone, Debug)]
pub struct Capture {
    _d3d_device: ID3D11Device,
    d3d_context: ID3D11DeviceContext,
    frame_pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    _on_frame_arrived: FrameArrivedHandler,
    pub rx: crossbeam::channel::Receiver<ID3D11Texture2D>,
    active: bool,
}
impl Capture {
    pub fn new(device: &Device) -> anyhow::Result<Self> {
        let d3d_context = Device::get_immediate_context(&device.d3d_device)?;
        let item_size = device.item.Size()?;

        // Initialize the capture
        let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &device.device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            1,
            item_size,
        )?;
        let session = frame_pool.CreateCaptureSession(&device.item)?;

        // to thread safety
        let (tx, rx) = crossbeam::channel::unbounded();
        let on_frame_arrived = FrameArrivedHandler::new({
            move |frame_pool, _| {
                let frame = frame_pool.as_ref().unwrap().TryGetNextFrame()?;
                let surface = frame.Surface()?;

                let frame_texture = Device::from_direct3d_surface(&surface)?;

                tx.send(frame_texture).unwrap();
                Ok(())
            }
        });

        // Start the capture
        frame_pool.FrameArrived(on_frame_arrived.clone())?;
        session.StartCapture()?;

        Ok(Self {
            _d3d_device: device.d3d_device.clone(),
            d3d_context,
            frame_pool,
            session,
            _on_frame_arrived: on_frame_arrived,
            rx,
            active: true,
        })
    }

    fn release(&mut self) -> anyhow::Result<()> {
        self.active = false;
    
        // End the capture
        self.session.Close()?;
        self.frame_pool.Close()?;

        Ok(())
    }

}
impl Drop for Capture {
    fn drop(&mut self) {
        self.release().unwrap();
    }
}

#[cfg(feature = "img")]
pub mod img;
#[cfg(feature = "img")]
#[cfg_attr(feature = "docs-features", doc(cfg(feature = "img")))]
pub use img::ImgFrameData;

#[cfg(feature = "mat")]
pub mod mat;
#[cfg(feature = "mat")]
#[cfg_attr(feature = "docs-features", doc(cfg(feature = "mat")))]
pub use mat::MatFrameData;
