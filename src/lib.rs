pub mod info_extract;

use docx_rs::{
    DocumentChild, Paragraph, ParagraphChild, RunChild, TableCellContent, TableChild,
    TableRowChild, read_docx,
};
use docx_rs::{Docx, Table};
use state::{IsXml, Loaded, NotLoaded};
use std::fs::File;
use std::io::{BufReader, Read};
use std::ops::{Deref, DerefMut};
use std::path::Path;

use anyhow::{Result, bail};
use derive_builder::Builder;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use serde::Serialize;
use tracing::{debug, info, trace, warn};

pub type UnloadedDoc = DocBytes<NotLoaded>;
pub type LoadedDoc = DocBytes<Loaded>;
pub type XmlDoc = DocBytes<IsXml>;

#[derive(Debug)]
pub struct DocBytes<S> {
    buf: Vec<u8>,
    _state: S,
}

impl Default for UnloadedDoc {
    fn default() -> Self {
        Self {
            buf: vec![],
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
    pub fn from_path(self, path: &Path) -> Result<LoadedDoc> {
        let Self { mut buf, .. } = self;
        let mut rd = BufReader::new(File::open(path)?);
        rd.read_to_end(&mut buf)?;
        Ok(LoadedDoc {
            buf,
            _state: Loaded,
        })
    }
}

impl LoadedDoc {
    pub fn read_docx(self) -> Result<XmlDoc> {
        let Self { buf, .. } = self;
        let doc = read_docx(&buf)?;
        Ok(DocBytes {
            buf,
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
    pub fn unload(self) -> UnloadedDoc {
        let Self { mut buf, .. } = self;
        buf.clear();
        DocBytes {
            buf,
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
    pub fn find_term_table_text(&self, term: &str) -> Option<&Block> {
        let Some(position) = self.0.iter().position(|x| {
            if !x.is_paragraph() {
                return false;
            }
            let Block::Paragraph(p) = x else {
                unreachable!()
            };
            p.trim() == term

            // match x {
            //     Block::Paragraph(p) => p.trim() == term,
            //     Block::Table(t) => t.iter().any(|x| x.trim() == term),
            // }
        }) else {
            warn!("Running case insensitive search for: {term}");
            return self.find_term_table_text_case_insensitive(term);
        };
        self.0.get(position + 1)
    }

    pub fn find_term_table_text_case_insensitive(&self, term: &str) -> Option<&Block> {
        let Some(position) = self.0.iter().position(|x| {
            if !x.is_paragraph() {
                return false;
            }
            let Block::Paragraph(p) = x else {
                unreachable!()
            };
            p.trim().to_lowercase() == term.to_lowercase()
            // match x {
            //     Block::Paragraph(p) => p.to_lowercase() == term.to_lowercase(),
            //     Block::Table(t) => t
            //         .iter()
            //         .map(|x| x.to_lowercase())
            //         .any(|x| x == term.to_lowercase()),
            // }
        }) else {
            unreachable!("Should have the term: {term}");
        };
        self.0.get(position + 1)
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

pub fn get_body<'a>(buf: &'a mut Vec<u8>, reader: &mut Reader<&[u8]>) -> Result<BytesStart<'a>> {
    loop {
        let event = reader.read_event_into(buf)?;
        match event {
            Event::Start(element) => match element.name().as_ref() {
                b"w:body" => {
                    return Ok(element.to_owned());
                }
                el => {
                    trace!("get_body not body: {el:?}");
                }
            },
            Event::Decl(d) => {
                debug!("get body Decl: {d:?}");
            }
            // Event::Eof
            other => {
                todo!("{other:?}")
            }
        }
    }
}

pub fn get_first_table<'a>(reader: &mut Reader<&[u8]>) -> Result<BytesStart<'a>> {
    let mut buf = vec![];
    loop {
        let event = reader.read_event_into(&mut buf)?;
        match event {
            Event::Start(element) if element.name().as_ref() == b"w:tbl" => {
                return Ok(element.to_owned());
            }
            Event::Eof => {
                unreachable!("Should have got a table before the end")
            }
            other => {
                debug!("get_first_table: {other:?}")
            }
        }
    }
}

pub fn get_table_row<'a>(reader: &mut Reader<&[u8]>) -> Result<BytesStart<'a>> {
    let mut buf = vec![];
    loop {
        let event = reader.read_event_into(&mut buf)?;
        match event {
            Event::Start(element) if element.name().as_ref() == b"w:tr" => {
                return Ok(element.to_owned());
            }
            Event::Eof => {
                unreachable!("Should have got a table before the end")
            }
            other => {
                debug!("get_table_row: {other:?}")
            }
        }
    }
}

pub fn get_element<'a>(
    name: &[u8],
    buf: &'a mut Vec<u8>,
    reader: &mut Reader<&[u8]>,
) -> Result<BytesStart<'a>> {
    trace!("Looking for element: {:?}", std::str::from_utf8(name));
    loop {
        let event = reader.read_event_into(buf)?;
        match event {
            Event::Start(element) if element.name().as_ref() == name => {
                return Ok(element.to_owned());
            }
            Event::Eof => {
                bail!(
                    "get_element failed looking for {:?} Should have got a table before the end",
                    std::str::from_utf8(name)
                )
            }
            other => {
                trace!("get_element: {other:?}")
            }
        }
    }
}

pub fn read_to_info_table(buf: &mut Vec<u8>, reader: &mut Reader<&[u8]>) -> Result<()> {
    get_body(buf, reader).unwrap();
    get_element(b"w:tbl", buf, reader)?;

    Ok(())
}

/// Read the text of the first cell in the row
pub fn read_row_first_cell_text(buf: &mut Vec<u8>, reader: &mut Reader<&[u8]>) -> Result<String> {
    get_element(b"w:tr", buf, reader)?;
    read_cell_text(buf, reader)
    // get_element(b"w:tc", buf, reader)?;
    // get_element(b"w:p", buf, reader)?;
    // get_element(b"w:r", buf, reader)?;
    // get_element(b"w:t", buf, reader).unwrap();

    // let mut tbuf = vec![];
    // let evt = reader.read_event_into(&mut tbuf).unwrap();
    // let Event::Text(t) = evt else { unreachable!() };

    // Ok(String::from_utf8(t.to_vec())?)
}

pub fn read_run_text(buf: &mut Vec<u8>, reader: &mut Reader<&[u8]>) -> Result<String> {
    let mut ret = String::new();
    // for _ in 0..2 {
    get_element(b"w:r", buf, reader)?;
    get_element(b"w:t", buf, reader)?;

    let mut tbuf = vec![];
    let evt = reader.read_event_into(&mut tbuf).unwrap();
    match evt {
        Event::Text(t) => {
            let t = std::str::from_utf8(&t)?;
            ret.push_str(t);
        }
        Event::End(_) => {
            info!("End of w:t, must be empty");
        }
        other => unreachable!("{other:?}"),
    }
    // }
    Ok(ret)
}

pub fn read_cell_text(buf: &mut Vec<u8>, reader: &mut Reader<&[u8]>) -> Result<String> {
    get_element(b"w:tc", buf, reader)?;
    get_element(b"w:p", buf, reader)?;
    read_run_text(buf, reader)
}

#[derive(Builder, Debug, Serialize)]
#[builder_struct_attr(derive(Debug))]
pub struct HeaderInfo {
    pub province: String,
    pub district: String,
    pub school: Option<String>,
    pub subject: Option<String>,
}

pub fn read_header_info(buf: &mut Vec<u8>, reader: &mut Reader<&[u8]>) -> Result<HeaderInfo> {
    let mut results = HeaderInfoBuilder::create_empty();
    let mut protection_counter = 0;
    loop {
        protection_counter += 1;
        let t = read_row_first_cell_text(buf, reader).unwrap();
        // 2 Header columns in a row
        let mut colval = String::new();
        for i in 0..2 {
            if i == 1 {
                colval = read_cell_text(buf, reader)?;
            } else {
                colval = t.clone();
            }
            debug!("{colval}, Loop: {i}, protcount: {protection_counter} <- HeaderInfo");
            match colval
                .split_whitespace()
                .collect::<String>()
                .to_lowercase()
                .as_str()
            {
                "province" | "province:" => {
                    let prov = read_cell_text(buf, reader)?;
                    debug!("{prov} <- Province");
                    results.province(prov);
                }
                "district" | "district/region" | "district:" => {
                    let dis = read_cell_text(buf, reader)?;
                    results.district(dis);
                }
                "school" | "school:" => {
                    let sc = read_cell_text(buf, reader)?;
                    results.school(Some(sc));
                }
                "subject" | "subject:" => {
                    let sub = read_cell_text(buf, reader)?;
                    results.subject(Some(sub));
                }
                other => debug!("{other} text found"),
            }
        }
        if protection_counter > 14 && !(results.province.is_some() && results.district.is_some()) {
            bail!("HeaderInfo loop ran too long. Builder status: {results:?}");
        }
        if results.province.is_some()
            && results.district.is_some()
            && results.subject.is_none()
            && protection_counter > 13
        {
            results.subject(None);
        }
        match results.build() {
            Ok(res) => return Ok(res),
            Err(e) => {
                debug!("{e}. Build not complete.");
            }
        }
    }
}

#[derive(Debug, Default)]
pub enum Case {
    #[default]
    Sensitive,
    Ignore,
}

pub fn read_to_text_starting_with(
    starts_with: &[u8],
    buf: &mut Vec<u8>,
    reader: &mut Reader<&[u8]>,
    case: Case,
) -> Result<()> {
    loop {
        get_element(b"w:t", buf, reader)?;
        let mut tbuf = vec![];
        let evt = reader.read_event_into(&mut tbuf)?;
        let t = match evt {
            Event::Text(t) => {
                debug!(
                    "Text {t:?} found, looking for {:?}",
                    std::str::from_utf8(starts_with)
                );
                t
            }
            Event::End(e) => {
                debug!(
                    "{e:?}, End found before text: {:?}",
                    std::str::from_utf8(starts_with)
                );
                continue;
            }
            other => unreachable!("{other:?}"),
        };
        match case {
            Case::Sensitive => {
                if t.starts_with(starts_with) {
                    info!(
                        "Found {t:?} starts with {:?}",
                        std::str::from_utf8(starts_with)
                    );
                    break;
                }
            }
            Case::Ignore => {
                if t.to_ascii_lowercase()
                    .starts_with(starts_with.to_ascii_lowercase().as_slice())
                {
                    info!(
                        "Found {t:?} starts with {:?}",
                        std::str::from_utf8(starts_with)
                    );
                    break;
                }
            }
        }
    }
    Ok(())
}

pub fn read_run_text_until(
    stop_str: &str,
    buf: &mut Vec<u8>,
    reader: &mut Reader<&[u8]>,
) -> Result<String> {
    let mut res = vec![];
    loop {
        let s = read_run_text(buf, reader)?;
        if s.contains(stop_str) {
            break;
        }
        res.push(s);
    }
    Ok(res.join(""))
}

pub fn read_all_table_cells(buf: &mut Vec<u8>, reader: &mut Reader<&[u8]>) -> Result<String> {
    get_element(b"w:tbl", buf, reader)?;
    let mut ret = vec![];
    loop {
        let event = reader.read_event_into(buf)?;
        match event {
            Event::Text(t) => {
                ret.push(String::from_utf8(t.to_vec())?);
            }
            Event::End(e) if e.name().as_ref() == b"w:tbl" => return Ok(ret.join("")),
            Event::Eof => {
                bail!(
                    "get_element failed looking for table text data. Should have got a table before the end",
                    // std::str::from_utf8(name)
                )
            }
            other => {
                trace!("get_element: {other:?}")
            }
        }
    }
}
