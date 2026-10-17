//! VIDEO: plays a file with a variable-speed playhead (reverse included).
//!
//! Same split as the camera: a background thread owns FFmpeg (demuxer +
//! decoder) and hands frames to the render loop, which only uploads them.
//! Frames travel as tightly packed YUV 4:2:0 (1.5 bytes/pixel instead of 4
//! for RGBA); the conversion to RGB happens on the GPU (`gpu::Yuv420Pipeline`).
//!
//! Memory stays bounded: decoded frames live in a per-video cache with a
//! byte budget, evicting whatever is farthest from the playhead. Random
//! access (reverse, scrubbing, big jumps) seeks to the keyframe before the
//! wanted frame and decodes forward from there, keeping a window of frames
//! behind the playhead so playing backwards is served from the cache; the
//! next window is prepared before the current one runs out.
//!
//! Future option (documented, not implemented): transcode on import to an
//! intra-only proxy (e.g. MJPEG, in memory or on disk) so every frame is a
//! keyframe — cheaper scrubbing on long-GOP files at the cost of an import
//! step.

use crate::dlog;
use crate::gpu::{ensure_plane, ensure_target, uniform_buffer, write_plane, VideoTex, Yuv420Pipeline};
use crate::modules::param::Param;
use crate::ModuleChrome;
use ffmpeg_next as ffmpeg;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use wgpu_upstream as wgpu;

/// Decoded-frame cache per video. 1080p 4:2:0 is ~3 MB a frame, so this
/// holds ~85 of them — enough for a typical GOP to play backwards smoothly.
pub const CACHE_BUDGET: usize = 256 * 1024 * 1024;
/// Taller videos are scaled down while decoding (4K would otherwise bring
/// the camera's bottleneck back: 12 MB a frame to convert and upload).
pub const MAX_HEIGHT: u32 = 1080;
/// A jump forward of more than this many frames seeks instead of decoding
/// everything in between.
const SEEK_AHEAD: i64 = 48;
/// Frames kept behind the playhead while playing forwards.
const KEEP_BEHIND_FORWARD: i64 = 6;
/// Frames decoded ahead of the playhead while idle.
const PREFETCH_AHEAD: i64 = 6;

pub const SPEED_MAX: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoopMode {
    #[default]
    Loop,
    /// Bounces between the ends.
    PingPong,
    /// Stops at the end (or the start, playing backwards).
    Once,
}

impl LoopMode {
    pub const ALL: [LoopMode; 3] = [LoopMode::Loop, LoopMode::PingPong, LoopMode::Once];

    pub fn name(self) -> &'static str {
        match self {
            LoopMode::Loop => "Loop",
            LoopMode::PingPong => "Ping-pong",
            LoopMode::Once => "Once",
        }
    }
}

/// One decoded frame: Y, then U, then V, each tightly packed.
pub struct VideoFrame {
    pub index: i64,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl VideoFrame {
    fn chroma_size(&self) -> (u32, u32) {
        (self.width.div_ceil(2), self.height.div_ceil(2))
    }

    pub fn planes(&self) -> [&[u8]; 3] {
        let y = (self.width * self.height) as usize;
        let (cw, ch) = self.chroma_size();
        let c = (cw * ch) as usize;
        [&self.data[..y], &self.data[y..y + c], &self.data[y + c..y + 2 * c]]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub frames: i64,
    pub duration: f64,
    /// Full-range (JPEG) levels rather than video's 16..235.
    pub full_range: bool,
    /// BT.709 colors (HD) rather than BT.601 (SD).
    pub bt709: bool,
}

#[derive(Default)]
struct Shared {
    /// The frame the playhead is on, and which way it's moving (-1, 0, 1).
    wanted: i64,
    direction: i8,
    ready: Option<Arc<VideoFrame>>,
    info: Option<VideoInfo>,
    error: Option<String>,
}

/// Owns the decoding thread for one file, plus its GPU textures.
pub struct VideoWorker {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    thread: thread::Thread,
    uploaded: Option<i64>,
    planes: [Option<VideoTex>; 3],
    out: Option<VideoTex>,
    uniform: Option<wgpu::Buffer>,
}

impl VideoWorker {
    pub fn spawn(path: PathBuf) -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (shared_handle, stop_handle) = (shared.clone(), stop.clone());
        let handle = thread::spawn(move || {
            let name = path.file_name().map_or_else(|| "video".into(), |n| n.to_string_lossy().to_string());
            match Player::open(&path, CACHE_BUDGET) {
                Ok(player) => {
                    let info = player.info;
                    dlog!(
                        "{name}: {}x{} @ {:.2} fps, {} frames, {}",
                        info.width,
                        info.height,
                        info.fps,
                        info.frames,
                        if info.bt709 { "BT.709" } else { "BT.601" }
                    );
                    shared_handle.lock().unwrap().info = Some(info);
                    run(player, &shared_handle, &stop_handle, &name);
                }
                Err(err) => {
                    dlog!("{name}: {err}");
                    shared_handle.lock().unwrap().error = Some(err);
                }
            }
        });
        VideoWorker { shared, stop, thread: handle.thread().clone(), uploaded: None, planes: [None, None, None], out: None, uniform: None }
    }

    pub fn info(&self) -> Option<VideoInfo> {
        self.shared.lock().unwrap().info
    }

    pub fn error(&self) -> Option<String> {
        self.shared.lock().unwrap().error.clone()
    }

    /// Points the decoder at `frame` (and tells it which way we're going,
    /// so it can prepare what comes next).
    pub fn request(&self, frame: i64, direction: i8) {
        let mut shared = self.shared.lock().unwrap();
        if shared.wanted != frame || shared.direction != direction {
            shared.wanted = frame;
            shared.direction = direction;
            drop(shared);
            self.thread.unpark();
        }
    }

    /// Uploads the newest decoded frame if it changed and records its YUV ->
    /// RGB pass into `encoder`. Returns the RGB texture (the last frame
    /// stays up while the decoder catches up).
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &Yuv420Pipeline,
    ) -> Option<VideoTex> {
        let (frame, info) = {
            let shared = self.shared.lock().unwrap();
            (shared.ready.clone(), shared.info)
        };
        let (Some(frame), Some(info)) = (frame, info) else { return self.out.clone() };
        if self.uploaded == Some(frame.index) && self.out.is_some() {
            return self.out.clone();
        }
        let (cw, ch) = frame.chroma_size();
        let sizes = [(frame.width, frame.height), (cw, ch), (cw, ch)];
        let planes = frame.planes();
        for (i, (w, h)) in sizes.into_iter().enumerate() {
            let tex = ensure_plane(device, &mut self.planes[i], w, h);
            write_plane(queue, &tex, planes[i]);
        }
        let out = ensure_target(device, &mut self.out, frame.width, frame.height);
        let uniform = self.uniform.get_or_insert_with(|| uniform_buffer(device, 16));
        let [y, u, v] = &self.planes;
        let planes = [y.as_ref()?, u.as_ref()?, v.as_ref()?];
        pipeline.record(device, queue, encoder, uniform, planes, &out, info.full_range, info.bt709);
        self.uploaded = Some(frame.index);
        Some(out)
    }
}

impl Drop for VideoWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.unpark();
    }
}

/// The decoding thread: serve the wanted frame, then use idle time to
/// prepare the frames the playhead is heading to.
fn run(mut player: Player, shared: &Mutex<Shared>, stop: &AtomicBool, name: &str) {
    let mut served: Option<i64> = None;
    let mut stats = Stats::new();
    while !stop.load(Ordering::Relaxed) {
        let (wanted, direction) = {
            let s = shared.lock().unwrap();
            (s.wanted.clamp(0, (player.info.frames - 1).max(0)), s.direction)
        };
        if served != Some(wanted) {
            let t = Instant::now();
            let frame = player.get(wanted, direction);
            stats.serve(t.elapsed(), player.cache.contains_key(&wanted));
            if let Some(frame) = frame {
                shared.lock().unwrap().ready = Some(frame);
            }
            served = Some(wanted);
        } else if !player.prefetch(wanted, direction) {
            thread::park_timeout(Duration::from_millis(10));
        }
        stats.report_every_second(name, &player);
    }
}

struct Player {
    info: VideoInfo,
    input: ffmpeg::format::context::Input,
    stream: usize,
    decoder: ffmpeg::decoder::Video,
    /// Seconds per stream timestamp tick, and the stream's first timestamp.
    time_base: f64,
    start: i64,
    scaler: Option<ffmpeg::software::scaling::Context>,
    /// The index the decoder will produce next if we keep reading, when
    /// known (it isn't right after a seek).
    next: Option<i64>,
    eof: bool,
    cache: BTreeMap<i64, Arc<VideoFrame>>,
    cache_bytes: usize,
    budget: usize,
    seeks: u32,
    decoded: u32,
}

impl Player {
    fn open(path: &Path, budget: usize) -> Result<Self, String> {
        ffmpeg::init().map_err(|e| e.to_string())?;
        let input = ffmpeg::format::input(path).map_err(|e| format!("can't open: {e}"))?;
        let stream = input.streams().best(ffmpeg::media::Type::Video).ok_or("no video stream")?;
        let stream_index = stream.index();
        let time_base = f64::from(stream.time_base());
        let start = if stream.start_time() == ffmpeg::ffi::AV_NOPTS_VALUE { 0 } else { stream.start_time() };
        let rate = stream.avg_frame_rate();
        let fps = if rate.denominator() != 0 && f64::from(rate) > 0.0 { f64::from(rate) } else { f64::from(stream.rate()).max(1.0) };
        let duration = if stream.duration() > 0 {
            stream.duration() as f64 * time_base
        } else {
            input.duration().max(0) as f64 / f64::from(ffmpeg::ffi::AV_TIME_BASE)
        };
        let frames = if stream.frames() > 0 { stream.frames() } else { (duration * fps).round() as i64 }.max(1);

        let mut context = ffmpeg::codec::context::Context::from_parameters(stream.parameters()).map_err(|e| e.to_string())?;
        // Let FFmpeg use every core (frame + slice threads).
        context.set_threading(ffmpeg::codec::threading::Config {
            kind: ffmpeg::codec::threading::Type::Frame,
            count: 0,
            ..Default::default()
        });
        let decoder = context.decoder().video().map_err(|e| format!("no decoder: {e}"))?;
        let (width, height) = output_size(decoder.width(), decoder.height());
        let full_range = decoder.color_range() == ffmpeg::color::Range::JPEG
            || matches!(decoder.format(), ffmpeg::format::Pixel::YUVJ420P | ffmpeg::format::Pixel::YUVJ422P | ffmpeg::format::Pixel::YUVJ444P);
        let bt709 = match decoder.color_space() {
            ffmpeg::color::Space::BT709 => true,
            ffmpeg::color::Space::BT470BG | ffmpeg::color::Space::SMPTE170M => false,
            _ => decoder.height() >= 720,
        };
        Ok(Player {
            info: VideoInfo { width, height, fps, frames, duration, full_range, bt709 },
            input,
            stream: stream_index,
            decoder,
            time_base,
            start,
            scaler: None,
            next: Some(0),
            eof: false,
            cache: BTreeMap::new(),
            cache_bytes: 0,
            budget,
            seeks: 0,
            decoded: 0,
        })
    }

    fn frame_bytes(&self) -> usize {
        let (w, h) = (self.info.width as usize, self.info.height as usize);
        w * h + 2 * w.div_ceil(2) * h.div_ceil(2)
    }

    /// How many frames fit in the cache.
    fn budget_frames(&self) -> i64 {
        (self.budget / self.frame_bytes().max(1)).max(2) as i64
    }

    /// The frame at `index` (or the nearest one the file actually has).
    fn get(&mut self, index: i64, direction: i8) -> Option<Arc<VideoFrame>> {
        if let Some(f) = self.cache.get(&index) {
            return Some(f.clone());
        }
        // Playing backwards keeps as much behind the target as fits, so the
        // next requests (index-1, index-2, ...) are already decoded.
        let behind = if direction < 0 { self.budget_frames() - PREFETCH_AHEAD } else { KEEP_BEHIND_FORWARD };
        self.decode_to(index, behind);
        self.evict(index);
        self.nearest(index)
    }

    /// Decodes up to `index`, keeping frames from `index - behind` on.
    fn decode_to(&mut self, index: i64, behind: i64) {
        let close_ahead = self.next.is_some_and(|n| n <= index && index - n <= SEEK_AHEAD) && !self.eof;
        if !close_ahead {
            self.seek(index);
        }
        let keep_from = index - behind.max(0);
        let mut decoded = ffmpeg::frame::Video::empty();
        loop {
            match self.decode_next(&mut decoded) {
                Some(i) => {
                    self.next = Some(i + 1);
                    if i >= keep_from && !self.cache.contains_key(&i) {
                        if let Some(frame) = self.convert(&decoded, i) {
                            self.cache_bytes += frame.data.len();
                            self.cache.insert(i, Arc::new(frame));
                        }
                    }
                    if i >= index {
                        break;
                    }
                }
                None => {
                    self.eof = true;
                    break;
                }
            }
        }
    }

    /// Seeks the demuxer to the keyframe at or before frame `index`.
    fn seek(&mut self, index: i64) {
        let seconds = index as f64 / self.info.fps + self.start as f64 * self.time_base;
        let ts = (seconds * f64::from(ffmpeg::ffi::AV_TIME_BASE)) as i64;
        let _ = self.input.seek(ts, ..ts);
        self.decoder.flush();
        self.next = None;
        self.eof = false;
        self.seeks += 1;
    }

    /// The next decoded frame's index, reading packets as needed; `None` at
    /// the end of the file.
    fn decode_next(&mut self, frame: &mut ffmpeg::frame::Video) -> Option<i64> {
        loop {
            if self.decoder.receive_frame(frame).is_ok() {
                self.decoded += 1;
                let ts = frame.timestamp().unwrap_or(0);
                return Some((((ts - self.start) as f64 * self.time_base) * self.info.fps).round() as i64);
            }
            if self.eof {
                return None;
            }
            match self.input.packets().next() {
                Some((stream, packet)) => {
                    if stream.index() == self.stream {
                        let _ = self.decoder.send_packet(&packet);
                    }
                }
                None => {
                    let _ = self.decoder.send_eof();
                    self.eof = true;
                    // Drain whatever the decoder still holds.
                    if self.decoder.receive_frame(frame).is_ok() {
                        self.decoded += 1;
                        let ts = frame.timestamp().unwrap_or(0);
                        return Some((((ts - self.start) as f64 * self.time_base) * self.info.fps).round() as i64);
                    }
                    return None;
                }
            }
        }
    }

    /// Packs a decoded frame as tight YUV 4:2:0 at the output size,
    /// converting/scaling with swscale only when the format or size differs.
    fn convert(&mut self, decoded: &ffmpeg::frame::Video, index: i64) -> Option<VideoFrame> {
        use ffmpeg::format::Pixel;
        let (w, h) = (self.info.width, self.info.height);
        let direct = matches!(decoded.format(), Pixel::YUV420P | Pixel::YUVJ420P) && (decoded.width(), decoded.height()) == (w, h);
        let scaled;
        let src = if direct {
            decoded
        } else {
            if self.scaler.is_none() {
                self.scaler = ffmpeg::software::scaling::Context::get(
                    decoded.format(),
                    decoded.width(),
                    decoded.height(),
                    Pixel::YUV420P,
                    w,
                    h,
                    ffmpeg::software::scaling::Flags::BILINEAR,
                )
                .ok();
            }
            let mut out = ffmpeg::frame::Video::empty();
            self.scaler.as_mut()?.run(decoded, &mut out).ok()?;
            scaled = out;
            &scaled
        };
        let mut data = Vec::with_capacity(self.frame_bytes());
        for plane in 0..3 {
            let (pw, ph) = if plane == 0 { (w, h) } else { (w.div_ceil(2), h.div_ceil(2)) };
            let stride = src.stride(plane);
            let bytes = src.data(plane);
            for row in 0..ph as usize {
                data.extend_from_slice(&bytes[row * stride..row * stride + pw as usize]);
            }
        }
        Some(VideoFrame { index, width: w, height: h, data })
    }

    /// Drops the frames farthest from `index` until the cache fits its budget.
    fn evict(&mut self, index: i64) {
        while self.cache_bytes > self.budget && self.cache.len() > 1 {
            let (&first, _) = self.cache.first_key_value().unwrap();
            let (&last, _) = self.cache.last_key_value().unwrap();
            let victim = if (index - first).abs() >= (last - index).abs() { first } else { last };
            if victim == index {
                break;
            }
            if let Some(f) = self.cache.remove(&victim) {
                self.cache_bytes -= f.data.len();
            }
        }
    }

    /// The cached frame closest to `index`, preferring the one before it
    /// (files with gaps, or past the last frame).
    fn nearest(&self, index: i64) -> Option<Arc<VideoFrame>> {
        self.cache
            .range(..=index)
            .next_back()
            .or_else(|| self.cache.range(index..).next())
            .map(|(_, f)| f.clone())
    }

    /// Idle work: decode a little of what's coming. Returns whether it did anything.
    fn prefetch(&mut self, index: i64, direction: i8) -> bool {
        if direction > 0 {
            let target = (index + PREFETCH_AHEAD).min(self.info.frames - 1);
            if self.cache.contains_key(&target) || self.eof {
                return false;
            }
            self.decode_to(target, KEEP_BEHIND_FORWARD + PREFETCH_AHEAD);
            self.evict(index);
            true
        } else if direction < 0 {
            // The contiguous run of cached frames ending at `index`: when the
            // playhead nears its start, prepare the window before it.
            let mut low = index;
            while low > 0 && self.cache.contains_key(&(low - 1)) {
                low -= 1;
            }
            if low == 0 || index - low > PREFETCH_AHEAD * 2 {
                return false;
            }
            // Decode the window that ends just before `low`, keeping `index`'s
            // run: evict around the playhead, not around the new window.
            let window = (self.budget_frames() - (index - low) - 1).max(1);
            self.decode_to(low - 1, window);
            self.evict(index);
            true
        } else {
            false
        }
    }
}

/// The decode size: the source, scaled down to `MAX_HEIGHT` if taller
/// (even dimensions, so the chroma planes divide evenly).
fn output_size(width: u32, height: u32) -> (u32, u32) {
    let (w, h) = if height > MAX_HEIGHT {
        ((width as f64 * MAX_HEIGHT as f64 / height as f64).round() as u32, MAX_HEIGHT)
    } else {
        (width, height)
    };
    ((w.max(2) + 1) & !1, (h.max(2) + 1) & !1)
}

/// Per-second decode numbers for the debug log.
struct Stats {
    since: Instant,
    served: u32,
    misses: u32,
    time: Duration,
}

impl Stats {
    fn new() -> Self {
        Stats { since: Instant::now(), served: 0, misses: 0, time: Duration::ZERO }
    }

    fn serve(&mut self, took: Duration, hit_after: bool) {
        self.served += 1;
        self.time += took;
        if !hit_after {
            self.misses += 1;
        }
    }

    fn report_every_second(&mut self, name: &str, player: &Player) {
        if self.since.elapsed() < Duration::from_secs(1) {
            return;
        }
        if self.served > 0 || player.decoded > 0 {
            dlog!(
                "{name}: {} frames shown, {:.1}ms avg wait, {} decoded, {} seeks, cache {} frames / {:.0} MB",
                self.served,
                self.time.as_secs_f32() * 1000.0 / self.served.max(1) as f32,
                player.decoded,
                player.seeks,
                player.cache.len(),
                player.cache_bytes as f32 / 1048576.0
            );
        }
        *self = Stats::new();
    }
}

/// The VIDEO module's config: which file, the playhead and how it moves.
pub struct VideoState {
    pub chrome: ModuleChrome,
    pub path: Option<PathBuf>,
    pub worker: Option<VideoWorker>,
    /// Playback speed; negative plays backwards. A cable's 0..1 maps onto
    /// the box's range (-4..4 by default).
    pub speed: Param,
    pub playing: bool,
    pub looping: LoopMode,
    /// Playhead, in seconds.
    pub position: f64,
    /// +1 or -1: which way Ping-pong is currently going.
    pub bounce: f64,
    pub dropdown_open: bool,
    pub info: Option<VideoInfo>,
}

impl VideoState {
    pub fn new() -> Self {
        VideoState {
            chrome: ModuleChrome::new(),
            path: None,
            worker: None,
            speed: Param::ranged(1.0, -SPEED_MAX, SPEED_MAX),
            playing: true,
            looping: LoopMode::Loop,
            position: 0.0,
            bounce: 1.0,
            dropdown_open: false,
            info: None,
        }
    }

    pub fn file_name(&self) -> Option<String> {
        self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string())
    }

    pub fn open(&mut self, path: PathBuf) {
        self.path = Some(path);
        self.worker = None;
        self.info = None;
        self.position = 0.0;
        self.bounce = 1.0;
        self.chrome.preview_texture = None;
    }

    pub fn duration(&self) -> f64 {
        self.info.map_or(0.0, |i| i.duration)
    }

    /// Advances the playhead by `dt` seconds at `speed`, or puts it where a
    /// wired scrub signal (0..1) says. Then tells the decoder.
    pub fn advance(&mut self, dt: f64, speed: f32, scrub: Option<f32>) {
        if let Some(info) = self.worker.as_ref().and_then(|w| w.info()) {
            self.info = Some(info);
        }
        let Some(info) = self.info else { return };
        let duration = info.duration.max(1.0 / info.fps);
        let mut direction = 0i8;
        if let Some(x) = scrub {
            let target = (x.clamp(0.0, 1.0) as f64) * duration;
            direction = (target - self.position).signum() as i8;
            self.position = target;
        } else if self.playing && speed != 0.0 {
            let velocity = speed as f64 * self.bounce;
            direction = velocity.signum() as i8;
            let mut p = self.position + velocity * dt;
            match self.looping {
                LoopMode::Loop => p = p.rem_euclid(duration),
                LoopMode::Once => {
                    if !(0.0..duration).contains(&p) {
                        p = p.clamp(0.0, duration - 1.0 / info.fps);
                        self.playing = false;
                    }
                }
                LoopMode::PingPong => {
                    if p >= duration {
                        p = 2.0 * duration - p;
                        self.bounce = -self.bounce;
                    } else if p < 0.0 {
                        p = -p;
                        self.bounce = -self.bounce;
                    }
                    p = p.clamp(0.0, duration - 1.0 / info.fps);
                }
            }
            self.position = p;
        }
        if let Some(worker) = &self.worker {
            let frame = ((self.position * info.fps) as i64).clamp(0, info.frames - 1);
            worker.request(frame, direction);
        }
    }

    /// Jumps the playhead to `t` (0..1 of the clip) from the scrub bar.
    pub fn seek_fraction(&mut self, t: f32) {
        self.position = t.clamp(0.0, 1.0) as f64 * self.duration();
    }
}

/// "1:05.3"
pub fn fmt_time(seconds: f64) -> String {
    let s = seconds.max(0.0);
    format!("{}:{:04.1}", (s / 60.0) as u32, s % 60.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_size_caps_height_and_keeps_even() {
        assert_eq!(output_size(1920, 1080), (1920, 1080));
        assert_eq!(output_size(3840, 2160), (1920, 1080));
        assert_eq!(output_size(641, 361), (642, 362));
    }

    #[test]
    fn time_format() {
        assert_eq!(fmt_time(65.34), "1:05.3");
        assert_eq!(fmt_time(0.0), "0:00.0");
    }

    /// Encodes a small long-GOP test clip. Frame `i` has a known gray level
    /// in its top-left block (the rest stays mid-gray, so the encoder sees
    /// no scene cuts and really uses P-frames), identifying decoded frames.
    fn write_test_clip(path: &Path, frames: i64) {
        use ffmpeg::{codec, encoder, format, frame, Rational};
        ffmpeg::init().unwrap();
        let mut output = format::output(path).unwrap();
        let codec = encoder::find(codec::Id::MPEG2VIDEO).unwrap();
        let mut stream = output.add_stream(codec).unwrap();
        let mut enc = codec::context::Context::new_with_codec(codec).encoder().video().unwrap();
        let (w, h) = (64u32, 48u32);
        enc.set_width(w);
        enc.set_height(h);
        enc.set_format(format::Pixel::YUV420P);
        enc.set_time_base(Rational(1, 25));
        enc.set_frame_rate(Some(Rational(25, 1)));
        enc.set_gop(12);
        enc.set_max_b_frames(0);
        enc.set_global_quality(1);
        let mut enc = enc.open_as(codec).unwrap();
        stream.set_parameters(&enc);
        stream.set_time_base(Rational(1, 25));
        stream.set_avg_frame_rate(Rational(25, 1));
        output.write_header().unwrap();
        let stream_tb = output.stream(0).unwrap().time_base();
        let mut write_packets = |enc: &mut encoder::Video, output: &mut format::context::Output| {
            let mut packet = codec::packet::Packet::empty();
            while enc.receive_packet(&mut packet).is_ok() {
                packet.set_stream(0);
                packet.rescale_ts(Rational(1, 25), stream_tb);
                packet.write_interleaved(output).unwrap();
            }
        };
        for i in 0..frames {
            let mut f = frame::Video::new(format::Pixel::YUV420P, w, h);
            for plane in 0..3 {
                f.data_mut(plane).fill(128);
            }
            let stride = f.stride(0);
            let y = f.data_mut(0);
            for row in 0..BLOCK {
                y[row * stride..row * stride + BLOCK].fill(gray(i));
            }
            f.set_pts(Some(i));
            enc.send_frame(&f).unwrap();
            write_packets(&mut enc, &mut output);
        }
        enc.send_eof().unwrap();
        write_packets(&mut enc, &mut output);
        output.write_trailer().unwrap();
    }

    fn gray(i: i64) -> u8 {
        (20 + (i * 5) % 210) as u8
    }

    const BLOCK: usize = 16;

    /// Average luma of the identifying block.
    fn luma(frame: &VideoFrame) -> u8 {
        let y = frame.planes()[0];
        let w = frame.width as usize;
        let sum: u32 = (2..BLOCK - 2).flat_map(|r| (2..BLOCK - 2).map(move |c| y[r * w + c] as u32)).sum();
        (sum / ((BLOCK - 4) * (BLOCK - 4)) as u32) as u8
    }

    #[test]
    fn test_clip_really_has_gops() {
        let path = clip("gops", 60);
        let mut input = ffmpeg::format::input(&path).unwrap();
        let keys = input.packets().filter(|(_, p)| p.is_key()).count();
        assert!((4..=8).contains(&keys), "{keys} keyframes in 60 frames");
        let _ = std::fs::remove_file(path);
    }

    fn clip(name: &str, frames: i64) -> PathBuf {
        clip_as(name, frames, "mkv")
    }

    fn clip_as(name: &str, frames: i64, ext: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("nvm-test-{}-{name}.{ext}", std::process::id()));
        write_test_clip(&path, frames);
        path
    }

    #[test]
    fn forward_backward_and_random_access_hit_the_right_frames() {
        access_test(clip("access", 60));
    }

    #[test]
    fn mov_container_too() {
        access_test(clip_as("access-mov", 60, "mov"));
    }

    fn access_test(path: PathBuf) {
        let mut player = Player::open(&path, CACHE_BUDGET).unwrap();
        assert_eq!(player.info.frames, 60);
        assert!((player.info.fps - 25.0).abs() < 0.01);
        let check = |player: &mut Player, i: i64, dir: i8| {
            let f = player.get(i, dir).unwrap();
            assert_eq!(f.index, i, "asked for {i}");
            assert!((luma(&f) as i32 - gray(i) as i32).abs() <= 3, "frame {i}: luma {} vs {}", luma(&f), gray(i));
        };
        for i in 0..30 {
            check(&mut player, i, 1);
        }
        for i in (0..59).rev() {
            check(&mut player, i, -1);
        }
        for i in [45, 3, 58, 17, 17, 0] {
            check(&mut player, i, 0);
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn cache_stays_within_budget_in_reverse() {
        let path = clip("budget", 60);
        // Room for ~10 frames only.
        let frame = 64 * 48 * 3 / 2;
        let mut player = Player::open(&path, frame * 10).unwrap();
        for i in (0..60).rev() {
            let f = player.get(i, -1).unwrap();
            assert_eq!(f.index, i);
            player.prefetch(i, -1);
            assert!(player.cache_bytes <= frame * 10, "cache {} bytes at frame {i}", player.cache_bytes);
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reverse_playback_seeks_once_per_gop_not_per_frame() {
        let path = clip("windows", 60);
        let mut player = Player::open(&path, CACHE_BUDGET).unwrap();
        let before = player.seeks;
        for i in (0..60).rev() {
            player.get(i, -1);
            player.prefetch(i, -1);
        }
        // 60 frames in GOPs of 12: about one seek per GOP.
        assert!(player.seeks - before <= 7, "{} seeks", player.seeks - before);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn playhead_loop_modes() {
        let mut v = VideoState::new();
        v.info = Some(VideoInfo { width: 2, height: 2, fps: 10.0, frames: 20, duration: 2.0, full_range: false, bt709: false });
        v.advance(2.5, 1.0, None);
        assert!((v.position - 0.5).abs() < 1e-9, "loop wraps: {}", v.position);
        v.looping = LoopMode::PingPong;
        v.position = 1.5;
        v.advance(1.0, 1.0, None);
        assert!((v.position - 1.5).abs() < 1e-9 && v.bounce < 0.0, "bounces: {} {}", v.position, v.bounce);
        v.looping = LoopMode::Once;
        v.bounce = 1.0;
        v.position = 1.5;
        v.advance(1.0, 1.0, None);
        assert!(!v.playing);
        v.advance(0.0, 1.0, Some(0.25));
        assert!((v.position - 0.5).abs() < 1e-9);
        v.playing = true;
        v.looping = LoopMode::Loop;
        v.position = 0.2;
        v.advance(0.5, -1.0, None);
        assert!((v.position - 1.7).abs() < 1e-9, "reverse wraps: {}", v.position);
    }
}
