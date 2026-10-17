use crate::gpu::{ensure_target, uniform_buffer, BlendPipeline, VideoTex};
use crate::ModuleChrome;
use wgpu_upstream as wgpu;

/// The BLEND module's own config: just the 0..1 mix between its two inputs.
pub struct BlendState {
    pub chrome: ModuleChrome,
    pub mix: f32,
    uniform: Option<wgpu::Buffer>,
    target: Option<VideoTex>,
}

impl BlendState {
    pub fn new() -> Self {
        BlendState {
            chrome: ModuleChrome::new(),
            mix: 0.5,
            uniform: None,
            target: None,
        }
    }

    /// 0 = all of `a`, 1 = all of `b`, in between a per-pixel mix done on the
    /// GPU. If only one side has a signal it passes through as-is (no pass,
    /// no copy). The output takes `a`'s resolution; `b` is resampled.
    pub fn record(
        &mut self,
        pipeline: &BlendPipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        a: Option<VideoTex>,
        b: Option<VideoTex>,
        mix: f32,
    ) -> Option<VideoTex> {
        let (a, b) = match (a, b) {
            (None, None) => return None,
            (Some(only), None) | (None, Some(only)) => return Some(only),
            (Some(a), Some(b)) => (a, b),
        };
        let uniform = self.uniform.get_or_insert_with(|| uniform_buffer(device, 16));
        let mut bytes = [0u8; 16];
        bytes[..4].copy_from_slice(&mix.clamp(0.0, 1.0).to_ne_bytes());
        queue.write_buffer(uniform, 0, &bytes);

        let target = ensure_target(device, &mut self.target, a.width, a.height);
        pipeline.record(device, encoder, uniform, &a, &b, &target);
        Some(target)
    }
}
