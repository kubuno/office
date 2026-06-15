use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PmNode {
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub attrs: Option<Value>,
    #[serde(default)]
    pub content: Option<Vec<PmNode>>,
    #[serde(default)]
    pub marks: Option<Vec<PmMark>>,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PmMark {
    #[serde(rename = "type")]
    pub mark_type: String,
    #[serde(default)]
    pub attrs: Option<Value>,
}

impl PmNode {
    pub fn doc(content: Vec<PmNode>) -> Self {
        PmNode { node_type: "doc".into(), attrs: None, content: Some(content), marks: None, text: None }
    }

    pub fn paragraph(content: Vec<PmNode>) -> Self {
        PmNode { node_type: "paragraph".into(), attrs: None, content: Some(content), marks: None, text: None }
    }

    pub fn heading(level: u8, content: Vec<PmNode>) -> Self {
        PmNode {
            node_type: "heading".into(),
            attrs: Some(serde_json::json!({ "level": level })),
            content: Some(content),
            marks: None,
            text: None,
        }
    }

    pub fn text(s: impl Into<String>, marks: Vec<PmMark>) -> Self {
        PmNode {
            node_type: "text".into(),
            attrs: None,
            content: None,
            marks: if marks.is_empty() { None } else { Some(marks) },
            text: Some(s.into()),
        }
    }

    pub fn hard_break() -> Self {
        PmNode { node_type: "hardBreak".into(), attrs: None, content: None, marks: None, text: None }
    }

    pub fn bullet_list(items: Vec<PmNode>) -> Self {
        PmNode { node_type: "bulletList".into(), attrs: None, content: Some(items), marks: None, text: None }
    }

    pub fn ordered_list(items: Vec<PmNode>) -> Self {
        PmNode { node_type: "orderedList".into(), attrs: None, content: Some(items), marks: None, text: None }
    }

    pub fn list_item(content: Vec<PmNode>) -> Self {
        PmNode { node_type: "listItem".into(), attrs: None, content: Some(content), marks: None, text: None }
    }

    pub fn blockquote(content: Vec<PmNode>) -> Self {
        PmNode { node_type: "blockquote".into(), attrs: None, content: Some(content), marks: None, text: None }
    }

    pub fn code_block(text: impl Into<String>) -> Self {
        let inner = PmNode::text(text, vec![]);
        PmNode { node_type: "codeBlock".into(), attrs: None, content: Some(vec![inner]), marks: None, text: None }
    }

    pub fn children(&self) -> &[PmNode] {
        self.content.as_deref().unwrap_or(&[])
    }

    pub fn has_mark(&self, mark_type: &str) -> bool {
        self.marks.as_ref().map_or(false, |marks| marks.iter().any(|m| m.mark_type == mark_type))
    }

    pub fn heading_level(&self) -> u8 {
        self.attrs.as_ref()
            .and_then(|a| a.get("level"))
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u8
    }
}
