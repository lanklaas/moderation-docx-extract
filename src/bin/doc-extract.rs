use doc_read::read_sectiong_info;
use doc_read::TEXT_STARTING_WITH;
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use tracing::metadata::LevelFilter;
use tracing::Level;
use tracing_subscriber::fmt;
use tracing_subscriber::Layer;

use anyhow::bail;
use anyhow::Result;
use clap::Parser;
use csv::WriterBuilder;
use doc_read::read_header_info;

use doc_read::read_to_info_table;
use doc_read::read_to_text_starting_with;
use doc_read::ExtractedInfo;
use quick_xml::Reader;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use walkdir::WalkDir;
use zip::ZipArchive;

#[derive(clap::Parser)]
#[clap(about = "Extracts data from word files in a directory")]
struct Args {
    #[clap(default_value = "../../data")]
    data_dir: PathBuf,
    #[clap(default_value = "/tmp/out.csv")]
    output_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let error_log = File::create("/tmp/errors")?;
    let (non_blocking, _guard) = tracing_appender::non_blocking(error_log);
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tiberius=error,odbc_api=error".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_span_events(FmtSpan::CLOSE))
        .with(
            fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_filter(LevelFilter::from_level(Level::ERROR)),
        )
        .init();
    let Args {
        data_dir,
        output_file,
    } = Args::parse();

    info!("Parsing docx files...");
    let files = collect_doc_xmls(&data_dir)?;
    info!("Found {} docx files", files.len());
    let mut wtr = WriterBuilder::new()
        .has_headers(true)
        .double_quote(true)
        .from_path(output_file)?;
    wtr.write_record(ExtractedInfo::header_record())?;
    for (file, file_path) in files {
        info!("Processing file: {file_path:?}");
        match extract_one(&file, file_path.clone()) {
            Ok(extracted) => wtr.write_record(extracted.as_record())?,
            Err(e) => {
                error!("{e:?} in file: {file_path:?}");
            }
        }
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

    // read_to_text_starting_with(TEXT_STARTING_WITH, &mut buf, &mut reader)?;
    debug!("Reading areas_that_require_intervention_and_support");
    let secg = read_sectiong_info(&mut buf, &mut reader)?;

    Ok(ExtractedInfo {
        header: info,
        sectiong: secg,
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
