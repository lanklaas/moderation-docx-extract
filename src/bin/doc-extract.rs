use doc_read::{
    UnloadedDoc, XmlDoc,
    info_extract::{read_body_info, read_head},
};
use std::{
    ffi::OsStr,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};
use tracing::{Level, debug, error, info, metadata::LevelFilter};
use tracing_subscriber::{Layer, fmt};

use anyhow::{Result, bail};
use clap::Parser;
use csv::WriterBuilder;

use doc_read::info_extract::ExtractedInfo;
use tracing_subscriber::{
    fmt::format::FmtSpan, prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt,
};
use walkdir::WalkDir;

#[derive(clap::Parser)]
#[clap(about = "Extracts data from word files in a directory")]
struct Args {
    #[clap(default_value = "../../data")]
    data_dir: PathBuf,
    #[clap(default_value = "/tmp/out.csv")]
    output_file: PathBuf,
    #[clap(
        short,
        help = "The data_dir path is a file with a list of paths to process"
    )]
    input_is_list_file: bool,
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
        input_is_list_file,
    } = Args::parse();

    info!("Parsing docx files...");
    let files = if !input_is_list_file {
        collect_docs(&data_dir)?
    } else {
        let paths = BufReader::new(File::open(data_dir)?);
        let mut ret = vec![];
        for path in paths.lines() {
            let path = path?;
            ret.push(PathBuf::from(path));
        }
        ret
    };
    info!("Found {} docx files", files.len());

    let mut wtr = WriterBuilder::new()
        .double_quote(true)
        .from_path(output_file)?;
    wtr.write_record(ExtractedInfo::header_record())?;
    let mut doc = UnloadedDoc::default();
    for file_path in files {
        let mut ldoc = doc.from_path(file_path.clone())?.read_docx()?;
        info!("Processing file: {file_path:?}");
        match extract_one(&mut ldoc) {
            Ok(extracted) => {
                debug!("{extracted:?} <- extracted");
                wtr.write_record(extracted.into_record(ldoc.file()))?
            }
            Err(e) => {
                error!("{e:?} in file: {file_path:?}");
            }
        }
        doc = ldoc.unload();
    }
    Ok(())
}

fn extract_one(doc: &mut XmlDoc) -> Result<ExtractedInfo> {
    let blocks = doc.extract_doc_blocks()?;
    let info = read_head(&blocks)?;
    // read_to_text_starting_with(TEXT_STARTING_WITH, &mut buf, &mut reader)?;
    debug!("Reading areas_that_require_intervention_and_support");
    let body = read_body_info(&blocks)?;

    Ok(ExtractedInfo { header: info, body })
}

fn collect_docs(dir_with_files: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut ret = vec![];
    for f in WalkDir::new(dir_with_files)
        .into_iter()
        .filter_map(|x| x.ok())
        .filter(|x| x.path().extension() == Some(OsStr::new("docx")))
    {
        ret.push(f.path().to_path_buf());
    }
    if ret.is_empty() {
        bail!(
            "No docx files found in {:?}",
            fs::canonicalize(dir_with_files)
        );
    }
    Ok(ret)
}
