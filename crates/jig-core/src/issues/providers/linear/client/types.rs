use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct NodeList<T> {
    pub nodes: Vec<T>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawIssue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub url: String,
    pub priority: u8,
    pub branch_name: Option<String>,
    pub state: RawState,
    pub project: Option<RawProject>,
    pub team: RawTeam,
    pub parent: Option<RawParentRef>,
    pub children: NodeList<RawChildRef>,
    pub labels: NodeList<RawLabel>,
    pub inverse_relations: NodeList<RawRelation>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawParentRef {
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub branch_name: Option<String>,
    pub state: RawState,
}

#[derive(Debug, Deserialize)]
pub struct RawState {
    #[serde(rename = "type")]
    pub state_type: String,
}

#[derive(Debug, Deserialize)]
pub struct RawProject {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct RawTeam {
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawChildRef {
    pub identifier: String,
    pub branch_name: Option<String>,
    pub state: Option<RawState>,
}

#[derive(Debug, Deserialize)]
pub struct RawLabel {
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawRelation {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub relation_type: String,
    pub issue: RawChildRef,
    #[allow(dead_code)]
    pub related_issue: RawChildRef,
}
