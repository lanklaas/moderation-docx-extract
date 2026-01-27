use crate::Block;
use crate::HeaderInfo;
use crate::XmlDoc;
use std::collections::HashMap;
use std::fmt::Debug;
use std::path::PathBuf;

use anyhow::Result;
use derive_builder::Builder;

use serde::Serialize;

#[derive(Debug)]
pub struct ExtractedInfo {
    pub header: HeaderInfo,
    pub body: HashMap<&'static str, String>,
    pub file: PathBuf,
}

pub fn read_body_info(doc: &mut XmlDoc) -> Result<HashMap<&'static str, String>> {
    let mut ret = HashMap::new();
    let blocks = doc.extract_doc_blocks()?;
    for term in EXTRACT_SEARCH_TERMS_IN_ORDER.iter().take(TERM_LEN) {
        let Some(Block::Table(t)) = blocks.find_term_table_text(term) else {
            todo!("Search case insensitive for term {term}, or unexpected non table after term");
        };

        ret.insert(*term, t.join(""));
    }

    Ok(ret)
}

impl ExtractedInfo {
    pub fn into_record(self) -> Vec<String> {
        todo!()
        // let Self {
        //     header:
        //         HeaderInfo {
        //             province,
        //             district,
        //             school,
        //             subject,
        //         },
        //     file,
        //     mut body,
        // } = self;
        // let school = school.as_deref().unwrap_or_default();

        // let mut ret = vec![
        //     province,
        //     district,
        //     school.to_string(),
        //     subject.unwrap_or("Subject not found".to_string()),
        //     // identification_of_irregularities
        //     //     .as_deref()
        //     //     .unwrap_or("Not Found"),
        //     // areas_of_good_practice_innovation
        //     //     .as_deref()
        //     //     .unwrap_or("Not Found"),
        //     // areas_that_require_intervention_and_support,
        //     // recommendations,
        // ];
        // ret.append(&mut body);
        // ret.push(file.to_str().unwrap_or_default().to_string());

        // ret
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
