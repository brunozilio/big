#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PermissionsInfo {
    pub added_dirs: Vec<String>,
    #[serde(default)]
    pub hooks: Vec<HookInfo>,
    #[serde(default)]
    pub interceptors: Vec<InterceptorInfo>,
    #[serde(default)]
    pub notices: Vec<String>,
    #[serde(default)]
    pub skipped_files: Vec<std::path::PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HookInfo {
    pub event: crate::hooks::HookEvent,
    pub matcher: String,
    pub runs: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InterceptorInfo {
    pub kind: String,
    pub matcher: String,
    pub runs: String,
}
