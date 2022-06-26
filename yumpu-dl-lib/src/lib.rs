extern crate serde;
extern crate printpdf;
extern crate reqwest;

use std::env::temp_dir;
use std::fs::{create_dir_all, File, read_dir, remove_dir_all};
use std::io;
use std::io::{BufWriter, Write};
use std::ops::Not;
use std::path::{Path, PathBuf};
use printpdf::{Image, ImageTransform, Mm, PdfDocument};
use printpdf::image_crate::codecs::jpeg::JpegDecoder;
use regex::Regex;
use serde::{Deserialize};
use crate::Error::{HttpError, ImageError, InvalidUrl, IoError, PdfError};

pub trait Logger {
    fn is_initialized(&self) -> bool;
    fn set_total_operations(&self, amount: u64);
    fn increment_progression(&self);

    fn log_message(&self, msg: &str);
}

#[derive(Debug)]
struct NoOpLogger {}

impl Logger for NoOpLogger {
    fn is_initialized(&self) -> bool { true }
    fn set_total_operations(&self, _: u64) {}
    fn increment_progression(&self) {}
    fn log_message(&self, _: &str) {}
}

#[derive(Debug)]
pub enum Error {
    InvalidUrl,
    IoError(Option<io::Error>),
    HttpError(reqwest::Error),
    ImageError(printpdf::image_crate::ImageError),
    PdfError(printpdf::Error),
}

pub async fn download_yumpu_to_pdf(yumpu_url: &str, target_path: &PathBuf, logger: Option<&dyn Logger>) -> Result<(), Error> {
    let noop_logger = NoOpLogger {};
    let log = logger.unwrap_or(&noop_logger);

    let document_id = parse_document_id(yumpu_url)?;
    log.log_message(&format!("Loading data for document {}", document_id));
    let doc_data = load_document_desc(document_id).await?.document;
    let img_dir = temp_dir().join(&document_id.to_string());

    if log.is_initialized().not() {
        log.log_message(&format!("Downloading document \"{}\" with ID {}", doc_data.title, doc_data.id));
        log.set_total_operations((2 * doc_data.pages.len() + 1) as u64);
    }

    // Create directories
    create_dir_all(img_dir.as_path()).map_err(|e| IoError(Some(e)))?;
    create_dir_all(target_path.as_path().parent().ok_or(IoError(None))?).map_err(|e| IoError(Some(e)))?;

    // load images
    download_yumpu_pages_as_jpg(yumpu_url, &img_dir, Some(log)).await?;

    // create pdf
    log.log_message("Creating pdf...");
    let page_size = [doc_data.width as f64, doc_data.height as f64];
    let (doc, page1, layer1) = PdfDocument::new(
        doc_data.title,
        Mm(page_size[0]),
        Mm(page_size[1]),
        "PageLayer");
    let mut current_layer = doc.get_page(page1).get_layer(layer1);

    let image_paths = read_dir(&img_dir).map_err(|e| IoError(Some(e)))?;
    for count in 0..image_paths.count() {
        if count != 0 {
            let (new_page_idx, new_layer_id) = doc.add_page(Mm(page_size[0]), Mm(page_size[1]), "PageLayer");
            current_layer = doc.get_page(new_page_idx).get_layer(new_layer_id);
        }

        log.log_message(&format!("Adding page {} to pdf", count + 1));
        let mut image_file = File::open(img_dir.join(format!("{}.jpg", count + 1))).map_err(|e| IoError(Some(e)))?;
        let image = Image::try_from(
            JpegDecoder::new(&mut image_file)
                .map_err(ImageError)?
        ).map_err(ImageError)?;
        image.add_to_layer(current_layer.clone(), ImageTransform::default());
        log.increment_progression();
    }
    log.log_message("Saving PDF-Document...");
    let mut out_file = File::create(&target_path).map_err(|e| IoError(Some(e)))?;
    doc.save(&mut BufWriter::new(&mut out_file)).map_err(PdfError)?;
    log.increment_progression();

    remove_dir_all(img_dir).map_err(|e| IoError(Some(e)))?;
    Ok(())
}

pub async fn download_yumpu_pages_as_jpg(yumpu_url: &str, folder_path: &Path, logger: Option<&dyn Logger>) -> Result<(), Error> {
    let noop_logger = NoOpLogger {};
    let log = logger.unwrap_or(&noop_logger);

    let document_id = parse_document_id(yumpu_url)?;
    let doc_data = load_document_desc(document_id).await?.document;
    create_dir_all(folder_path).map_err(|e| IoError(Some(e)))?;

    if log.is_initialized().not() {
        log.log_message(&format!("Downloading images for document \"{}\" with ID {}", doc_data.title, doc_data.id));
        log.set_total_operations(doc_data.pages.len() as u64);
    }

    let client = reqwest::Client::new();
    for page in doc_data.pages.iter() {
        log.log_message(&format!("Downloading page {}", page.nr));
        let mut image = File::create(folder_path.join(format!("{}.jpg", page.nr))).map_err(|e| IoError(Some(e)))?;
        let data = client
            .get(format!("{}{}?{}", &doc_data.base_path, page.images.large, page.qss.large))
            .header("User-Agent", "reqwest-rs/0.11.11")
            .send()
            .await.map_err(HttpError)?
            .bytes()
            .await.map_err(HttpError)?;
        image.write_all(data.as_ref()).unwrap();
        log.increment_progression();
    }
    Ok(())
}

pub async fn load_document_desc(yumpu_doc_id: u64) -> Result<JsonResponse, Error> {
    let url = format!("https://www.yumpu.com/en/document/json2/{}", yumpu_doc_id);
    let response = reqwest::Client::new()
        .get(&url)
        .header("User-Agent", "reqwest-rs/0.11.11")
        .send()
        .await.map_err(HttpError)?
        .json()
        .await.map_err(HttpError)?;
    Ok(response)
}

pub fn parse_document_id(yumpu_url: &str) -> Result<u64, Error> {
    let regex = Regex::new("http(s)?://www.yumpu.com/\\S+/\\S+/\\S+/(?P<id>\\d+)/\\S+").unwrap();
    let captures = regex.captures(yumpu_url).ok_or(InvalidUrl)?;
    captures.name("id").ok_or(InvalidUrl)?.as_str().parse().map_err(|_| InvalidUrl)
}

#[derive(Debug, Deserialize)]
pub struct JsonResponse {
    document: DocumentData,
}

#[derive(Debug, Deserialize)]
pub struct DocumentData {
    pub id: u64,
    pub title: String,
    pub url_title: String,
    pub width: u64,
    pub height: u64,
    pub url: String,
    pub base_path: String,
    pub pages: Vec<PageData>,
}

#[derive(Debug, Deserialize)]
pub struct PageData {
    pub nr: u64,
    pub images: PageSubData,
    pub qss: PageSubData,
}

#[derive(Debug, Deserialize)]
pub struct PageSubData {
    pub thumb: String,
    pub small: String,
    pub medium: String,
    pub large: String,
}

#[cfg(test)]
mod tests {
    use crate::parse_document_id;

    #[test]
    fn url_parses() {
        let expected: u64 = 66625223;
        let actual = parse_document_id("https://www.yumpu.com/en/document/read/66625223/lebaron-manuals-92en").unwrap();
        assert_eq!(expected, actual);
    }
}
