pub mod extraction;

use docx_rs::Docx;
use docx_rs::{DocumentChild, read_docx};
use extraction::phase2_extract::{DocTable, DocTables};
use state::{IsXml, Loaded, NotLoaded};
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
        let mut ret = DocTables::default();

        // Take the children so we can own them (and not reverse them)
        let children = std::mem::take(&mut self.document.children);

        let mut heading = None;
        for (i, child) in children.clone().into_iter().enumerate() {
            let next = children.get(i + 1);
            let nextnext = children.get(i + 2);
            match child {
                DocumentChild::Paragraph(p) => {
                    let text = extraction::extract_paragraph_text(*p);
                    ret.paragraphs.push(text.clone());
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
                    let rows = extraction::collect_tables(*t);
                    let heading = heading.take();
                    ret.tables.push(DocTable { heading, rows })
                }
                other => {
                    debug!("Unhandled document child: {other:?}");
                }
            }
        }

        Ok(ret)
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
