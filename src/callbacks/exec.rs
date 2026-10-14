use crate::protocol::{self, TaskResultParams};
use crate::utils::post_json_async;
use miniserde::{Deserialize, Serialize, json};
use std::collections::{HashSet, VecDeque};
use std::process::Stdio;
use std::sync::Arc;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::process::Command;
use tokio::sync::Mutex;

/// Shared across HTTP and WebSocket, including reconnects. Retain completed
/// results so a delivery retry never needs to rerun a recent command.
#[derive(Default)]
pub struct ExecCache {
    running: HashSet<String>,
    completed: VecDeque<(String, String)>,
}

pub type SharedExecCache = Arc<Mutex<ExecCache>>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RemoteExec {
    message: String,
    task_id: String,
    command: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RemoteExecCallback {
    task_id: String,
    result: String,
    exit_code: i32,
    finished_at: String,
}

// Receive string directly instead of struct to avoid redundant parsing
pub async fn exec_command(
    utf8_str: &str,
    callback_url: String,
    ignore_unsafe_cert: &bool,
    cache: &SharedExecCache,
) -> Result<(), String> {
    let remote_exec: RemoteExec = match json::from_str(utf8_str) {
        Ok(value) => value,
        Err(_) => {
            let rpc: crate::protocol::IncomingRequest =
                json::from_str(utf8_str).map_err(|_| "Failed to parse RemoteExec".to_string())?;
            let params = rpc
                .params
                .ok_or_else(|| "Failed to parse RemoteExec params".to_string())?;
            RemoteExec {
                message: rpc.method,
                task_id: params.task_id.unwrap_or_default(),
                command: params.command.unwrap_or_default(),
            }
        }
    };

    if remote_exec.task_id.is_empty() || remote_exec.command.is_empty() {
        return Err("Exec request is missing task_id or command".into());
    }
    let task_id = remote_exec.task_id.clone();
    let cached = {
        let mut cache = cache.lock().await;
        if let Some((_, result)) = cache.completed.iter().find(|(id, _)| id == &task_id) {
            Some(result.clone())
        } else if cache.running.insert(task_id.clone()) {
            None
        } else {
            return Err(format!("Task {task_id} is already running"));
        }
    };
    let payload = if let Some(payload) = cached {
        payload
    } else {
        let result = execute(remote_exec).await;
        let mut cache = cache.lock().await;
        cache.running.remove(&task_id);
        let payload = result?;
        cache.completed.push_back((task_id, payload.clone()));
        while cache.completed.len() > 256 {
            cache.completed.pop_front();
        }
        payload
    };
    post_json_async(callback_url, payload, *ignore_unsafe_cert)
        .await
        .map(|_| ())
}

async fn execute(remote_exec: RemoteExec) -> Result<String, String> {
    let exec = tokio::spawn(async move {
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("powershell");
            command.args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &remote_exec.command,
            ]);
            command
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("sh");
            command.arg("-c").arg(&remote_exec.command);
            command
        };

        let Ok(child) = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        else {
            return Err("failed to execute process".to_string());
        };

        let Ok(output) = child.wait_with_output().await else {
            return Err("failed to get process output".to_string());
        };

        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let stderr_str = String::from_utf8_lossy(&output.stderr);

        let status = output.status.code().unwrap_or(1);

        Ok((status, format!("{stdout_str}{stderr_str}"))) // Simplify string concatenation
    });

    let Ok(Ok((status, output))) = exec.await else {
        return Err("failed to execute process".to_string());
    };

    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let finished_at = now.format(&Rfc3339).unwrap_or_default();

    let reply = RemoteExecCallback {
        task_id: remote_exec.task_id,
        result: output,
        exit_code: status,
        finished_at,
    };

    Ok(protocol::task_result(TaskResultParams {
        task_id: reply.task_id,
        result: reply.result,
        exit_code: reply.exit_code,
        finished_at: reply.finished_at,
    }))
}
