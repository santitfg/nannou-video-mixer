use crate::gpu::{ensure_target, uniform_buffer, BlendPipeline, VideoTex};
use crate::modules::param::Param;
use crate::ModuleChrome;
use serde::{Deserialize, Serialize};
use wgpu_upstream as wgpu;

/// How B goes over A. The order is the `mode` index in `gpu::BLEND_FS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlendMode {
    #[default]
    Mix,
    Add,
    Subtract,
    Multiply,
    Screen,
    Overlay,
    Difference,
    Lighten,
    Darken,
    LumaKey,
    Wipe,
    Dissolve,
}

impl BlendMode {
    pub const ALL: [BlendMode; 12] = [
        BlendMode::Mix,
        BlendMode::Add,
        BlendMode::Subtract,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Difference,
        BlendMode::Lighten,
        BlendMode::Darken,
        BlendMode::LumaKey,
        BlendMode::Wipe,
        BlendMode::Dissolve,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BlendMode::Mix => "Mix",
            BlendMode::Add => "Add",
            BlendMode::Subtract => "Subtract",
            BlendMode::Multiply => "Multiply",
            BlendMode::Screen => "Screen",
            BlendMode::Overlay => "Overlay",
            BlendMode::Difference => "Difference",
            BlendMode::Lighten => "Lighten",
            BlendMode::Darken => "Darken",
            BlendMode::LumaKey => "Luma key",
            BlendMode::Wipe => "Wipe",
            BlendMode::Dissolve => "Dissolve",
        }
    }

    fn index(self) -> f32 {
        BlendMode::ALL.iter().position(|m| *m == self).unwrap_or(0) as f32
    }
}

/// The BLEND module's own config: the mix between its two inputs (0..1 by
/// default; its range and clamping are configurable like any number box).
pub struct BlendState {
    pub chrome: ModuleChrome,
    pub mix: Param,
    pub mode: BlendMode,
    pub dropdown_open: bool,
    uniform: Option<wgpu::Buffer>,
    target: Option<VideoTex>,
}

impl BlendState {
    pub fn new() -> Self {
        BlendState {
            chrome: ModuleChrome::new(),
            mix: Param::unit(0.5),
            mode: BlendMode::Mix,
            dropdown_open: false,
            uniform: None,
            target: None,
        }
    }

    /// Blends `b` over `a` in `self.mode` on the GPU; `mix` is how much of
    /// `b` (0 = all `a`, 1 = full effect). If only one side has a signal it passes through as-is (no pass,
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
        bytes[4..8].copy_from_slice(&self.mode.index().to_ne_bytes());
        queue.write_buffer(uniform, 0, &bytes);

        let target = ensure_target(device, &mut self.target, a.width, a.height);
        pipeline.record(device, encoder, uniform, &a, &b, &target);
        Some(target)
    }
}
