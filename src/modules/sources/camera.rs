use crate::gpu::{create_texture, write_rgba, VideoTex};
use crate::ModuleChrome;
use wgpu_upstream as wgpu;
use nokhwa::pixel_format::RgbAFormat;
use nokhwa::utils::{ApiBackend, CameraIndex, CameraInfo, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Serializes the actual frame-grab call across every camera thread.
/// Two `nokhwa` `Camera` instances pulling frames concurrently from
/// different threads can end up delivering one camera's data into the
/// other's buffer — this forces frame grabs one at a time so that can't
/// happen, even though each camera otherwise runs on its own thread.
static FRAME_LOCK: Mutex<()> = Mutex::new(());

/// A decoded camera frame handed from the capture thread to the render loop.
struct CamFrame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    version: u64,
}

/// Owns a background thread that pulls frames from one camera device.
pub struct CameraWorker {
    frame: Arc<Mutex<Option<CamFrame>>>,
    stop: Arc<AtomicBool>,
    /// False once the capture thread has exited (device unplugged, failed
    /// to open, ...).
    alive: Arc<AtomicBool>,
}

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
        let frame = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(true));

        let frame_handle = frame.clone();
        let stop_handle = stop.clone();
        let alive_handle = alive.clone();
        thread::spawn(move || {
            let _guard = AliveGuard(alive_handle);
            let requested =
                RequestedFormat::new::<RgbAFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
            let mut camera = match Camera::new(index, requested) {
                Ok(camera) => camera,
                Err(_) => return,
            };
            if camera.open_stream().is_err() {
                return;
            }

            let mut version = 0u64;
            let mut errors = 0u32;
            while !stop_handle.load(Ordering::Relaxed) {
                let frame = {
                    let _guard = FRAME_LOCK.lock().unwrap();
                    camera.frame()
                };
                let Ok(buffer) = frame else {
                    errors += 1;
                    if errors >= MAX_CONSECUTIVE_ERRORS {
                        break;
                    }
                    thread::sleep(Duration::from_millis(20));
                    continue;
                };
                errors = 0;
                {
                    if let Ok(decoded) = buffer.decode_image::<RgbAFormat>() {
                        version += 1;
                        let (width, height) = decoded.dimensions();
                        *frame_handle.lock().unwrap() = Some(CamFrame {
                            width,
                            height,
                            rgba: decoded.into_raw(),
                            version,
                        });
                    }
                }
            }
            let _ = camera.stop_stream();
        });

        CameraWorker { frame, stop, alive }
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    /// Uploads the newest captured frame into `slot` if it's newer than
    /// what's already there, and returns the texture. The upload copies
    /// straight from the capture thread's buffer under its lock; nothing is
    /// cloned.
    pub fn upload(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        slot: &mut Option<(VideoTex, u64)>,
    ) -> Option<VideoTex> {
        if let Some(f) = self.frame.lock().unwrap().as_ref() {
            let fresh = !matches!(slot, Some((t, v)) if *v == f.version && (t.width, t.height) == (f.width, f.height));
            if fresh && f.rgba.len() == (f.width * f.height * 4) as usize {
                let reuse = matches!(slot, Some((t, _)) if (t.width, t.height) == (f.width, f.height));
                if !reuse {
                    let usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST;
                    *slot = Some((create_texture(device, f.width, f.height, usage), 0));
                }
                let (tex, version) = slot.as_mut().unwrap();
                write_rgba(queue, tex, &f.rgba);
                *version = f.version;
            }
        }
        slot.as_ref().map(|(t, _)| t.clone())
    }
}

impl Drop for CameraWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
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
    /// The latest frame on the GPU, and the capture version it came from.
    pub gpu: Option<(VideoTex, u64)>,
}

impl CameraState {
    pub fn new() -> Self {
        CameraState {
            chrome: ModuleChrome::new(),
            selected: None,
            wanted: None,
            worker: None,
            dropdown_open: false,
            gpu: None,
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
        self.chrome.preview_texture = None;
        self.gpu = None;
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
            self.gpu = None;
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
    for candidate in &candidates {
        if camera_produces_frame(candidate.index().clone()) {
            return candidate.clone();
        }
    }
    candidates.into_iter().next().expect("at least one candidate per group")
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
                    let _guard = FRAME_LOCK.lock().unwrap();
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
                    .map(|(name, candidates)| {
                        let still_there = known
                            .iter()
                            .find(|k| k.human_name() == name && candidates.iter().any(|c| c.index() == k.index()));
                        match still_there {
                            Some(k) => k.clone(),
                            None => pick_working_camera(candidates),
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
