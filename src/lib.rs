pub mod info_extract;

use docx_rs::{
    DocumentChild, Paragraph, ParagraphChild, RunChild, TableCellContent, TableChild,
    TableRowChild, read_docx,
};
use docx_rs::{Docx, Table};
use info_extract::Term;
use state::{IsXml, Loaded, NotLoaded};
use std::borrow::Cow;
use std::fs::File;
use std::io::{BufReader, Read};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use anyhow::Result;

use tracing::{debug, trace};

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
    pub fn extract_doc_tables(&mut self) -> Result<DocTables> {
        let mut ret = Vec::new();

        // Take the children so we can own them (and not reverse them)
        let children = std::mem::take(&mut self.document.children);

        let mut heading = None;
        for (i, child) in children.clone().into_iter().enumerate() {
            let next = children.get(i + 1);
            let nextnext = children.get(i + 2);
            match child {
                DocumentChild::Paragraph(p) => {
                    let text = extract_paragraph_text(*p);
                    match (next, nextnext) {
                        (Some(DocumentChild::Table(t)), _) if !text.trim().is_empty() => {
                            heading = Some(text);
                        }
                        (_, Some(DocumentChild::Table(t))) if !text.trim().is_empty() => {
                            heading = Some(text);
                        }

                        other => {
                            trace!("{other:?} -> other element found.");
                        }
                    }
                }
                DocumentChild::Table(t) => {
                    let rows = collect_tables(*t);
                    let heading = heading.take();
                    ret.push(DocTable { heading, rows })
                }
                other => {
                    debug!("Unhandled document child: {other:?}");
                }
            }
        }

        Ok(DocTables(ret))
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

#[derive(Debug)]
pub struct DocTable {
    heading: Option<String>,
    rows: Vec<[String; 2]>,
}

#[derive(Debug)]
pub struct DocTables(Vec<DocTable>);

impl DocTables {
    pub fn find_heading_description(&self, heading: &str) -> Option<Cow<'_, str>> {
        for table in &self.0 {
            for row in &table.rows {
                let [col1, col2] = row;

                if col1 == heading {
                    return Some(col2.into());
                }

                let col1_deep = col1
                    .split_whitespace()
                    .flat_map(|x| x.chars().filter(|x| *x != ':'))
                    .collect::<String>()
                    .to_uppercase();

                if col1_deep == heading.to_uppercase() {
                    return Some(col2.into());
                }
            }
        }
        None
    }

    pub fn find_schools(&self) -> Option<String> {
        for table in &self.0 {
            let Some(pos) = table
                .rows
                .iter()
                .position(|[.., col2]| col2 == "List of Moderated Schools")
            else {
                continue;
            };

            return Some(
                table.rows[pos + 1..]
                    .iter()
                    .map(|[.., col2]| col2.to_string())
                    .collect::<Vec<String>>()
                    .join(", "),
            );
        }
        None
    }

    pub fn find_info_descriptions(&self, info_term: &Term) -> Option<String> {
        for DocTable { heading, rows } in self.0.iter().filter(|x| x.heading.is_some()) {
            let heading = heading.as_ref().expect("filter");

            if *info_term == **heading || info_term.deep_matches(heading) {
                return Some(
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
            }
        }
        None
    }
}

fn collect_tables(table: Table) -> Vec<[String; 2]> {
    let mut ret = vec![];
    for row in table.rows {
        let TableChild::TableRow(row) = row;

        let mut col1 = "".to_string();
        let mut col2 = "".to_string();
        for (i, cell) in row.cells.into_iter().enumerate() {
            let TableRowChild::TableCell(c) = cell;

            for cell_child in c.children {
                match cell_child {
                    TableCellContent::Paragraph(p) => match i {
                        0 => col1 = extract_paragraph_text(p),
                        1 => col2 = extract_paragraph_text(p),
                        other => {
                            debug!(
                                "Ignoring column {other}: {}. Only 2 columns are supported",
                                extract_paragraph_text(p)
                            )
                        }
                    },
                    TableCellContent::Table(_) => {
                        debug!("Doc has nested tables, not extracting the nested table");
                    }
                    other => debug!("Unhandled collect_table_paragraphs: {other:?}"),
                }
            }
        }

        if col1.is_empty() && col2.is_empty() {
            continue;
        }
        ret.push([col1, col2]);
    }
    ret
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test_log::test]
    fn test_table_extract() -> Result<()> {
        let doc = UnloadedDoc::default();
        let mut ldoc = doc.from_path("data/14. Final SBA REPORTS to Head of Departments/PHYSICAL SCIENCES/PHASE 2/DISTRICT REPORTS/PHYSICAL SCIENCES - WC -WEST COAST- 2025 DBE SBA PHASE 2 REPORT Verified 04.11.25.docx".into())?.read_docx()?;
        let tables = ldoc.extract_doc_tables()?;
        dbg!(tables.find_heading_description("PROVINCE").unwrap());
        // dbg!(tables);
        Ok(())
    }
}
