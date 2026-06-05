use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TaskStatus {
    #[default]
    Open,
    InProgress,
    Done,
    Cancelled,
    User(String),
}

impl FromStr for TaskStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "open" => Self::Open,
            "in_progress" => Self::InProgress,
            "done" => Self::Done,
            "cancelled" => Self::Cancelled,
            other => Self::User(other.to_string()),
        })
    }
}

impl TaskStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Open => "open",
            Self::InProgress => "in_progress",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::User(s) => s.as_str(),
        }
    }
}

impl Serialize for TaskStatus {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TaskStatus {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(Self::from_str(&s).unwrap())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TaskPriority {
    Low,
    #[default]
    Medium,
    High,
    User(String),
}

impl FromStr for TaskPriority {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            other => Self::User(other.to_string()),
        })
    }
}

impl TaskPriority {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::User(s) => s.as_str(),
        }
    }
}

impl Serialize for TaskPriority {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TaskPriority {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(Self::from_str(&s).unwrap())
    }
}

fn is_active_lifecycle(s: &str) -> bool {
    s == "active"
}

/// Wire shape for task nodes sent to the frontend.
///
/// Produced by `node_to_typed_value` for `node_type == "task"`. Fields map
/// directly to the TypeScript `TaskNode` interface.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskNode {
    pub id: String,
    #[serde(rename = "nodeType")]
    pub node_type: String,
    pub content: String,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub properties: serde_json::Value,
    #[serde(default, skip_serializing_if = "is_active_lifecycle")]
    pub lifecycle_status: String,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<TaskPriority>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
}

/// Partial update for task-specific properties, received from the frontend.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskNodeUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<TaskStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<Option<TaskPriority>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "flexible_date::deserialize_with_null"
    )]
    pub due_date: Option<Option<DateTime<Utc>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "flexible_date::deserialize_with_null"
    )]
    pub started_at: Option<Option<DateTime<Utc>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "flexible_date::deserialize_with_null"
    )]
    pub completed_at: Option<Option<DateTime<Utc>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// Flexible date deserializer: accepts ISO8601 full timestamps, date-only
/// "YYYY-MM-DD" strings, and explicit JSON null (which clears the field).
///
/// The `Option<Option<T>>` pattern distinguishes three cases:
/// - Field absent from JSON → `None` (don't change)
/// - Field present as `null` → `Some(None)` (clear the value)
/// - Field present as a string → `Some(Some(dt))` (set to this value)
pub(crate) mod flexible_date {
    use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
    use serde::{Deserialize, Deserializer};

    pub fn deserialize_with_null<'de, D>(
        deserializer: D,
    ) -> Result<Option<Option<DateTime<Utc>>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            None => Ok(Some(None)),
            Some(s) => parse_date_string(&s).map_err(serde::de::Error::custom),
        }
    }

    fn parse_date_string(s: &str) -> Result<Option<Option<DateTime<Utc>>>, String> {
        if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
            return Ok(Some(Some(dt.with_timezone(&Utc))));
        }
        if let Ok(dt) = s.parse::<DateTime<Utc>>() {
            return Ok(Some(Some(dt)));
        }
        if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
            let datetime = date.and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap());
            return Ok(Some(Some(DateTime::from_naive_utc_and_offset(
                datetime, Utc,
            ))));
        }
        Err(format!(
            "Invalid date format: '{}'. Expected YYYY-MM-DD or ISO8601",
            s
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_node_update_null_clears_due_date() {
        let json = r#"{"dueDate": null}"#;
        let update: TaskNodeUpdate = serde_json::from_str(json).unwrap();
        assert_eq!(update.due_date, Some(None));
    }

    #[test]
    fn task_node_update_absent_due_date_is_none() {
        let json = r#"{}"#;
        let update: TaskNodeUpdate = serde_json::from_str(json).unwrap();
        assert_eq!(update.due_date, None);
    }

    #[test]
    fn task_node_update_iso8601_due_date() {
        let json = r#"{"dueDate": "2025-06-15T00:00:00Z"}"#;
        let update: TaskNodeUpdate = serde_json::from_str(json).unwrap();
        assert!(matches!(update.due_date, Some(Some(_))));
    }

    #[test]
    fn task_node_update_date_only_due_date() {
        let json = r#"{"dueDate": "2025-06-15"}"#;
        let update: TaskNodeUpdate = serde_json::from_str(json).unwrap();
        assert!(matches!(update.due_date, Some(Some(_))));
    }
}
