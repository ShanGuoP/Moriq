//! Same-user local protocol transport. No database or library ownership here.
use serde::Deserialize;
use serde_json::Value;
use std::{path::PathBuf, time::Duration};
pub type Result<T> = std::result::Result<T, String>;
#[derive(Deserialize)]
struct Discovery {
    protocol: u32,
    base_url: String,
    instance_id: String,
}
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: String,
}
impl Client {
    pub async fn discover_or_start(executable: Option<PathBuf>) -> Result<Self> {
        if !cfg!(windows) {
            return Err("当前 Synco v1 协议仅适用于 Windows".into());
        }
        if let Ok(client) = Self::discover().await {
            return Ok(client);
        }
        static STARTING: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
        let _guard = STARTING
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        if let Ok(client) = Self::discover().await {
            return Ok(client);
        }
        let path = executable
            .or_else(|| std::env::var_os("SYNCO_EXECUTABLE").map(PathBuf::from))
            .or_else(|| {
                [
                    std::env::var_os("LOCALAPPDATA")
                        .map(|p| PathBuf::from(p).join("Programs/Synco/Synco.exe")),
                    std::env::var_os("ProgramFiles")
                        .map(|p| PathBuf::from(p).join("Synco/Synco.exe")),
                ]
                .into_iter()
                .flatten()
                .find(|p| p.is_file())
            })
            .ok_or("未找到 Synco 安装位置，请在联动面板选择 Synco 程序")?;
        tokio::task::spawn_blocking(move || -> Result<()> {
            let path = path
                .canonicalize()
                .map_err(|e| format!("Synco 程序不存在：{e}"))?;
            if !path.is_file() {
                return Err("Synco 程序路径无效".into());
            }
            let mut command = std::process::Command::new(&path);
            command.current_dir(path.parent().ok_or("Synco 程序目录无效")?);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            let mut child = command.spawn().map_err(|e| e.to_string())?;
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())??;
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            match Self::discover().await {
                Ok(client) => return Ok(client),
                Err(error) if std::time::Instant::now() >= deadline => {
                    return Err(format!("已启动 Synco，但服务尚未就绪：{error}"));
                }
                Err(_) => {}
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
    pub async fn discover() -> Result<Self> {
        if !cfg!(windows) {
            return Err("当前 Synco v1 协议仅适用于 Windows；macOS 尚未定义交接协议".into());
        }
        let path = std::env::var_os("SYNCO_INTEROP_DISCOVERY")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("LOCALAPPDATA")
                    .map(|p| PathBuf::from(p).join("Synco/interop.json"))
            })
            .ok_or("无法定位 Synco 发现文件")?;
        let discovery = tokio::task::spawn_blocking(move || {
            if std::fs::metadata(&path)
                .map_err(|_| "未发现 Synco，请启动 Synco 0.4.1 或更新版本后重试")?
                .len()
                > 65536
            {
                return Err("Synco 发现文件过大".to_string());
            }
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            serde_json::from_slice::<Discovery>(&bytes)
                .map_err(|e| format!("Synco 发现文件损坏：{e}"))
        })
        .await
        .map_err(|e| e.to_string())??;
        Self::connect(
            &discovery.base_url,
            &discovery.instance_id,
            discovery.protocol,
        )
        .await
    }
    pub async fn connect(base: &str, instance: &str, protocol: u32) -> Result<Self> {
        let url = reqwest::Url::parse(base).map_err(|_| "Synco 地址无效")?;
        if protocol != 1
            || url.scheme() != "http"
            || url.host_str() != Some("127.0.0.1")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
        {
            return Err("Synco 只允许协议 v1 的 127.0.0.1 本机服务".into());
        }
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(90))
            .build()
            .map_err(|e| e.to_string())?;
        let client = Self {
            http,
            base: base.trim_end_matches('/').into(),
        };
        let response = client
            .http
            .get(format!("{}/api/interop/v1/info", client.base))
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .map_err(|e| format!("Synco 未就绪：{e}"))?;
        let info = bounded_json(response).await?;
        if info["protocol"] != 1 || info["instance_id"].as_str() != Some(instance) {
            return Err("Synco 实例与发现文件不一致，请重新启动 Synco".into());
        }
        Ok(client)
    }
    pub async fn get(&self, task: &str, client: &str) -> Result<Option<Value>> {
        let response = self
            .http
            .get(format!("{}/api/interop/v1/tasks/{task}", self.base))
            .query(&[("client_id", client)])
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        bounded_json(response).await.map(Some)
    }
    pub async fn send(&self, spec: &Value) -> Result<Value> {
        let response = self
            .http
            .post(format!("{}/api/interop/v1/tasks", self.base))
            .json(spec)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        bounded_json(response).await
    }
    pub async fn ack(&self, task: &str, body: &Value) -> Result<()> {
        let response = self
            .http
            .post(format!("{}/api/interop/v1/tasks/{task}/ack", self.base))
            .json(body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        bounded_json(response).await?;
        Ok(())
    }
    pub async fn cancel(&self, task: &str, client: &str) -> Result<()> {
        let response = self
            .http
            .post(format!("{}/api/interop/v1/tasks/{task}/cancel", self.base))
            .json(&serde_json::json!({"client_id":client}))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        bounded_json(response).await?;
        Ok(())
    }
}
async fn bounded_json(mut response: reqwest::Response) -> Result<Value> {
    let status = response.status();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > 1024 * 1024 {
            return Err("Synco 响应超过 1 MiB，已停止读取".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("Synco 响应格式错误：{e}"))?;
    if !status.is_success() {
        return Err(format!(
            "Synco {}：{}",
            status,
            value["error"]["message"]
                .as_str()
                .unwrap_or("请求失败，请查看 Synco")
        ));
    }
    Ok(value)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    fn server(replies: Vec<(u16, String)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let thread = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in replies {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0u8; 4096];
                loop {
                    let n = stream.read(&mut buffer).unwrap();
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(end) = bytes.windows(4).position(|p| p == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                        let length = head
                            .lines()
                            .find_map(|l| {
                                l.strip_prefix("content-length:")
                                    .and_then(|v| v.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                requests.push(String::from_utf8(bytes).unwrap());
                write!(stream,"HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
            requests
        });
        (base, thread)
    }
    #[test]
    fn transport_pins_loopback_instance_and_keeps_retry_payload() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            assert!(Client::connect("http://example.com", "a", 1).await.is_err());
            assert!(
                Client::connect("http://127.0.0.1/evil", "a", 1)
                    .await
                    .is_err()
            );
            let (base, thread) = server(vec![
                (200, r#"{"protocol":1,"instance_id":"a"}"#.into()),
                (404, "{}".into()),
                (202, "{}".into()),
                (202, "{}".into()),
                (200, r#"{"ok":true}"#.into()),
            ]);
            let client = Client::connect(&base, "a", 1).await.unwrap();
            assert!(client.get("task-1", "client-1").await.unwrap().is_none());
            let request = serde_json::json!({"task_id":"stable-task","client_id":"client-1"});
            client.send(&request).await.unwrap();
            client.send(&request).await.unwrap();
            client
                .ack("task-1", &serde_json::json!({"client_id":"client-1"}))
                .await
                .unwrap();
            let requests = thread.join().unwrap();
            let a = requests[2].split("\r\n\r\n").nth(1).unwrap();
            let b = requests[3].split("\r\n\r\n").nth(1).unwrap();
            assert_eq!(a, b);
            let (base, thread) = server(vec![(
                200,
                r#"{"protocol":1,"instance_id":"different"}"#.into(),
            )]);
            assert!(Client::connect(&base, "a", 1).await.is_err());
            thread.join().unwrap();
        });
    }
}
