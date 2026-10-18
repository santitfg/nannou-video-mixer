use crate::gpu::{ensure_texture, write_rgba, VideoTex, YuyvPipeline};
use crate::ModuleChrome;
use wgpu_upstream as wgpu;
use nokhwa::pixel_format::RgbAFormat;
use nokhwa::utils::{ApiBackend, CameraIndex, CameraInfo, FrameFormat, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;
use crate::dlog;
use crate::sync::{lock, try_lock};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Serializes the actual frame-grab call across every camera thread.
/// Two `nokhwa` `Camera` instances pulling frames concurrently from
/// different threads can end up delivering one camera's data into the
/// other's buffer — this forces frame grabs one at a time so that can't
/// happen, even though each camera otherwise runs on its own thread.
///
/// `frame()` blocks until the next frame arrives, so holding this makes
/// every camera wait out the others' frame intervals; it's only taken
/// while `SERIALIZE_GRABS` is on (Settings → Debug).
static FRAME_LOCK: Mutex<()> = Mutex::new(());
static SERIALIZE_GRABS: AtomicBool = AtomicBool::new(false);

pub fn set_serialize_grabs(on: bool) {
    SERIALIZE_GRABS.store(on, Ordering::Relaxed);
}

/// A camera frame handed from the capture thread to the render loop.
struct CamFrame {
    width: u32,
    height: u32,
    pixels: Pixels,
    data: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Pixels {
    /// Already decoded on the CPU (MJPEG and anything else).
    Rgba,
    /// Raw YUYV 4:2:2, converted on the GPU by `YuyvPipeline`. Decoding it
    /// on the CPU through nokhwa costs ~3 ms in release but ~160 ms in a
    /// debug build (its converter is generic, so it's compiled unoptimized
    /// inside this crate); copying it is just a memcpy either way.
    Yuyv,
}

/// What the capture thread and the render loop share. Buffers circulate
/// between the two instead of being allocated per frame: the capture
/// thread decodes into a `spare`, publishes it as `ready`, and the render
/// loop takes it, uploads it outside the lock and hands it back.
#[derive(Default)]
struct Shared {
    /// The newest decoded frame the render loop hasn't uploaded yet.
    ready: Option<CamFrame>,
    spare: Vec<Vec<u8>>,
}

/// Owns a background thread that pulls frames from one camera device.
pub struct CameraWorker {
    shared: Arc<Mutex<Shared>>,
    /// The latest frame on the GPU, as RGBA.
    gpu: Option<VideoTex>,
    /// YUYV frames land here (half width, one texel per pixel pair) before
    /// being converted into `gpu`.
    packed: Option<VideoTex>,
    stop: Arc<AtomicBool>,
    /// False once the capture thread has exited (device unplugged, failed
    /// to open, ...).
    alive: Arc<AtomicBool>,
    /// When the worker was spawned, and the last frame it published
    /// (milliseconds after that; 0 = none yet): a grab that blocks forever
    /// in the driver leaves `alive` set but stops this advancing.
    started: Instant,
    last_frame_ms: Arc<AtomicU64>,
}

/// Devices currently held by a worker, by index: a new worker for the same
/// device waits for the old thread to let go instead of racing it.
static HELD: Mutex<Vec<(String, Arc<AtomicBool>)>> = Mutex::new(Vec::new());

/// How long a worker may go without a frame before it's considered hung
/// (longer before the first one: the device is still opening).
const STALL_AFTER: Duration = Duration::from_secs(3);
const FIRST_FRAME_WITHIN: Duration = Duration::from_secs(10);

/// Flips `alive` off when the capture thread ends, however it ends.
struct AliveGuard(Arc<AtomicBool>);

impl Drop for AliveGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

/// How many failed grabs in a row (~20 ms apart) before a camera is
/// considered gone.
const MAX_CONSECUTIVE_ERRORS: u32 = 100;

impl CameraWorker {
    pub fn spawn(index: CameraIndex) -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(true));

        let shared_handle = shared.clone();
        let stop_handle = stop.clone();
        let alive_handle = alive.clone();
        let last_frame_ms = Arc::new(AtomicU64::new(0));
        let last_frame_handle = last_frame_ms.clone();
        let started = Instant::now();

        let key = index.to_string();
        let previous: Vec<Arc<AtomicBool>> = {
            let mut held = lock(&HELD);
            held.retain(|(_, a)| a.load(Ordering::Relaxed));
            let previous = held.iter().filter(|(k, _)| *k == key).map(|(_, a)| a.clone()).collect();
            held.push((key, alive.clone()));
            previous
        };
        thread::spawn(move || {
            let _guard = AliveGuard(alive_handle);
            let name = format!("cam {index}");
            // The old worker for this device may still be inside a grab.
            let waited = Instant::now();
            while previous.iter().any(|a| a.load(Ordering::Relaxed)) && waited.elapsed() < Duration::from_secs(3) {
                thread::sleep(Duration::from_millis(50));
            }
            let requested =
                RequestedFormat::new::<RgbAFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
            let mut camera = match Camera::new(index, requested) {
                Ok(camera) => camera,
                Err(err) => {
                    dlog!("{name}: open failed: {err}");
                    return;
                }
            };
            if let Err(err) = camera.open_stream() {
                dlog!("{name}: open_stream failed: {err}");
                return;
            }
            dlog!("{name}: streaming {}", camera.camera_format());

            let mut stats = CaptureStats::new();
            let mut errors = 0u32;
            let mut last_pixels = None;
            while !stop_handle.load(Ordering::Relaxed) {
                let t0 = Instant::now();
                let (frame, t_locked) = if SERIALIZE_GRABS.load(Ordering::Relaxed) {
                    let _guard = lock(&FRAME_LOCK);
                    let t_locked = Instant::now();
                    (camera.frame(), t_locked)
                } else {
                    (camera.frame(), t0)
                };
                let t_grabbed = Instant::now();
                let Ok(buffer) = frame else {
                    errors += 1;
                    if errors >= MAX_CONSECUTIVE_ERRORS {
                        dlog!("{name}: {errors} grab errors in a row, giving up");
                        break;
                    }
                    thread::sleep(Duration::from_millis(20));
                    continue;
                };
                errors = 0;

                let res = buffer.resolution();
                let (width, height) = (res.width_x, res.height_y);
                let raw = buffer.buffer();
                let mut data = lock(&shared_handle).spare.pop().unwrap_or_default();
                let is_yuyv = buffer.source_frame_format() == FrameFormat::YUYV
                    && width % 2 == 0
                    && raw.len() == (width * height * 2) as usize;
                let frame = if is_yuyv {
                    data.clear();
                    data.extend_from_slice(raw);
                    Some(CamFrame { width, height, pixels: Pixels::Yuyv, data })
                } else {
                    data.resize((width * height * 4) as usize, 0);
                    match buffer.decode_image_to_buffer::<RgbAFormat>(&mut data) {
                        Ok(()) => Some(CamFrame { width, height, pixels: Pixels::Rgba, data }),
                        // Some streams report a resolution that doesn't match
                        // the payload; fall back to the allocating decoder.
                        Err(_) => buffer.decode_image::<RgbAFormat>().ok().map(|img| {
                            let (width, height) = img.dimensions();
                            CamFrame { width, height, pixels: Pixels::Rgba, data: img.into_raw() }
                        }),
                    }
                };
                let t_decoded = Instant::now();

                if let Some(frame) = frame {
                    if last_pixels != Some(frame.pixels) {
                        last_pixels = Some(frame.pixels);
                        dlog!("{name}: {:?} frames, {}", buffer.source_frame_format(), match frame.pixels {
                            Pixels::Yuyv => "converted on the GPU",
                            Pixels::Rgba => "decoded on the CPU",
                        });
                    }
                    last_frame_handle.store((started.elapsed().as_millis() as u64).max(1), Ordering::Relaxed);
                    let mut shared = lock(&shared_handle);
                    // Not uploaded yet: the render loop is behind, so this
                    // frame is dropped and its buffer recycled.
                    if let Some(old) = shared.ready.replace(frame) {
                        shared.spare.push(old.data);
                        stats.dropped += 1;
                    }
                }
                stats.add(t_locked - t0, t_grabbed - t_locked, t_decoded - t_grabbed);
                stats.report_every_second(&name);
            }
            let _ = camera.stop_stream();
            dlog!("{name}: stopped");
        });

        CameraWorker { shared, gpu: None, packed: None, stop, alive, started, last_frame_ms }
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    /// Whether the camera has delivered at least one frame.
    pub fn has_frames(&self) -> bool {
        self.last_frame_ms.load(Ordering::Relaxed) > 0
    }

    /// Alive but silent for too long: the grab is hung in the driver.
    pub fn stalled(&self) -> bool {
        let age = self.started.elapsed();
        let last = Duration::from_millis(self.last_frame_ms.load(Ordering::Relaxed));
        let (silent_for, limit) = if last.is_zero() { (age, FIRST_FRAME_WITHIN) } else { (age.saturating_sub(last), STALL_AFTER) };
        self.is_alive() && silent_for > limit
    }

    /// Uploads the newest captured frame if one arrived since the last
    /// call (YUYV gets its conversion pass recorded into `encoder`), and
    /// returns the RGBA texture. The lock is only held to move the buffer
    /// in and out; the copy to the GPU happens outside it, so the capture
    /// thread never waits on the render loop.
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        yuyv: &YuyvPipeline,
    ) -> Option<VideoTex> {
        // Never wait on the capture thread: keep showing the current frame.
        let ready = try_lock(&self.shared).and_then(|mut s| s.ready.take());
        if let Some(f) = ready {
            use wgpu::TextureUsages as U;
            let usage = U::TEXTURE_BINDING | U::COPY_DST | U::RENDER_ATTACHMENT | U::COPY_SRC;
            match f.pixels {
                Pixels::Rgba if f.data.len() == (f.width * f.height * 4) as usize => {
                    let tex = ensure_texture(device, &mut self.gpu, f.width, f.height, usage);
                    write_rgba(queue, &tex, &f.data);
                }
                Pixels::Yuyv => {
                    let packed = ensure_texture(device, &mut self.packed, f.width / 2, f.height, U::TEXTURE_BINDING | U::COPY_DST);
                    write_rgba(queue, &packed, &f.data);
                    let tex = ensure_texture(device, &mut self.gpu, f.width, f.height, usage);
                    yuyv.record(device, encoder, &packed, &tex);
                }
                Pixels::Rgba => {}
            }
            if let Some(mut shared) = try_lock(&self.shared) {
                shared.spare.push(f.data);
            }
        }
        self.gpu.clone()
    }
}

impl Drop for CameraWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Per-second capture timings for the debug log.
struct CaptureStats {
    since: Instant,
    frames: u32,
    dropped: u32,
    lock_wait: Duration,
    grab: Duration,
    decode: Duration,
}

impl CaptureStats {
    fn new() -> Self {
        CaptureStats {
            since: Instant::now(),
            frames: 0,
            dropped: 0,
            lock_wait: Duration::ZERO,
            grab: Duration::ZERO,
            decode: Duration::ZERO,
        }
    }

    fn add(&mut self, lock_wait: Duration, grab: Duration, decode: Duration) {
        self.frames += 1;
        self.lock_wait += lock_wait;
        self.grab += grab;
        self.decode += decode;
    }

    fn report_every_second(&mut self, name: &str) {
        let elapsed = self.since.elapsed();
        if elapsed < Duration::from_secs(1) {
            return;
        }
        let n = self.frames.max(1) as f32;
        let ms = |d: Duration| d.as_secs_f32() * 1000.0 / n;
        dlog!(
            "{name}: {:.1} fps | lock {:.1}ms grab {:.1}ms decode {:.1}ms | dropped {}",
            self.frames as f32 / elapsed.as_secs_f32(),
            ms(self.lock_wait),
            ms(self.grab),
            ms(self.decode),
            self.dropped
        );
        *self = CaptureStats::new();
    }
}

/// The CAMERA module's own config: which device, and its capture thread.
pub struct CameraState {
    pub chrome: ModuleChrome,
    /// Index into the current camera list; `None` while the wanted camera isn't plugged in.
    pub selected: Option<usize>,
    /// The camera this module is meant to use, by name. Survives the device
    /// being unplugged so it can reattach when it comes back.
    pub wanted: Option<String>,
    pub worker: Option<CameraWorker>,
    pub dropdown_open: bool,
    /// After a capture failure: consecutive failures and when to try again.
    pub failures: u32,
    pub retry_at: Option<Instant>,
}

impl CameraState {
    pub fn new() -> Self {
        CameraState {
            chrome: ModuleChrome::new(),
            selected: None,
            wanted: None,
            worker: None,
            dropdown_open: false,
            failures: 0,
            retry_at: None,
        }
    }

    pub fn label(&self, available: &[CameraInfo]) -> String {
        match (self.selected.and_then(|i| available.get(i)), &self.wanted) {
            (Some(info), _) => info.human_name().to_string(),
            (None, Some(name)) => format!("{name} (disconnected)"),
            (None, None) if available.is_empty() => "No cameras found".to_string(),
            (None, None) => "Select a camera".to_string(),
        }
    }

    /// Only records the pick; the worker lifecycle pass in `update` starts
    /// or stops the actual capture thread based on whether this module is
    /// in use, so an idle selection never keeps a device streaming.
    pub fn select(&mut self, choice: Option<usize>, available: &[CameraInfo]) {
        self.selected = choice;
        self.wanted = choice.and_then(|i| available.get(i)).map(|c| c.human_name().to_string());
        self.worker = None;
        self.failures = 0;
        self.retry_at = None;
        self.chrome.preview_texture = None;
    }

    /// The camera list changed: find `wanted` in it again. The capture thread
    /// is only restarted if the device's index actually moved (or it's gone),
    /// so an unrelated plug/unplug doesn't interrupt a running camera.
    pub fn relink(&mut self, old: &[CameraInfo], new: &[CameraInfo]) {
        let before = self.selected.and_then(|i| old.get(i)).map(|c| c.index().clone());
        self.selected = self
            .wanted
            .as_ref()
            .and_then(|name| new.iter().position(|c| c.human_name() == *name));
        let after = self.selected.and_then(|i| new.get(i)).map(|c| c.index().clone());
        if before != after {
            self.worker = None;
        }
    }
}

/// Enumerates connected cameras, deduplicated to one entry per physical
/// device (see `dedupe_cameras`).
pub fn discover_cameras() -> Vec<CameraInfo> {
    dedupe_cameras(nokhwa::query(ApiBackend::Auto).unwrap_or_default())
}

/// Many webcams register several V4L2/driver nodes for the same physical
/// sensor (this machine's integrated camera shows up as four, e.g. a
/// separate IR node for Windows Hello, or a metadata-only node alongside
/// the real capture node). Only one of them typically produces an actual
/// decodable frame. Probe every candidate concurrently and, per distinct
/// camera name, keep whichever one actually streams video — falling back
/// to the first entry if none of them do, so the camera still shows up.
fn dedupe_cameras(cameras: Vec<CameraInfo>) -> Vec<CameraInfo> {
    let mut groups: Vec<(String, Vec<CameraInfo>)> = Vec::new();
    for info in cameras {
        let name = info.human_name().to_string();
        match groups.iter_mut().find(|(n, _)| *n == name) {
            Some((_, list)) => list.push(info),
            None => groups.push((name, vec![info])),
        }
    }

    // Different physical cameras can be probed in parallel, but a camera's
    // sibling nodes share a USB interface, so probing two of them at once
    // would recreate the exact contention we're trying to detect around.
    // Try each camera's candidates one at a time within its own thread.
    let handles: Vec<_> = groups
        .into_iter()
        .map(|(_, candidates)| thread::spawn(move || pick_working_camera(candidates)))
        .collect();

    handles.into_iter().filter_map(|h| h.join().ok()).collect()
}

/// Tries a physical camera's candidate nodes in turn and keeps the first
/// one that actually produces a frame, falling back to the first candidate
/// if none of them do (so it still shows up, even if non-functional).
fn pick_working_camera(candidates: Vec<CameraInfo>) -> CameraInfo {
    find_working_camera(&candidates)
        .or_else(|| candidates.into_iter().next())
        .expect("at least one candidate per group")
}

fn find_working_camera(candidates: &[CameraInfo]) -> Option<CameraInfo> {
    candidates.iter().find(|c| camera_produces_frame(c.index().clone())).cloned()
}

fn camera_produces_frame(index: CameraIndex) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let requested =
            RequestedFormat::new::<RgbAFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
        let ok = (|| -> Result<(), nokhwa::NokhwaError> {
            let mut camera = Camera::new(index, requested)?;
            camera.open_stream()?;
            // The first frame or two often isn't ready the instant the
            // stream opens (auto-exposure/auto-focus warm-up on real UVC
            // hardware), so a single immediate attempt reads as a false
            // negative on a perfectly working camera. Retry briefly before
            // giving up.
            let mut last_err = None;
            for _ in 0..8 {
                let result = {
                    camera.frame()
                };
                match result {
                    Ok(_) => return Ok(()),
                    Err(err) => last_err = Some(err),
                }
                thread::sleep(Duration::from_millis(200));
            }
            Err(last_err.expect("loop ran at least once"))
        })()
        .is_ok();
        let _ = tx.send(ok);
    });
    rx.recv_timeout(Duration::from_millis(2500)).unwrap_or(false)
}

/// Polls for cameras being plugged in or unplugged while the app runs, and
/// sends the new list whenever it changes. Listing devices is cheap; the
/// expensive "does this node actually stream" probe only runs for a camera
/// that wasn't in the previous list, so running cameras are left alone.
pub struct CameraWatcher {
    pub updates: Receiver<Vec<CameraInfo>>,
}

const WATCH_INTERVAL: Duration = Duration::from_secs(2);

impl CameraWatcher {
    pub fn spawn(initial: Vec<CameraInfo>) -> Self {
        let (tx, updates) = channel();
        thread::spawn(move || {
            let mut known = initial;
            loop {
                thread::sleep(WATCH_INTERVAL);
                let Ok(raw) = nokhwa::query(ApiBackend::Auto) else { continue };

                let mut groups: Vec<(String, Vec<CameraInfo>)> = Vec::new();
                for info in raw {
                    let name = info.human_name().to_string();
                    match groups.iter_mut().find(|(n, _)| *n == name) {
                        Some((_, list)) => list.push(info),
                        None => groups.push((name, vec![info])),
                    }
                }

                let current: Vec<CameraInfo> = groups
                    .into_iter()
                    .filter_map(|(name, candidates)| {
                        let same_name = known.iter().find(|k| k.human_name() == name);
                        let still_there = same_name.filter(|k| candidates.iter().any(|c| c.index() == k.index()));
                        match still_there {
                            Some(k) => Some(k.clone()),
                            // A node that can't be probed may just be busy
                            // streaming: keep what we knew rather than
                            // swapping in an unprobed (maybe metadata) node.
                            None => find_working_camera(&candidates).or_else(|| same_name.cloned()),
                        }
                    })
                    .collect();

                let same = current.len() == known.len()
                    && current
                        .iter()
                        .zip(&known)
                        .all(|(a, b)| a.human_name() == b.human_name() && a.index() == b.index());
                if !same {
                    known = current.clone();
                    if tx.send(current).is_err() {
                        return; // app is gone
                    }
                }
            }
        });
        CameraWatcher { updates }
    }
}
