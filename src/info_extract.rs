use crate::Block;
use crate::DocBlocks;
use std::collections::HashMap;
use std::fmt::Debug;
use std::fmt::Display;
use std::mem;
use std::path::Path;

use anyhow::Result;
use anyhow::bail;
use derive_builder::Builder;

use serde::Serialize;
use tracing::trace;

#[derive(Debug)]
pub struct ExtractedInfo {
    pub header: HashMap<&'static str, String>,
    pub body: HashMap<&'static str, String>,
}

const HEADER_WORDS: &[&str] = &["PROVINCE", "DISTRICT", "SCHOOL", "SUBJECT"];

pub fn read_head(blocks: &DocBlocks) -> Result<HashMap<&'static str, String>> {
    let Some(Block::Table(t)) = blocks.find_table_containing_one_of(HEADER_WORDS) else {
        bail!("This doc does not have any of the header terms.");
    };
    let mut t = t.clone();

    // Normalize to district
    t.iter_mut()
        .filter(|x| *x == "DISTRICT/REGION")
        .for_each(|x| {
            *x = "DISTRICT".to_string();
        });

    // Normalize extra chars
    t.iter_mut()
        .filter(|x| {
            let fixed = x
                .split_whitespace()
                .collect::<String>()
                .to_uppercase()
                .split(':')
                .collect::<String>();
            HEADER_WORDS.contains(&fixed.as_str())
        })
        .for_each(|x| {
            *x = x
                .split_whitespace()
                .collect::<String>()
                .to_uppercase()
                .split(':')
                .collect::<String>()
        });

    let mut ret = HashMap::new();

    for word in HEADER_WORDS {
        let Some(pos) = t.iter().position(|x| x == word) else {
            ret.insert(*word, "".to_string());
            continue;
        };
        let val = t
            .get_mut(pos + 1)
            .expect("Next text to be the value of the word");
        let val = mem::take(val);
        ret.insert(*word, val);
    }

    if ret.len() < 4 {
        for word in HEADER_WORDS {
            if ret.contains_key(word) {
                continue;
            }
            ret.insert(*word, String::new());
        }
    }
    Ok(ret)
}

pub fn read_body_info(blocks: &DocBlocks) -> Result<HashMap<&'static str, String>> {
    let mut ret = HashMap::new();
    for term in EXTRACT_SEARCH_TERMS_IN_ORDER {
        match blocks.find_term_table_text(&term) {
            Some(Block::Table(t)) => {
                // Several sections might be in the table, so I need to scan again and slice it up.
                // This will not be very performant, but should do for the small amount of times
                // I have to run this app
                let Some(pos) = t.iter().position(|x| term.deep_matches(x)) else {
                    ret.insert(term.into_main(), t.join(""));
                    continue;
                };
                let mut first_term_after_me = None;
                for (i, word) in t.iter().enumerate().skip(pos) {
                    for term in EXTRACT_SEARCH_TERMS_IN_ORDER.iter().filter(|x| **x != term) {
                        if !term.deep_matches(word) {
                            continue;
                        }
                        first_term_after_me = Some(i);
                    }
                }

                if let Some(next_term_pos) = first_term_after_me {
                    ret.insert(term.into_main(), t[pos + 1..next_term_pos].join("\n"));
                } else {
                    ret.insert(term.into_main(), t[pos..].join("\n"));
                }
            }
            Some(Block::Paragraph(p)) => {
                ret.insert(term.into_main(), p.to_string());
            }
            None => {
                ret.insert(term.into_main(), "".to_string());
                continue;
            }
        }
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
            ret.push(term.into_main());
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
