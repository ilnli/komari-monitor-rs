//! Komari Agent protocol v2 (JSON-RPC 2.0) message types.

use crate::data_struct::{BasicInfo, RealTimeInfo};
use miniserde::{Deserialize, Serialize};

pub const JSONRPC_VERSION: &str = "2.0";
pub const AGENT_REPORT: &str = "agent.report";
pub const AGENT_BASIC_INFO: &str = "agent.basicInfo";
pub const AGENT_PING_RESULT: &str = "agent.pingResult";
pub const AGENT_TASK_RESULT: &str = "agent.taskResult";
pub const AGENT_EXEC: &str = "agent.exec";
pub const AGENT_PING: &str = "agent.ping";
pub const AGENT_MESSAGE: &str = "agent.message";
pub const AGENT_EVENT: &str = "agent.event";
pub const AGENT_TERMINAL_REQUEST: &str = "agent.terminal.request";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RpcRequest<P> {
    pub jsonrpc: String,
    pub method: String,
    pub params: P,
    pub id: Option<String>,
}

impl<P: Serialize> RpcRequest<P> {
    pub fn notification(method: &str, params: P) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            method: method.to_string(),
            params,
            id: None,
        }
    }

    pub fn request(method: &str, params: P, id: impl Into<String>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            method: method.to_string(),
            params,
            id: Some(id.into()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BasicInfoParams {
    pub info: BasicInfo,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReportParams {
    pub report: RealTimeInfo,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReportRequestParams {
    pub report: RealTimeInfo,
    pub ack_event_ids: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PingResultParams {
    pub task_id: u64,
    pub ping_type: String,
    pub value: i64,
    pub finished_at: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TaskResultParams {
    pub task_id: String,
    pub result: String,
    pub exit_code: i32,
    pub finished_at: String,
}

/// Parameters used by server-to-agent events. Unknown fields are ignored so
/// newer server fields remain forward compatible.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct EventParams {
    pub message: Option<String>,
    pub task_id: Option<String>,
    pub command: Option<String>,
    pub ping_task_id: Option<u64>,
    pub ping_type: Option<String>,
    pub ping_target: Option<String>,
    pub request_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IncomingRequest {
    pub jsonrpc: Option<String>,
    pub method: String,
    pub params: Option<EventParams>,
    pub id: Option<miniserde::json::Value>,
    // v1 compatibility: old server messages put the method in `message`.
    pub message: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IncomingEvent {
    pub id: String,
    pub method: String,
    pub params: Option<EventParams>,
    pub created_at: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EventResult {
    pub status: Option<String>,
    pub events: Option<Vec<IncomingEvent>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RpcResponse {
    pub jsonrpc: Option<String>,
    pub id: Option<miniserde::json::Value>,
    pub result: Option<EventResult>,
    pub error: Option<RpcError>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

pub fn parse_response(body: &str) -> Result<RpcResponse, String> {
    let response: RpcResponse =
        miniserde::json::from_str(body).map_err(|_| "Invalid JSON-RPC response".to_string())?;
    if response.jsonrpc.as_deref() != Some(JSONRPC_VERSION) {
        return Err("Invalid JSON-RPC version in response".into());
    }
    if let Some(error) = &response.error {
        return Err(format!("JSON-RPC error {}: {}", error.code, error.message));
    }
    if response.result.is_none() {
        return Err("Missing JSON-RPC result".into());
    }
    Ok(response)
}

pub fn basic_info(info: BasicInfo) -> String {
    miniserde::json::to_string(&RpcRequest::notification(
        AGENT_BASIC_INFO,
        BasicInfoParams { info },
    ))
}

pub fn report(report: RealTimeInfo) -> String {
    miniserde::json::to_string(&RpcRequest::notification(
        AGENT_REPORT,
        ReportParams { report },
    ))
}

pub fn report_request(
    report: RealTimeInfo,
    id: impl Into<String>,
    ack_event_ids: Vec<String>,
) -> String {
    miniserde::json::to_string(&RpcRequest::request(
        AGENT_REPORT,
        ReportRequestParams {
            report,
            ack_event_ids,
        },
        id,
    ))
}

pub fn ping_result(params: PingResultParams) -> String {
    miniserde::json::to_string(&RpcRequest::notification(AGENT_PING_RESULT, params))
}

pub fn task_result(params: TaskResultParams) -> String {
    miniserde::json::to_string(&RpcRequest::notification(AGENT_TASK_RESULT, params))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_is_json_rpc_v2_and_wraps_report() {
        let payload = report(RealTimeInfo {
            cpu: crate::data_struct::Cpu { usage: 1.0 },
            ram: crate::data_struct::Ram { used: 1, total: 2 },
            swap: crate::data_struct::Swap { used: 0, total: 0 },
            disk: crate::data_struct::Disk { used: 1, total: 2 },
            load: crate::data_struct::Load {
                load1: 0.0,
                load5: 0.0,
                load15: 0.0,
            },
            network: crate::data_struct::Network {
                up: 1,
                down: 2,
                total_up: 3,
                total_down: 4,
            },
            connections: crate::data_struct::Connections { tcp: 1, udp: 2 },
            uptime: 3,
            process: 4,
            message: String::new(),
        });
        assert!(payload.contains("\"jsonrpc\":\"2.0\""));
        assert!(payload.contains("\"method\":\"agent.report\""));
        assert!(payload.contains("\"id\":null"));
        assert!(payload.contains("\"report\""));
        assert!(!payload.contains("ack_event_ids"));
    }

    #[test]
    fn callbacks_use_v2_method_names_and_direct_params() {
        let payload = ping_result(PingResultParams {
            task_id: 7,
            ping_type: "tcp".into(),
            value: -1,
            finished_at: "2026-01-01T00:00:00Z".into(),
        });
        assert!(payload.contains("\"method\":\"agent.pingResult\""));
        assert!(payload.contains("\"task_id\":7"));
        assert!(!payload.contains("\"type\":\"ping_result\""));
    }

    #[test]
    fn incoming_v2_exec_params_are_bindable() {
        let request = r#"{"jsonrpc":"2.0","method":"agent.exec","params":{"task_id":"t1","command":"echo ok"}}"#;
        let parsed: IncomingRequest = miniserde::json::from_str(request).unwrap();
        assert_eq!(parsed.method, AGENT_EXEC);
        assert_eq!(parsed.params.unwrap().task_id.as_deref(), Some("t1"));
    }

    #[test]
    fn incoming_rpc_accepts_numeric_ids() {
        let request = r#"{"jsonrpc":"2.0","id":42,"method":"agent.ping","params":{"ping_task_id":7,"ping_type":"tcp","ping_target":"localhost:80"}}"#;
        let parsed: IncomingRequest = miniserde::json::from_str(request).unwrap();
        assert_eq!(parsed.params.unwrap().ping_task_id, Some(7));
    }
}
