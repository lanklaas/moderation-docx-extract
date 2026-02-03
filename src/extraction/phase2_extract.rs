use tracing::debug;

use crate::extraction::ExtractedInfo;
use std::borrow::Cow;
use std::fmt::Debug;
use std::fmt::Display;
use std::path::Path;

impl ExtractedInfo {
    pub fn into_record(self, file: &Path) -> Vec<String> {
        let Self { mut record } = self;

        record.push(file.to_str().unwrap_or_default().to_string());

        record
    }

    pub fn header_record() -> Vec<&'static str> {
        let mut ret = vec![];
        for term in EXTRACT_SEARCH_TERMS_IN_ORDER {
            ret.push(term.as_main());
        }

        ret.push("File");
        ret
    }
}

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
    pub fn as_main(&self) -> &'static str {
        match self {
            Self::Single(s) => s,
            Self::Double { main, .. } => main,
            Self::Many { main, .. } => main,
        }
    }

    pub fn matches(&self, search_space: &str) -> bool {
        match self {
            Self::Single(s) => {
                if search_space.trim() == *s {
                    return true;
                }
                deep_search_term(search_space, s)
            }
            Self::Double { main, alt } => {
                search_space.trim() == *main
                    || search_space.trim() == *alt
                    || deep_search_term(search_space, main)
                    || deep_search_term(search_space, alt)
            }
            Self::Many { main, other } => {
                if search_space.trim() == *main {
                    return true;
                }
                for term in *other {
                    if search_space.trim() == *term {
                        return true;
                    }
                }
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

    pub fn word_starts_with_term(&self, word: &str) -> bool {
        match self {
            Self::Single(s) => word.trim().starts_with(s),
            Self::Double { main, alt } => {
                word.trim().starts_with(main) || word.trim().starts_with(alt)
            }
            Self::Many { main, other } => {
                if word.trim().starts_with(main) {
                    return true;
                }
                for term in *other {
                    if word.trim().starts_with(term) {
                        return true;
                    }
                }
                false
            }
        }
    }

    pub fn strip_term_from_word(&self, word: &str) -> String {
        let ret = match self {
            Self::Single(s) => word.replace(s, ""),
            Self::Double { main, alt } => word.replace(main, "").replace(alt, ""),
            Self::Many { main, other } => {
                let mut ret = word.replace(main, "");
                for term in *other {
                    ret = ret.replace(term, "");
                }
                ret
            }
        };
        if let Some(pref) = ret.strip_prefix(":") {
            return pref.trim().to_string();
        }
        ret
    }

    /// Finds the word I am looking for in a column of the table
    fn find_term_in_column<'a>(&self, rows: &'a [[String; 2]]) -> Option<Cow<'a, str>> {
        for row in rows {
            let [col1, col2] = row;

            if self.matches(col1) && !col2.is_empty() {
                return Some(col2.into());
            }
        }
        None
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
const EXTRACT_SEARCH_TERMS_IN_ORDER: [Term; 10] = [
    Term::Many {
        main: "PROVINCE",
        other: &["PROVINCE:", "Province", "Province:"],
    },
    Term::Many {
        main: "DISTRICT",
        other: &[
            "DISTRICT:",
            "District",
            "District:",
            "NAME OF DISTRICT",
            "DISTRICT 1",
        ],
    },
    Term::Many {
        main: "SUBJECT",
        other: &["SUBJECT:", "Subject", "Subject:"],
    },
    Term::Many {
        main: "SCHOOL",
        other: &[
            "SCHOOL:",
            "School",
            "School:",
            "List of Moderated Schools",
            "The schools that were moderated are",
            "The schools that were moderated are:",
        ],
    },
    Term::Single("Areas of good practice / Innovation"),
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

#[derive(Debug)]
pub struct DocTable {
    pub heading: Option<String>,
    pub rows: Vec<[String; 2]>,
}

#[derive(Debug, Default)]
pub struct DocTables {
    pub paragraphs: Vec<String>,
    pub tables: Vec<DocTable>,
}

impl DocTables {
    pub fn try_into_extracted(self) -> ExtractedInfo {
        let mut record = vec![];
        for term in EXTRACT_SEARCH_TERMS_IN_ORDER {
            let mut found_it = false;
            debug!("Searching for term: {term}");

            for DocTable { heading, rows } in &self.tables {
                let Some(heading) = heading.as_ref() else {
                    debug!("No heading for table. Looking in column");
                    if let Some(text) = term.find_term_in_column(rows) {
                        found_it = true;
                        record.push(text.to_string());
                        break;
                    }

                    continue;
                };

                if term.matches(heading) {
                    // With the district column on oral, the table heading has the word district
                    // but the actual info is contained in the second column cell of the first row
                    if first_column_contains_term(rows, &term)
                        && let Some(text) = term.find_term_in_column(rows)
                    {
                        found_it = true;
                        record.push(text.to_string());
                        break;
                    }
                    record.push(
                        rows.iter()
                            .map(|x| {
                                x.iter()
                                    .filter(|x| !x.is_empty())
                                    .map(ToString::to_string)
                                    .collect::<String>()
                            })
                            .collect::<Vec<String>>()
                            .join("\n"),
                    );
                    found_it = true;
                    break;
                }
            }
            if !found_it {
                debug!("No heading for table. Looking in paragraphs");
                if let Some(word) = self.find_in_paragraphs(&term) {
                    record.push(word);
                    debug!("Found term: {found_it}");
                    continue;
                }
                debug!("Found term: {found_it}");
                record.push("".to_string());
            }
        }
        ExtractedInfo { record }
    }

    fn find_in_paragraphs(&self, term: &Term) -> Option<String> {
        for par in &self.paragraphs {
            if term.word_starts_with_term(par) {
                return Some(term.strip_term_from_word(par));
            }
        }
        None
    }
}

fn first_column_contains_term(rows: &[[String; 2]], term: &Term) -> bool {
    for row in rows {
        let [col1, ..] = row;
        if term.matches(col1) {
            return true;
        }
    }
    false
}
