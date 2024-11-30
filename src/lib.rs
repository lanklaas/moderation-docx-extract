use std::path::PathBuf;

use anyhow::{bail, Result};
use derive_builder::Builder;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde::Serialize;
use tracing::{debug, info, trace};

use docx_rust::document::{
    Paragraph, ParagraphContent, RunContent, TableCell, TableCellContent, TableRowContent,
};
use docx_rust::{document::BodyContent, Docx};

pub fn extract_first_table_first_row(docx: &Docx) {
    // let mut table_counter = 0;
    // Iterate through all blocks in the document
    for block in &docx.document.body.content {
        match block {
            BodyContent::Table(table) => {
                // table_counter += 1;
                // Found the first table
                for row in &table.rows {
                    // Extract text from each cell in the first row
                    let row_text: Vec<String> = row
                        .cells
                        .iter()
                        .filter(|x| matches!(x, TableRowContent::TableCell(_)))
                        .map(|x| {
                            let TableRowContent::TableCell(c) = x else {
                                unreachable!("filter")
                            };
                            c
                        })
                        .map(|cell| {
                            // cell.content.iter().find(|x| matches!(x, TableCellContent::))
                            // let TableRowContent::TableCell(c) = cell else {
                            //     unreachable!("filter")
                            // };

                            extract_text_from_cell(cell)
                        })
                        .collect();

                    println!("Row Data: {:?}", row_text);
                }
                // if table_counter == 2 {
                //     break; // Since we only need the first table
                // }
            }
            BodyContent::Sdt(s) => {
                panic!("{s:?}")
            }
            BodyContent::Paragraph(p) => {
                trace!("body para: {p:?}")
            }
            other => todo!("{other:?}"),
        }
    }
}

// Helper function to extract text from a table cell
pub fn extract_text_from_cell(cell: &TableCell) -> String {
    cell.content
        .iter()
        .map(|block| {
            let TableCellContent::Paragraph(paragraph) = block;
            paragraph.text()
        })
        .collect::<Vec<String>>()
        .join(",")
}
// Helper function to convert a paragraph to text
pub fn paragraph_to_text(paragraph: &Paragraph) -> String {
    paragraph
        .content
        .iter()
        // .filter(|x| matches!(x, ParagraphContent::Run(_)))
        .map(|run| {
            let ParagraphContent::Run(r) = run else {
                unreachable!("filter")
            };
            r.content
                .iter()
                // .get(0)
                .find(|x| matches!(x, RunContent::Text(_)))
                .map(|x| {
                    let RunContent::Text(t) = x else {
                        unreachable!("filter")
                    };
                    t.text.to_string()
                })
                .unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join("")
}

pub fn get_body<'a>(buf: &'a mut Vec<u8>, reader: &mut Reader<&[u8]>) -> Result<BytesStart<'a>> {
    loop {
        let event = reader.read_event_into(buf)?;
        match event {
            Event::Start(element) => {
                dbg!(std::str::from_utf8(element.name().as_ref())?);
                match element.name().as_ref() {
                    b"w:body" => {
                        return Ok(element.to_owned());
                    }
                    el => {
                        trace!("get_body not body: {el:?}");
                    }
                }
            }
            Event::Decl(d) => {
                debug!("get body Decl: {d:?}");
            }
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
                return Ok(element.to_owned())
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
                return Ok(element.to_owned())
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
    loop {
        let event = reader.read_event_into(buf)?;
        match event {
            Event::Start(element) if element.name().as_ref() == name => {
                return Ok(element.to_owned())
            }
            Event::Eof => {
                unreachable!("Should have got a table before the end")
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
    get_element(b"w:r", buf, reader)?;
    get_element(b"w:t", buf, reader).unwrap();

    let mut tbuf = vec![];
    let evt = reader.read_event_into(&mut tbuf).unwrap();
    let Event::Text(t) = evt else {
        unreachable!("{evt:?}")
    };

    Ok(String::from_utf8(t.to_vec())?)
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
    pub school: String,
    pub subject: String,
}

pub fn read_header_info(buf: &mut Vec<u8>, reader: &mut Reader<&[u8]>) -> Result<HeaderInfo> {
    let mut results = HeaderInfoBuilder::create_empty();
    let mut protection_counter = 0;
    loop {
        protection_counter += 1;
        let t = read_row_first_cell_text(buf, reader).unwrap();
        match t.as_str() {
            "Province" => {
                let prov = read_cell_text(buf, reader)?;
                results.province(prov);
            }
            "District" => {
                let dis = read_cell_text(buf, reader)?;
                results.district(dis);
            }
            "School" => {
                let sc = read_cell_text(buf, reader)?;
                results.school(sc);
            }
            "Subject" => {
                let sub = read_cell_text(buf, reader)?;
                results.subject(sub);
            }
            other => debug!("{other} text found"),
        }
        if protection_counter > 5 {
            bail!("HeaderInfo loop ran too long. Builder status: {results:?}");
        }
        match results.build() {
            Ok(res) => return Ok(res),
            Err(e) => {
                debug!("{e}. Build not complete.");
            }
        }
    }
}

pub fn read_to_text_starting_with(
    starts_with: &[u8],
    buf: &mut Vec<u8>,
    reader: &mut Reader<&[u8]>,
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
        if t.starts_with(starts_with) {
            info!(
                "Found {t:?} starts with {:?}",
                std::str::from_utf8(starts_with)
            );
            break;
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
        if s == stop_str {
            break;
        }
        res.push(s);
    }
    Ok(res.join(""))
}

pub fn read_part_four(
    start_section_text: &[u8],
    next_section_text: &str,
    buf: &mut Vec<u8>,
    reader: &mut Reader<&[u8]>,
) -> Result<String> {
    read_to_text_starting_with(start_section_text, buf, reader)?;
    get_element(b"w:tbl", buf, reader)?;
    get_element(b"w:tr", buf, reader)?;
    read_run_text_until(next_section_text, buf, reader)
}

/// If the previous call was read_part_four then the reader position is alredy in the correct place
pub fn read_part_four_no_search(
    next_section_text: &str,
    buf: &mut Vec<u8>,
    reader: &mut Reader<&[u8]>,
) -> Result<String> {
    get_element(b"w:tbl", buf, reader)?;
    get_element(b"w:tr", buf, reader)?;
    read_run_text_until(next_section_text, buf, reader)
}

#[derive(Debug, Serialize)]
pub struct ExtractedInfo {
    pub header: HeaderInfo,
    pub part4: Part4,
    pub file: PathBuf,
}

impl ExtractedInfo {
    pub fn as_record(&self) -> [&str; 9] {
        let Self {
            header:
                HeaderInfo {
                    province,
                    district,
                    school,
                    subject,
                },
            part4:
                Part4 {
                    areas_of_improvement,
                    areas_of_non_compliance,
                    directives_for_compliance,
                    recommendations_for_improvement,
                },
            file,
        } = self;
        [
            province,
            district,
            school,
            subject,
            areas_of_improvement,
            areas_of_non_compliance,
            directives_for_compliance,
            recommendations_for_improvement,
            file.to_str().unwrap_or_default(),
        ]
    }

    pub fn header_record() -> [&'static str; 9] {
        [
            "Province",
            "District",
            "School",
            "Subject",
            "Areas Of Improvement",
            "Areas Of Non Compliance",
            "Directives For Compliance",
            "Recommendations For Improvement",
            "File",
        ]
    }
}

#[derive(Debug, Serialize)]
pub struct Part4 {
    pub areas_of_improvement: String,
    pub areas_of_non_compliance: String,
    pub directives_for_compliance: String,
    pub recommendations_for_improvement: String,
}
