use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(Debug, Deserialize, Serialize)]
pub struct ElementDetails {
    pub value: String,
    pub role: String,
    pub subrole: String,
    pub description: String,
    pub identifier: String,
    pub title: String,
    pub help: String,
    pub position: String,
    pub size: String,
}