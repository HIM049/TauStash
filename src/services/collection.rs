use serde::{Deserialize, Serialize};

use crate::database::entities::TagNamespace;
use crate::services::macrotag::MacroTag;

fn default_true() -> bool {
    true
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
pub enum GroupRule {
    #[default]
    None,
    CreateTime,
    UpdateTime,
    Size,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrderRule {
    #[default]
    CreateTime,
    FileName,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ViewOption {
    #[default]
    Grid,
    List,
}

/// for the paramters only need when open
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionConfig {
    pub filters: Vec<MacroTag>,
    pub view: ViewOption,
    pub group_by: GroupRule,
    pub order_by: OrderRule,
    /// Default should be DESC
    pub is_order_revert: bool,
    pub is_hide_tagbar: bool,
    #[serde(default)]
    pub pinned_tags: Vec<MacroTag>,
    #[serde(default = "default_true")]
    pub include_view_tags: bool,
    #[serde(default)]
    pub candidate_filter: Option<CandidateTagRule>,
    #[serde(default)]
    pub enable_drag: bool,
}

impl Default for CollectionConfig {
    fn default() -> Self {
        Self {
            filters: Vec::new(),
            view: ViewOption::default(),
            group_by: GroupRule::default(),
            order_by: OrderRule::default(),
            is_order_revert: false,
            is_hide_tagbar: false,
            pinned_tags: Vec::new(),
            include_view_tags: true,
            candidate_filter: None,
            enable_drag: false,
        }
    }
}
