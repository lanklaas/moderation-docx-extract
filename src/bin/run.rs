use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

use anyhow::bail;
use anyhow::Result;
use csv::WriterBuilder;
use doc_read::read_header_info;
use doc_read::read_part_four;
use doc_read::read_part_four_no_search;
use doc_read::read_to_info_table;
use doc_read::read_to_text_starting_with;
use doc_read::ExtractedInfo;
use doc_read::Part4;
use quick_xml::Reader;
use tracing::debug;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use walkdir::WalkDir;
use zip::ZipArchive;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tiberius=error,odbc_api=error".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_span_events(FmtSpan::CLOSE))
        .init();
    let files = collect_doc_xmls(Path::new("../../data/"))?;
    let mut wtr = WriterBuilder::new()
        .has_headers(true)
        .double_quote(true)
        .from_path("/tmp/out.csv")?;
    let mut header_written = false;
    for (file, file_path) in files {
        let extracted = extract_one(&file, file_path)?;
        if !header_written {
            wtr.write_record(extracted.header_record())?;
            header_written = true;
        }
        wtr.write_record(extracted.as_record())?;
    }
    Ok(())
}

fn extract_one(doc: &[u8], file: PathBuf) -> Result<ExtractedInfo> {
    let mut reader = Reader::from_reader(doc);

    let config = reader.config_mut();

    config.trim_text(true);
    let mut buf = vec![];
    read_to_info_table(&mut buf, &mut reader)?;
    let info = read_header_info(&mut buf, &mut reader)?;

    read_to_text_starting_with(b"PART 4:", &mut buf, &mut reader)?;

    let ares = read_part_four(
        b"AREAS OF IMPROVEMENT",
        "AREAS OF NON-COMPLIANCE",
        &mut buf,
        &mut reader,
    )?;

    let bres = read_part_four_no_search(
        // b"AREAS OF NON-COMPLIANCE",
        "DIRECTIVES FOR COMPLIANCE",
        &mut buf,
        &mut reader,
    )?;

    let cres = read_part_four_no_search(
        // b"DIRECTIVES FOR COMPLIANCE",
        "RECOMMENDATIONS FOR IMPROVEMENT",
        &mut buf,
        &mut reader,
    )?;
    let dres = read_part_four_no_search(
        // b"RECOMMENDATIONS FOR IMPROVEMENT",
        "CONCLUSION",
        &mut buf,
        &mut reader,
    )?;

    let p4 = Part4 {
        areas_of_improvement: ares,
        areas_of_non_compliance: bres,
        directives_for_compliance: cres,
        recommendations_for_improvement: dres,
    };
    Ok(ExtractedInfo {
        header: info,
        part4: p4,
        file,
    })
}

fn collect_doc_xmls(dir_with_files: &Path) -> anyhow::Result<Vec<(Vec<u8>, PathBuf)>> {
    let mut ret = vec![];
    for f in WalkDir::new(dir_with_files)
        .into_iter()
        .filter_map(|x| x.ok())
        .filter(|x| x.path().extension() == Some(OsStr::new("docx")))
    {
        let mut zip = ZipArchive::new(File::open(f.path())?)?;
        debug!(
            "Zip files in {f:?}: {:?}",
            zip.file_names().collect::<Vec<_>>()
        );
        let mut file = zip.by_name("word/document.xml")?;

        let mut buf = Vec::with_capacity(file.size().try_into().unwrap());
        file.read_to_end(&mut buf)?;
        ret.push((buf, f.path().to_path_buf()));
    }
    if ret.is_empty() {
        bail!(
            "No docx files found in {:?}",
            fs::canonicalize(dir_with_files)
        );
    }
    Ok(ret)
}
