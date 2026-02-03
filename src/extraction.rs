pub mod oral;
pub mod phase2_extract;

use clap::ValueEnum;
use docx_rs::Table;
use docx_rs::{Paragraph, ParagraphChild, RunChild, TableCellContent, TableChild, TableRowChild};
use std::fmt::Debug;
use tracing::debug;

use clap::Parser;

#[derive(Debug)]
pub struct ExtractedInfo {
    /// A single line of info for the csv found in one word doc.
    pub record: Vec<String>,
}

#[derive(Debug, Parser, Default, ValueEnum, Clone)]
pub enum DocType {
    #[default]
    #[clap(alias = "p2")]
    Phase2District,
    #[clap(alias = "o")]
    Oral,
}

pub fn collect_tables(table: Table) -> Vec<[String; 2]> {
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
                        0 => col1 += &extract_paragraph_text(p),
                        1 => col2 += &extract_paragraph_text(p),
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
