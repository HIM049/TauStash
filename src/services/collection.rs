use serde::{Deserialize, Serialize};

use crate::database::entities::TagNamespace;
use crate::database::storage::query::FileSortField;
use crate::services::macrotag::{FileState, MacroTag};

/// States a view can offer in its context menu, in display order.
pub const CONTEXT_MENU_STATES: &[FileState] =
    &[FileState::Managed, FileState::Ignored, FileState::Deleted];

fn default_true() -> bool {
    true
}

fn default_context_states() -> Vec<FileState> {
    vec![FileState::Deleted]
}

fn default_candidate_limit() -> usize {
    12
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateTagRule {
    #[serde(default)]
    pub namespaces: Vec<TagNamespace>,
    #[serde(default = "default_candidate_limit")]
    pub limit: usize,
}

impl Default for CandidateTagRule {
    fn default() -> Self {
        Self {
            namespaces: vec![TagNamespace::User],
            limit: 12,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TimeGroup {
    #[default]
    Day,
    Month,
    Year,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GroupMode {
    #[default]
    None,
    Time(TimeGroup),
    Size,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ViewOption {
    #[default]
    Grid,
    List,
    Gallery,
}

/// for the paramters only need when open
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionConfig {
    pub filters: Vec<MacroTag>,
    pub view: ViewOption,
    #[serde(default)]
    pub sort_field: FileSortField,
    #[serde(default = "default_true")]
    pub is_desc: bool,
    #[serde(default)]
    pub group_mode: GroupMode,
    pub is_hide_tagbar: bool,
    #[serde(default)]
    pub pinned_tags: Vec<MacroTag>,
    #[serde(default = "default_true")]
    pub include_view_tags: bool,
    #[serde(default)]
    pub candidate_filter: Option<CandidateTagRule>,
    #[serde(default)]
    pub enable_drag: bool,
    #[serde(default = "default_context_states")]
    pub context_states: Vec<FileState>,
}

impl Default for CollectionConfig {
    fn default() -> Self {
        Self {
            filters: Vec::new(),
            view: ViewOption::default(),
            sort_field: FileSortField::default(),
            is_desc: true,
            group_mode: GroupMode::default(),
            is_hide_tagbar: false,
            pinned_tags: Vec::new(),
            include_view_tags: true,
            candidate_filter: None,
            enable_drag: false,
            context_states: default_context_states(),
        }
    }
}
