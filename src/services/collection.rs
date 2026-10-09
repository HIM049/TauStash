use serde::{Deserialize, Serialize};

use crate::services::macrotag::MacroTag;

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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
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
}
