pub(crate) fn default_lifecycle_status() -> String {
    "active".to_string()
}

pub(crate) fn default_version() -> i64 {
    1
}

pub(crate) fn is_active_lifecycle(s: &str) -> bool {
    s == "active"
}
