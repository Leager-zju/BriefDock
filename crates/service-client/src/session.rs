//! One service actor per active feature. Idle actors block without polling.
use crate::Client;
use futures::channel::oneshot;
use serde_json::Value;
use std::{
    io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

enum Command {
    Call(Value, oneshot::Sender<io::Result<Value>>),
    Stop,
    Finish(oneshot::Sender<()>),
}

struct Inner {
    commands: mpsc::Sender<Command>,
    cancelled: Arc<AtomicBool>,
    graceful: AtomicBool,
    #[cfg(windows)]
    stopper: Arc<std::sync::Mutex<Option<Arc<crate::job::Job>>>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        if self.graceful.load(Ordering::Acquire) {
            return;
        }
        self.cancelled.store(true, Ordering::Release);
        #[cfg(windows)]
        if let Some(job) = self.stopper.lock().unwrap().as_ref() {
            job.stop();
        }
        let _ = self.commands.send(Command::Stop);
    }
}

#[derive(Clone)]
pub struct Session(Arc<Inner>);

impl Session {
    pub fn start(executable: PathBuf, arguments: Vec<String>) -> Self {
        Self::with_environment(executable, arguments, vec![])
    }

    pub fn with_environment(
        executable: PathBuf,
        arguments: Vec<String>,
        environment: Vec<(String, String)>,
    ) -> Self {
        let (commands, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        #[cfg(windows)]
        let stopper = Arc::new(std::sync::Mutex::new(None));
        #[cfg(windows)]
        let worker_stopper = stopper.clone();
        std::thread::spawn(move || {
            if worker_cancelled.load(Ordering::Acquire) {
                return;
            }
            let mut client = Client::start_with_environment(&executable, &arguments, &environment);
            #[cfg(windows)]
            if let Ok(client) = &client {
                *worker_stopper.lock().unwrap() = Some(client.job.clone());
            }
            while let Ok(command) = receiver.recv() {
                if worker_cancelled.load(Ordering::Acquire) {
                    break;
                }
                match command {
                    Command::Stop => break,
                    Command::Finish(reply) => {
                        if let Ok(client) = &mut client {
                            let _=client.request(&serde_json::json!({"version":1,"id":"shutdown","method":"lifecycle.suspend","args":{}}));
                        }
                        drop(client);
                        let _ = reply.send(());
                        return;
                    }
                    Command::Call(request, reply) => {
                        let result = match &mut client {
                            Ok(client) => client.request_cancellable(&request, &worker_cancelled),
                            Err(_) => Err(io::Error::other("service startup failed")),
                        };
                        let _ = reply.send(result);
                    }
                }
            }
            // Client's Drop tears down its Windows Job and joins its pipe reader.
        });
        Self(Arc::new(Inner {
            commands,
            cancelled,
            graceful: AtomicBool::new(false),
            #[cfg(windows)]
            stopper,
        }))
    }

    pub fn request(
        &self,
        request: Value,
    ) -> impl std::future::Future<Output = io::Result<Value>> + Send + 'static {
        let (sender, receiver) = oneshot::channel();
        let sent = self.0.commands.send(Command::Call(request, sender));
        async move {
            if sent.is_err() {
                return Err(io::Error::other("service session stopped"));
            }
            receiver
                .await
                .map_err(|_| io::Error::other("service request cancelled"))?
        }
    }

    pub fn stop(&self) {
        self.0.cancelled.store(true, Ordering::Release);
        #[cfg(windows)]
        if let Some(job) = self.0.stopper.lock().unwrap().as_ref() {
            job.stop();
        }
        let _ = self.0.commands.send(Command::Stop);
    }

    /// Finish a final persistence transaction, then tear down the owned process tree.
    pub fn shutdown(self) -> impl std::future::Future<Output = ()> + Send + 'static {
        self.0.graceful.store(true, Ordering::Release);
        let (sender, receiver) = oneshot::channel();
        let _ = self.0.commands.send(Command::Finish(sender));
        async move {
            let _ = receiver.await;
        }
    }
}

impl std::fmt::Debug for Session {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServiceSession")
            .field("stopped", &self.0.cancelled.load(Ordering::Acquire))
            .finish()
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cancelling_a_blocked_pipe_write_does_not_wait_for_the_service() {
        let session = Session::start(
            PathBuf::from("pwsh.exe"),
            vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                "Start-Sleep -Seconds 30".into(),
            ],
        );
        let pending = session.request(
            json!({"version":1,"id":"blocked","method":"fixture","pad":"x".repeat(60000)}),
        );
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(futures::executor::block_on(pending));
        });
        std::thread::sleep(std::time::Duration::from_millis(100));
        session.stop();
        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap()
                .is_err()
        );
    }

    #[test]
    fn graceful_shutdown_finishes_the_cleanup_rpc_before_stopping() {
        let script = r#"while($line=[Console]::ReadLine()){$r=$line|ConvertFrom-Json;if($r.method -eq 'lifecycle.suspend'){Start-Sleep -Milliseconds 100};[Console]::WriteLine((@{version=1;id=$r.id;ok=$true;data=@{pid=$PID}}|ConvertTo-Json -Compress))}"#;
        let session = Session::start(
            PathBuf::from("pwsh.exe"),
            vec![
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                script.into(),
            ],
        );
        futures::executor::block_on(
            session.request(json!({"version":1,"id":"health","method":"health"})),
        )
        .unwrap();
        let started = std::time::Instant::now();
        futures::executor::block_on(session.shutdown());
        assert!(started.elapsed() >= std::time::Duration::from_millis(80));
    }

    #[test]
    fn session_reuses_process_and_cancellation_stops_an_inflight_request() {
        let script = r#"$n=0;while($line=[Console]::ReadLine()){$r=$line|ConvertFrom-Json;if($r.method -eq 'block'){Start-Sleep -Seconds 30};$n++;[Console]::WriteLine((@{version=1;id=$r.id;ok=$true;data=@{pid=$PID;count=$n}}|ConvertTo-Json -Compress))}"#;
        let session = Session::start(
            PathBuf::from("pwsh.exe"),
            vec![
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                script.into(),
            ],
        );
        let request = |method: &str| json!({"version":1,"id":"test","method":method});
        let first = futures::executor::block_on(session.request(request("echo"))).unwrap();
        let second = futures::executor::block_on(session.request(request("echo"))).unwrap();
        assert_eq!(first["data"]["pid"], second["data"]["pid"]);
        assert_eq!(second["data"]["count"], 2);
        let pid = first["data"]["pid"].as_u64().unwrap() as u32;
        let process =
            unsafe { windows_sys::Win32::System::Threading::OpenProcess(0x00100000, 0, pid) };
        assert!(!process.is_null());
        let pending = session.request(request("block"));
        std::thread::sleep(std::time::Duration::from_millis(100));
        let started = std::time::Instant::now();
        session.stop();
        assert!(futures::executor::block_on(pending).is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert!(futures::executor::block_on(session.request(request("echo"))).is_err());
        unsafe {
            assert_eq!(
                windows_sys::Win32::System::Threading::WaitForSingleObject(process, 2000),
                0
            );
            windows_sys::Win32::Foundation::CloseHandle(process);
        }
    }
}
