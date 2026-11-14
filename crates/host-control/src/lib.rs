//! Versioned local RPC shared by the desktop app and console client.
#[cfg(not(windows))]
compile_error!("LazyBuddy host control currently supports Windows only");
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{io, path::PathBuf, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

pub const MAX_REQUEST: usize = 65536;
pub const TIMEOUT: Duration = Duration::from_secs(10);
pub type Reply = tokio::sync::oneshot::Sender<Value>;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u8,
    pub command: String,
    #[serde(default)]
    pub args: Value,
}
pub fn error(code: &str, message: impl ToString) -> Value {
    json!({"ok":false,"error":{"code":code,"message":message.to_string()}})
}
pub fn instance() -> io::Result<String> {
    let value = std::env::var("LAZYBUDDY_INSTANCE").unwrap_or_else(|_| "default".into());
    if !valid_instance(&value) {
        return Err(io::Error::other(
            "instance must contain 1-64 ASCII letters, digits, - or _",
        ));
    }
    Ok(value)
}
fn valid_instance(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
pub fn isolated() -> bool {
    instance().is_ok_and(|id| id != "default")
}
pub fn data_dir() -> io::Result<PathBuf> {
    if instance()? != "default" {
        let path =
            PathBuf::from(std::env::var_os("LAZYBUDDY_DATA_DIR").ok_or_else(|| {
                io::Error::other("isolated instance requires LAZYBUDDY_DATA_DIR")
            })?);
        if !path.is_absolute() {
            return Err(io::Error::other("isolated data directory must be absolute"));
        }
        let production = default_data_dir()?;
        let normalize = |p: &std::path::Path| {
            std::fs::canonicalize(p)
                .unwrap_or_else(|_| p.to_path_buf())
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase()
        };
        let target = normalize(&path);
        let production = normalize(&production);
        if target == production || target.starts_with(&(production + "/")) {
            return Err(io::Error::other(
                "isolated data must not use the production data directory",
            ));
        }
        return Ok(path);
    }
    if let Some(path) = std::env::var_os("LAZYBUDDY_DATA_DIR") {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err(io::Error::other("absolute data directory required"));
        }
        return Ok(path);
    }
    default_data_dir()
}
fn default_data_dir() -> io::Result<PathBuf> {
    #[cfg(windows)]
    let path = std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("LazyBuddy"));
    path.ok_or_else(|| io::Error::other("data directory unavailable"))
}
#[cfg(windows)]
fn user_sid() -> io::Result<String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, LocalFree},
        Security::{
            Authorization::ConvertSidToStringSidW, GetTokenInformation, TOKEN_QUERY, TOKEN_USER,
            TokenUser,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut size = 0;
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut size);
        let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
        let ok = GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        );
        CloseHandle(token);
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut wide = std::ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut wide) == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut len = 0;
        while *wide.add(len) != 0 {
            len += 1;
        }
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(wide, len));
        LocalFree(wide.cast());
        Ok(sid)
    }
}
#[cfg(windows)]
fn endpoint() -> io::Result<String> {
    Ok(format!(
        r"\\.\pipe\LazyBuddy-{}-{}",
        user_sid()?,
        instance()?
    ))
}
pub async fn call(request: &Request) -> io::Result<Value> {
    #[cfg(windows)]
    let stream = tokio::net::windows::named_pipe::ClientOptions::new().open(endpoint()?)?;
    tokio::time::timeout(TIMEOUT, async {
        let mut stream = BufReader::new(stream);
        let mut bytes = serde_json::to_vec(request)?;
        if bytes.len() + 1 > MAX_REQUEST {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "maximum request size is 64 KiB",
            ));
        }
        bytes.push(b'\n');
        stream.get_mut().write_all(&bytes).await?;
        let mut response = String::new();
        use tokio::io::AsyncReadExt;
        (&mut stream)
            .take(4 * 1024 * 1024 + 1)
            .read_to_string(&mut response)
            .await?;
        if response.len() > 4 * 1024 * 1024 {
            return Err(io::Error::other("host reply too large"));
        }
        serde_json::from_str(&response).map_err(io::Error::other)
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "application did not reply"))?
}
async fn connection<S, F>(stream: S, dispatch: F)
where
    S: AsyncRead + AsyncWrite + Unpin,
    F: Fn(Request, tokio::sync::oneshot::Sender<Value>),
{
    let mut stream = BufReader::new(stream);
    let result = tokio::time::timeout(TIMEOUT, async {
        let mut bytes = Vec::new();
        loop {
            let available = stream.fill_buf().await?;
            if available.is_empty() {
                break;
            }
            let count = available
                .iter()
                .position(|b| *b == b'\n')
                .map_or(available.len(), |i| i + 1);
            if bytes.len() + count > MAX_REQUEST {
                return Ok(error("request_too_large", "maximum request size is 64 KiB"));
            }
            bytes.extend_from_slice(&available[..count]);
            stream.consume(count);
            if bytes.last() == Some(&b'\n') {
                break;
            }
        }
        let request: Request = match serde_json::from_slice(&bytes) {
            Ok(r) => r,
            Err(_) => return Ok(error("invalid_request", "invalid RPC request")),
        };
        if request.version != 1 {
            return Ok(error("protocol_version", "supported protocol version is 1"));
        }
        let (send, receive) = tokio::sync::oneshot::channel();
        dispatch(request, send);
        receive
            .await
            .map_err(|_| io::Error::other("application closed"))
    })
    .await;
    let response = match result {
        Ok(Ok(v)) => v,
        _ => error("timeout", "application not ready or request timed out"),
    };
    if let Ok(mut bytes) = serde_json::to_vec(&response) {
        bytes.push(b'\n');
        let _ = stream.get_mut().write_all(&bytes).await;
    }
}
#[cfg(windows)]
fn create_pipe(first: bool) -> io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SECURITY_ATTRIBUTES,
        },
    };
    let address = endpoint()?;
    let sddl: Vec<u16> = format!("D:P(A;;GA;;;{})", user_sid()?)
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe {
        let mut descriptor = std::ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let result = {
            tokio::net::windows::named_pipe::ServerOptions::new()
                .first_pipe_instance(first)
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(
                    address,
                    std::ptr::from_ref(&attributes).cast_mut().cast(),
                )
        };
        LocalFree(descriptor);
        result
    }
}

/// The IPC worker never reads application thread-local settings or mutates GUI state.
pub fn serve(
    dispatch: impl Fn(Request, tokio::sync::oneshot::Sender<Value>) + Send + Sync + 'static,
) -> io::Result<()> {
    // Bind synchronously so duplicate instances fail before a second GUI is created.
    #[cfg(windows)]
    let first = {
        let runtime = tokio::runtime::Runtime::new()?;
        let server = {
            let _guard = runtime.enter();
            create_pipe(true)?
        };
        (runtime, server)
    };
    std::thread::spawn(move || {
        let (runtime, mut listener) = first;
        runtime.block_on(async move {
            #[cfg(windows)]
            loop {
                if listener.connect().await.is_err() {
                    break;
                }
                let next = match create_pipe(false) {
                    Ok(next) => next,
                    Err(_) => break,
                };
                let connected = std::mem::replace(&mut listener, next);
                connection(connected, &dispatch).await;
            }
        });
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn instance_names_cannot_escape_the_endpoint_namespace() {
        for name in ["", "../default", "a/b", "a\\b", "a b", "a\n", "新闻"] {
            assert!(!valid_instance(name));
        }
        assert!(!valid_instance(&"a".repeat(65)));
        assert!(valid_instance("qa_123-ABC"));
        assert!(valid_instance(&"a".repeat(64)));
    }
    #[test]
    fn protocol_rejects_unknown_fields_and_keeps_explicit_version() {
        assert!(
            serde_json::from_str::<Request>(r#"{"version":1,"command":"status","surprise":1}"#)
                .is_err()
        );
        let r: Request = serde_json::from_str(r#"{"version":9,"command":"status"}"#).unwrap();
        assert_eq!(r.version, 9);
    }
}

pub fn call_sync(request: &Request) -> io::Result<Value> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(call(request))
}
