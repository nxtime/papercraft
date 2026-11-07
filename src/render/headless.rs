//! A wgpu device without a window: tests, screenshots and PNG export.

use super::gpu::{CanvasRenderer, OFFSCREEN_FORMAT};
use super::scene::CanvasFrame;

/// A device without a window, for tests, screenshots and PNG export.
pub struct Headless {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter_info: wgpu::AdapterInfo,
}

impl Headless {
    pub fn new() -> Result<Headless, String> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        // No hardware adapter (a server, CI): retry with the software one.
        let request = |fallback| {
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: fallback,
                compatible_surface: None,
                ..Default::default()
            }))
        };
        let adapter = request(false)
            .or_else(|_| request(true))
            .map_err(|e| format!("no GPU adapter available: {e}"))?;
        let adapter_info = adapter.get_info();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("papercraft headless"),
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .map_err(|e| format!("failed to create device: {e}"))?;
        Ok(Headless {
            device,
            queue,
            adapter_info,
        })
    }

    /// Create a render target texture of `size` physical px.
    pub fn target(&self, size: [u32; 2]) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: OFFSCREEN_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }

    /// Copy `tex` into a mappable buffer and submit; returns the buffer and its row stride.
    fn start_readback(&self, tex: &wgpu::Texture) -> (wgpu::Buffer, u32) {
        let (w, h) = (tex.width(), tex.height());
        let row = (w * 4).div_ceil(256) * 256;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([enc.finish()]);
        (buf, row)
    }

    /// Strip the row padding off a mapped readback buffer.
    fn unpad(buf: &wgpu::Buffer, row: u32, w: u32, h: u32) -> image::RgbaImage {
        let data = buf
            .slice(..)
            .get_mapped_range()
            .expect("mapped readback buffer");
        let mut img = image::RgbaImage::new(w, h);
        for y in 0..h {
            let src = &data[(y * row) as usize..(y * row + w * 4) as usize];
            img.as_mut()[(y * w * 4) as usize..((y + 1) * w * 4) as usize].copy_from_slice(src);
        }
        img
    }

    /// Copy a texture back to the CPU as RGBA8.
    pub fn read(&self, tex: &wgpu::Texture) -> image::RgbaImage {
        let (buf, row) = self.start_readback(tex);
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
        Self::unpad(&buf, row, tex.width(), tex.height())
    }

    /// [`Headless::read`] without blocking: the browser cannot wait on the GPU, so the
    /// buffer is mapped with a callback and the caller awaits it (the JS event loop drives it).
    pub async fn read_async(&self, tex: &wgpu::Texture) -> Result<image::RgbaImage, String> {
        let (buf, row) = self.start_readback(tex);
        let (tx, rx) = oneshot();
        buf.slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r));
        // WebGPU resolves the mapping from the JS event loop; the WebGL backend only delivers
        // callbacks when polled, so nudge it (never blocks).
        let _ = self.device.poll(wgpu::PollType::Poll);
        rx.await.map_err(|e| format!("GPU readback failed: {e}"))?;
        Ok(Self::unpad(&buf, row, tex.width(), tex.height()))
    }

    /// Draw a canvas frame into a new offscreen texture (not yet read back).
    fn draw_frame(
        &self,
        frame: &CanvasFrame,
        size: [u32; 2],
        scale: f32,
        clear: [f64; 4],
    ) -> wgpu::Texture {
        let tex = self.target(size);
        let view = tex.create_view(&Default::default());
        let mut r = CanvasRenderer::new(&self.device, OFFSCREEN_FORMAT);
        let mut enc = self.device.create_command_encoder(&Default::default());
        r.render(
            &self.device,
            &self.queue,
            &mut enc,
            &view,
            size,
            scale,
            frame,
            clear,
        );
        self.queue.submit([enc.finish()]);
        tex
    }

    /// Render a canvas frame into a new image of `size` physical px.
    pub fn render_frame(
        &self,
        frame: &CanvasFrame,
        size: [u32; 2],
        scale: f32,
        clear: [f64; 4],
    ) -> image::RgbaImage {
        self.read(&self.draw_frame(frame, size, scale, clear))
    }

    /// [`Headless::render_frame`] for the browser (see [`Headless::read_async`]).
    pub async fn render_frame_async(
        &self,
        frame: &CanvasFrame,
        size: [u32; 2],
        scale: f32,
        clear: [f64; 4],
    ) -> Result<image::RgbaImage, String> {
        self.read_async(&self.draw_frame(frame, size, scale, clear))
            .await
    }
}

/// Minimal one-shot channel: wgpu's map callback on one end, an awaiting task on the other.
/// (No async runtime exists in the browser build, and `tokio::sync::oneshot` is native-only.)
struct Slot<T> {
    value: Option<T>,
    waker: Option<std::task::Waker>,
}

struct Sender<T>(std::sync::Arc<std::sync::Mutex<Slot<T>>>);
struct Receiver<T>(std::sync::Arc<std::sync::Mutex<Slot<T>>>);

fn oneshot<T>() -> (Sender<T>, Receiver<T>) {
    let slot = std::sync::Arc::new(std::sync::Mutex::new(Slot {
        value: None,
        waker: None,
    }));
    (Sender(slot.clone()), Receiver(slot))
}

impl<T> Sender<T> {
    fn send(self, v: T) {
        let mut s = self.0.lock().expect("oneshot lock");
        s.value = Some(v);
        if let Some(w) = s.waker.take() {
            w.wake();
        }
    }
}

impl<T> std::future::Future for Receiver<T> {
    type Output = T;
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<T> {
        let mut s = self.0.lock().expect("oneshot lock");
        match s.value.take() {
            Some(v) => std::task::Poll::Ready(v),
            None => {
                s.waker = Some(cx.waker().clone());
                std::task::Poll::Pending
            }
        }
    }
}
