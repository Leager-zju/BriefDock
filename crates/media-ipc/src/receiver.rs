//! One outstanding frame; a GPU completion acknowledgement releases producer storage.
use crate::{
    Packet, finish, frame_received, gpu::Importer, read_packet, release_frame, request_frame,
};
use futures::channel::mpsc as asynchronous;
use std::{
    io,
    os::windows::io::{AsRawHandle, OwnedHandle},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

pub struct Frame {
    /// GStreamer IPC system presentation timestamp, not a media timeline position.
    pub pts_ns: u64,
    pub adapter_luid: i64,
    pub(crate) handle: OwnedHandle,
    pub(crate) uploaded: AtomicBool,
    release: Mutex<Option<mpsc::Sender<bool>>>,
}
impl Frame {
    pub fn release(&self) {
        self.complete(true);
    }
    pub(crate) fn failed(&self) {
        self.complete(false);
    }
    fn complete(&self, success: bool) {
        if let Some(sender) = self.release.lock().unwrap().take() {
            let _ = sender.send(success);
        }
    }
}
impl Drop for Frame {
    fn drop(&mut self) {
        self.release();
    }
}
impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedFrame")
            .field("pts_ns", &self.pts_ns)
            .finish_non_exhaustive()
    }
}

pub struct Receiver {
    cancelled: Arc<AtomicBool>,
    wake: mpsc::Sender<bool>,
    worker: thread::JoinHandle<()>,
}
impl Receiver {
    pub fn start(
        path: String,
        expected_pid: u32,
    ) -> (
        Self,
        asynchronous::UnboundedReceiver<Result<Arc<Frame>, String>>,
    ) {
        let cancelled = Arc::new(AtomicBool::new(false));
        let stop = cancelled.clone();
        let (sender, output) = asynchronous::unbounded();
        let (wake, ack) = mpsc::channel();
        let release = wake.clone();
        let worker = thread::spawn(move || {
            let run = || -> Result<(), Box<dyn std::error::Error>> {
                if !path.starts_with(r"\\.\pipe\lazybuddy-media-") {
                    return Err("invalid media pipe".into());
                }
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                let mut pipe = loop {
                    if stop.load(Ordering::Acquire) {
                        return Ok(());
                    }
                    match std::fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(&path)
                    {
                        Ok(pipe) => break pipe,
                        Err(error)
                            if (error.kind() == io::ErrorKind::NotFound
                                || error.raw_os_error() == Some(231))
                                && std::time::Instant::now() < deadline =>
                        {
                            thread::sleep(std::time::Duration::from_millis(25))
                        }
                        Err(error) => return Err(error.into()),
                    }
                };
                let Packet::Config {
                    pid,
                    adapter_luid,
                    fence_handle,
                    caps,
                } = read_packet(&mut pipe)?
                else {
                    return Err("media configuration required".into());
                };
                if pid != expected_pid || !caps.contains("RGBA") {
                    return Err("media producer mismatch".into());
                }
                let importer = Importer::new(pid, adapter_luid, fence_handle)?;
                while !stop.load(Ordering::Acquire) {
                    request_frame(&mut pipe)?;
                    match read_packet(&mut pipe)? {
                        Packet::Frame {
                            texture_handle,
                            fence_value,
                            pts_ns,
                            ..
                        } => {
                            frame_received(&mut pipe)?;
                            let handle = importer.receive_handle(texture_handle, fence_value)?;
                            let frame = Arc::new(Frame {
                                pts_ns,
                                adapter_luid,
                                handle,
                                uploaded: AtomicBool::new(false),
                                release: Mutex::new(Some(release.clone())),
                            });
                            if sender.unbounded_send(Ok(frame)).is_err() {
                                break;
                            }
                            let copied = ack.recv().map_err(io::Error::other)?;
                            release_frame(&mut pipe, texture_handle)?;
                            if !copied {
                                return Err("media texture import failed".into());
                            }
                        }
                        Packet::Eos => break,
                        _ => return Err("unexpected media frame".into()),
                    }
                }
                finish(&mut pipe)?;
                Ok(())
            };
            if run().is_err() && !stop.load(Ordering::Acquire) {
                let _ = sender.unbounded_send(Err("媒体画面传输失败。".into()));
            }
        });
        (
            Self {
                cancelled,
                wake,
                worker,
            },
            output,
        )
    }
}
impl Drop for Receiver {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        let _ = self.wake.send(false);
        unsafe {
            let _ = windows::Win32::System::IO::CancelSynchronousIo(
                windows::Win32::Foundation::HANDLE(self.worker.as_raw_handle()),
            );
        }
    }
}
