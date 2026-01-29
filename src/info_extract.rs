use crate::DocTables;
use std::fmt::Debug;
use std::fmt::Display;
use std::path::Path;

use anyhow::Result;

use tracing::trace;

#[derive(Debug)]
pub struct ExtractedInfo {
    pub header: Vec<String>,
    pub body: Vec<String>,
}

const HEADER_WORDS: &[&str] = &["PROVINCE", "DISTRICT", "SUBJECT"];

pub fn read_head(tables: &DocTables) -> Result<Vec<String>> {
    let mut ret = Vec::new();
    for term in HEADER_WORDS {
        let Some(description) = tables.find_heading_description(term) else {
            ret.push("".to_string());
            continue;
        };

        ret.push(description.to_string());
    }

    let Some(skewls) = tables.find_schools() else {
        ret.push("".to_string());
        return Ok(ret);
    };

    ret.push(skewls);

    Ok(ret)
}

pub fn read_body_info(tables: &DocTables) -> Result<Vec<String>> {
    let mut ret = Vec::new();
    for term in EXTRACT_SEARCH_TERMS_IN_ORDER {
        let Some(description) = tables.find_info_descriptions(&term) else {
            ret.push("".to_string());
            continue;
        };

        ret.push(description.to_string());
    }

    Ok(ret)
}

impl ExtractedInfo {
    pub fn into_record(self, file: &Path) -> Vec<String> {
        let Self { header, body } = self;

        let mut ret = vec![];

        ret.extend(header);
        ret.extend(body);
        ret.push(file.to_str().unwrap_or_default().to_string());

        ret
    }

    pub fn header_record() -> Vec<&'static str> {
        let mut ret = vec!["Province", "District", "Subject", "Schools"];
        for term in EXTRACT_SEARCH_TERMS_IN_ORDER {
            ret.push(term.into_main());
        }

        ret.push("File");
        ret
    }
}

const TERM_LEN: usize = 5;

#[derive(Eq, Hash, PartialEq, Clone, Copy, Debug)]
pub enum Term {
    Single(&'static str),
    Double {
        main: &'static str,
        alt: &'static str,
    },
    Many {
        main: &'static str,
        other: &'static [&'static str],
    },
}

impl Display for Term {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Single(s) => write!(f, "{s}"),
            Self::Double { main, .. } => write!(f, "{main}"),
            Self::Many { main, .. } => write!(f, "{main}"),
        }
    }
}

impl PartialEq<str> for Term {
    fn eq(&self, other: &str) -> bool {
        match self {
            Self::Single(s) => *s == other,
            Self::Double { main, alt } => *main == other || *alt == other,
            Self::Many { main, other: o } => *main == other || o.contains(&other),
        }
    }
}

impl Term {
    pub fn into_main(self) -> &'static str {
        match self {
            Self::Single(s) => s,
            Self::Double { main, .. } => main,
            Self::Many { main, .. } => main,
        }
    }

    pub fn deep_matches(&self, search_space: &str) -> bool {
        match self {
            Self::Single(s) => deep_search_term(search_space, s),
            Self::Double { main, alt } => {
                trace!("Deep searching for {main} and {alt} in {search_space}");
                deep_search_term(search_space, main) || deep_search_term(search_space, alt)
            }
            Self::Many { main, other } => {
                if deep_search_term(search_space, main) {
                    return true;
                }
                for term in *other {
                    if deep_search_term(search_space, term) {
                        return true;
                    }
                }
                false
            }
        }
    }

    pub fn is(&self, main_word: &str) -> bool {
        match self {
            Self::Single(s) => *s == main_word,
            Self::Double { main, .. } => *main == main_word,
            Self::Many { main, .. } => *main == main_word,
        }
    }
}

fn deep_search_term(search_space: &str, term: &str) -> bool {
    let lower_term = term.to_lowercase();
    let trimmed_lower_text = search_space.trim().to_lowercase();

    trimmed_lower_text == lower_term
        || trimmed_lower_text.split_whitespace().collect::<String>() == lower_term
        || trimmed_lower_text.split(':').collect::<String>() == lower_term
}

/// List of phrases in the doc that contains the info after the word
/// The order here is as they appear in the doc
const EXTRACT_SEARCH_TERMS_IN_ORDER: [Term; TERM_LEN] = [
    Term::Many {
        main: "IDENTIFICATION OF IRREGULARITIES",
        other: &[
            "IDENTIFICATION OF NON-COMPLIANCE / IRREGULARITIES",
            "SECTION F:  IDENTIFICATION OF NON-COMPLIANCE / IRREGULARITIES",
        ],
    },
    Term::Single("AREAS OF GOOD PRACTICE / INNOVATION"),
    Term::Single("AREAS THAT REQUIRE INTERVENTION AND SUPPORT"),
    Term::Double {
        main: "RECOMMENDATIONS",
        alt: "RECOMMENDATIONS FOR IMPROVEMENT",
    },
    Term::Single("CONCLUSION"),
];
