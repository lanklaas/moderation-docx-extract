pub mod info_extract;

use docx_rs::{
    DocumentChild, Paragraph, ParagraphChild, RunChild, TableCellContent, TableChild,
    TableRowChild, read_docx,
};
use docx_rs::{Docx, Table};
use info_extract::Term;
use state::{IsXml, Loaded, NotLoaded};
use std::fs::File;
use std::io::{BufReader, Read};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use anyhow::Result;

use tracing::{debug, info, trace};

pub type UnloadedDoc = DocBytes<NotLoaded>;
pub type LoadedDoc = DocBytes<Loaded>;
pub type XmlDoc = DocBytes<IsXml>;

#[derive(Debug)]
pub struct DocBytes<S> {
    buf: Vec<u8>,
    path: PathBuf,
    _state: S,
}

impl Default for UnloadedDoc {
    fn default() -> Self {
        Self {
            buf: vec![],
            path: PathBuf::new(),
            _state: NotLoaded,
        }
    }
}

pub mod state {
    use docx_rs::Docx;

    pub struct NotLoaded;
    pub struct Loaded;
    pub struct IsXml {
        pub(super) xml_doc: Docx,
    }
}

impl UnloadedDoc {
    pub fn from_path(self, path: PathBuf) -> Result<LoadedDoc> {
        let Self { mut buf, .. } = self;
        let mut rd = BufReader::new(File::open(&path)?);
        rd.read_to_end(&mut buf)?;
        Ok(LoadedDoc {
            buf,
            path,
            _state: Loaded,
        })
    }
}

impl LoadedDoc {
    pub fn read_docx(self) -> Result<XmlDoc> {
        let Self { buf, path, .. } = self;
        let doc = read_docx(&buf)?;
        Ok(DocBytes {
            buf,
            path,
            _state: IsXml { xml_doc: doc },
        })
    }
}

#[derive(Debug)]
pub enum Block {
    Paragraph(String),
    Table(Vec<String>), // or just String if you prefer
}

impl Block {
    pub fn is_paragraph(&self) -> bool {
        matches!(self, Self::Paragraph(_))
    }
}

impl XmlDoc {
    pub fn file(&self) -> &Path {
        self.path.as_path()
    }
    pub fn unload(self) -> UnloadedDoc {
        let Self { mut buf, .. } = self;
        buf.clear();
        DocBytes {
            buf,
            path: PathBuf::new(),
            _state: NotLoaded,
        }
    }
    pub fn extract_doc_blocks(&mut self) -> Result<DocBlocks> {
        let mut blocks = Vec::new();

        // Take the children so we can own them (and not reverse them)
        let children = std::mem::take(&mut self.document.children);

        for child in children {
            match child {
                DocumentChild::Paragraph(p) => {
                    let text = extract_paragraph_text(*p);
                    if !text.trim().is_empty() {
                        blocks.push(Block::Paragraph(text));
                    }
                }
                DocumentChild::Table(t) => {
                    let mut paras_in_table = Vec::new();
                    collect_table_paragraphs(*t, &mut paras_in_table);

                    let mut table_text = Vec::new();
                    for p in paras_in_table {
                        let text = extract_paragraph_text(p);
                        if !text.trim().is_empty() {
                            table_text.push(text);
                        }
                    }

                    if !table_text.is_empty() {
                        blocks.push(Block::Table(table_text));
                    }
                }
                other => {
                    debug!("Unhandled document child: {other:?}");
                }
            }
        }

        Ok(DocBlocks(blocks))
    }
}

impl Deref for XmlDoc {
    type Target = Docx;
    fn deref(&self) -> &Self::Target {
        &self._state.xml_doc
    }
}

impl DerefMut for XmlDoc {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self._state.xml_doc
    }
}

pub struct DocBlocks(Vec<Block>);

impl DocBlocks {
    pub fn find_table_containing_one_of(&self, text: &[&str]) -> Option<&Block> {
        let Some(opt) = self.0.iter().find(|x| match x {
            Block::Paragraph(_) => false,
            Block::Table(t) => t.iter().any(|x| text.contains(&x.trim())),
        }) else {
            info!("Running deep search for table terms");
            return self.deep_find_table_containing_one_of(text);
        };
        Some(opt)
    }

    /// Tries case insensitive, without whitespace and adds some common characters
    pub fn deep_find_table_containing_one_of(&self, text: &[&str]) -> Option<&Block> {
        self.0.iter().find(|x| match x {
            Block::Paragraph(_) => false,
            Block::Table(t) => t.iter().any(|x| {
                let trimmed_upper_text_whitespace_removed = x.split_whitespace().collect::<String>().to_uppercase();

                text.contains(&trimmed_upper_text_whitespace_removed.as_str())
                    || text.contains(
                        &trimmed_upper_text_whitespace_removed
                            .split_whitespace()
                            .collect::<String>()
                            .as_str(),
                    )
                    || text.contains(&trimmed_upper_text_whitespace_removed.split(':').collect::<String>().as_str())
                    // Some docs has DISTRICT/REGOIN instead of just DISTRICT
                    || text.contains(&format!("{trimmed_upper_text_whitespace_removed}/REGION").as_str())
            }),
        })
    }

    pub fn find_term_table_text(&self, term: &Term) -> Option<&Block> {
        let Some(position) = self.0.iter().position(|x| {
            if !x.is_paragraph() {
                return false;
            }
            let Block::Paragraph(p) = x else {
                unreachable!()
            };
            term == p.trim()
        }) else {
            info!("Running deep search for: {term}");
            return self.deep_find_term_table(term);
        };

        self.0.get(position + 1)
    }

    /// Tries case insensitive, without whitespace and adds some common characters
    pub fn deep_find_term_table(&self, term: &Term) -> Option<&Block> {
        let Some(position) = self.0.iter().position(|x| {
            // if !x.is_paragraph() {
            //     return false;
            // }
            // let Block::Paragraph(p) = x else {
            //     unreachable!()
            // };
            match x {
                Block::Paragraph(p) => term.deep_matches(p),
                Block::Table(t) => {
                    for word in t {
                        trace!("Looking for {term} in {word}");
                        if term.deep_matches(word) {
                            return true;
                        }
                    }
                    false
                }
            }
        }) else {
            debug!("Term: {term} was not found in doc.");
            return None;
        };

        match self.0.get(position) {
            Some(thing) => {
                // If the search text was in a table (Like in the left column, return the table instead of the next table)
                if !thing.is_paragraph() {
                    return Some(thing);
                }

                self.0.get(position + 1)
            }
            _ => self.0.get(position + 1),
        }
    }
}

fn collect_table_paragraphs(table: Table, acc: &mut Vec<Paragraph>) {
    for row in table.rows {
        let TableChild::TableRow(row) = row;
        for cell in row.cells {
            let TableRowChild::TableCell(c) = cell;

            for cell_child in c.children {
                match cell_child {
                    TableCellContent::Paragraph(p) => acc.push(p),
                    TableCellContent::Table(t) => {
                        // recursion for nested tables
                        collect_table_paragraphs(t, acc);
                    }
                    other => debug!("Unhandled collect_table_paragraphs: {other:?}"),
                }
            }
        }
    }
}

pub fn extract_paragraph_text(p: Paragraph) -> String {
    let mut ret = vec![];
    for par in p.children {
        match par {
            ParagraphChild::Run(r) => {
                for run in r.children {
                    match run {
                        RunChild::Text(t) => {
                            ret.push(t.text);
                        }
                        other => debug!("Unhandled run found: {other:?}"),
                    }
                }
            }
            other => debug!("Unhandled element found: {other:?}"),
        }
    }
    ret.join("")
}
