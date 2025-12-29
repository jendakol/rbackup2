use crate::error::{BackupError, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupStats {
    pub files_new: i32,
    pub files_changed: i32,
    pub files_unmodified: i32,
    pub dirs_new: i32,
    pub dirs_changed: i32,
    pub dirs_unmodified: i32,
    pub data_added_bytes: i64,
    pub total_files_processed: i32,
    pub total_bytes_processed: i64,
    pub snapshot_id: String,
}

#[derive(Debug, Deserialize)]
struct ResticMessageType {
    message_type: String,
}

#[derive(Debug, Deserialize)]
struct ResticSummary {
    files_new: Option<i32>,
    files_changed: Option<i32>,
    files_unmodified: Option<i32>,
    dirs_new: Option<i32>,
    dirs_changed: Option<i32>,
    dirs_unmodified: Option<i32>,
    data_added: Option<i64>,
    total_files_processed: Option<i32>,
    total_bytes_processed: Option<i64>,
    snapshot_id: Option<String>,
}

pub fn parse_restic_json_output(stdout: &str) -> Result<BackupStats> {
    let mut summary: Option<ResticSummary> = None;

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let msg_type: ResticMessageType = match serde_json::from_str(line) {
            Ok(mt) => mt,
            Err(e) => {
                debug!("Failed to parse message type from line: {} - {}", line, e);
                continue;
            }
        };

        if msg_type.message_type == "summary" {
            match serde_json::from_str::<ResticSummary>(line) {
                Ok(s) => {
                    summary = Some(s);
                    break;
                }
                Err(e) => {
                    warn!("Failed to parse summary: {} - line: {}", e, line);
                }
            }
        }
    }

    let summary = summary.ok_or_else(|| {
        BackupError::OutputParseFailed("No summary message found in restic output".to_string())
    })?;

    let snapshot_id = summary
        .snapshot_id
        .ok_or_else(|| BackupError::OutputParseFailed("No snapshot_id in summary".to_string()))?;

    Ok(BackupStats {
        files_new: summary.files_new.unwrap_or(0),
        files_changed: summary.files_changed.unwrap_or(0),
        files_unmodified: summary.files_unmodified.unwrap_or(0),
        dirs_new: summary.dirs_new.unwrap_or(0),
        dirs_changed: summary.dirs_changed.unwrap_or(0),
        dirs_unmodified: summary.dirs_unmodified.unwrap_or(0),
        data_added_bytes: summary.data_added.unwrap_or(0),
        total_files_processed: summary.total_files_processed.unwrap_or(0),
        total_bytes_processed: summary.total_bytes_processed.unwrap_or(0),
        snapshot_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_restic_json_output() {
        let json_output = r#"{"message_type":"status","percent_done":0.5,"total_files":100}
{"message_type":"summary","files_new":10,"files_changed":5,"files_unmodified":85,"dirs_new":2,"dirs_changed":1,"dirs_unmodified":8,"data_added":1048576,"total_files_processed":100,"total_bytes_processed":10485760,"snapshot_id":"abc123def456"}"#;

        let result = parse_restic_json_output(json_output);
        assert!(result.is_ok());

        let stats = result.expect("Failed to parse stats");
        assert_eq!(stats.files_new, 10);
        assert_eq!(stats.files_changed, 5);
        assert_eq!(stats.files_unmodified, 85);
        assert_eq!(stats.data_added_bytes, 1048576);
        assert_eq!(stats.snapshot_id, "abc123def456");
    }

    #[test]
    fn test_parse_restic_json_output_missing_summary() {
        let json_output = r#"{"message_type":"status","percent_done":0.5,"total_files":100}"#;

        let result = parse_restic_json_output(json_output);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_restic_json_output_missing_snapshot_id() {
        let json_output =
            r#"{"message_type":"summary","files_new":10,"files_changed":5,"files_unmodified":85}"#;

        let result = parse_restic_json_output(json_output);
        assert!(result.is_err());
    }
}
