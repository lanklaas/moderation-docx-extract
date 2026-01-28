use crate::Block;
use crate::DocBlocks;
use std::collections::HashMap;
use std::fmt::Debug;
use std::path::Path;

use anyhow::Result;
use derive_builder::Builder;

use serde::Serialize;
use tracing::debug;

#[derive(Debug)]
pub struct ExtractedInfo {
    pub header: HashMap<&'static str, String>,
    pub body: HashMap<&'static str, String>,
}

const HEADER_WORDS: &[&str] = &[
    "PROVINCE",
    "DISTRICT",
    "DISTRICT/REGION",
    "SCHOOL",
    "SUBJECT",
];

pub fn read_head(blocks: &DocBlocks) -> HashMap<&'static str, String> {
    let Some(Block::Table(t)) = blocks.find_table_containing_one_of(HEADER_WORDS) else {
        todo!("Search case insensitive for term first table, or unexpected non table after term");
    };

    let mut ret = HashMap::new();
    for (i, word) in t.iter().enumerate() {
        match word.trim() {
            "PROVINCE" | "PROVINCE:" => {
                let prov = t.get(i + 1);
                ret.insert("Province", prov.cloned().unwrap_or("NULL".to_string()));
            }
            "DISTRICT" | "DISTRICT/REGION" | "DISTRICT:" => {
                let prov = t.get(i + 1);
                ret.insert("District", prov.cloned().unwrap_or("NULL".to_string()));
            }
            "SCHOOL" | "SCHOOL:" => {
                let prov = t.get(i + 1);
                ret.insert("School", prov.cloned().unwrap_or("NULL".to_string()));
            }
            "SUBJECT" | "SUBJECT:" => {
                let prov = t.get(i + 1);
                ret.insert("Subject", prov.cloned().unwrap_or("NULL".to_string()));
            }
            other => debug!("{other} text found in header table"),
        }
    }
    ret
}

pub fn read_body_info(blocks: &DocBlocks) -> Result<HashMap<&'static str, String>> {
    let mut ret = HashMap::new();
    for term in EXTRACT_SEARCH_TERMS_IN_ORDER.iter().take(TERM_LEN) {
        let Some(Block::Table(t)) = blocks.find_term_table_text(term) else {
            todo!("Search case insensitive for term {term}, or unexpected non table after term");
        };

        ret.insert(*term, t.join(""));
    }

    Ok(ret)
}

impl ExtractedInfo {
    pub fn into_record(self, file: &Path) -> Vec<String> {
        let Self { header, body } = self;

        let mut ret = vec![];
        ret.extend(header.into_values());
        ret.extend(body.into_values());
        ret.push(file.to_str().unwrap_or_default().to_string());

        ret
    }

    pub fn header_record() -> Vec<&'static str> {
        let mut ret = vec![
            "Province", "District", "School",
            "Subject",
            // "Areas That Require Intervention And Support",
            // "Recommendations For Improvement",
            // "File",
        ];
        for term in EXTRACT_SEARCH_TERMS_IN_ORDER.iter().take(TERM_LEN) {
            ret.push(term);
        }
        ret.push("File");
        ret
    }
}

#[derive(Builder, Debug, Serialize)]
#[builder_struct_attr(derive(Debug))]
pub struct ExtractInfo {
    pub identification_of_irregularities: Option<String>,
    pub areas_that_require_intervention_and_support: String,
    pub recommendations: String,
    pub areas_of_good_practice_innovation: Option<String>,
}

const TERM_LEN: usize = 2;

/// List of phrases in the doc that contains the info after the word
/// The order here is as they appear in the doc
const EXTRACT_SEARCH_TERMS_IN_ORDER: [&str; TERM_LEN] = [
    "IDENTIFICATION OF IRREGULARITIES",
    "AREAS THAT REQUIRE INTERVENTION AND SUPPORT",
];
