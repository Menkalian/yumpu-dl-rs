extern crate serde;
extern crate serde_json;
extern crate printpdf;
extern crate reqwest;

use std::env::temp_dir;
use std::fs::{create_dir_all, File, read_dir, remove_dir_all};
use std::io;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use printpdf::{Image, ImageTransform, Mm, PdfDocument};
use printpdf::image_crate::codecs::jpeg::JpegDecoder;
use regex::Regex;
use serde::{Deserialize};
use crate::Error::{HttpError, ImageError, InvalidUrl, IoError, PdfError};

#[derive(Debug)]
pub enum Error {
    InvalidUrl,
    IoError(Option<io::Error>),
    HttpError(reqwest::Error),
    ImageError(printpdf::image_crate::ImageError),
    PdfError(printpdf::Error),
}

pub async fn download_yumpu_to_pdf(yumpu_url: &str, target_path: &PathBuf) -> Result<(), Error> {
    let document_id = parse_document_id(yumpu_url)?;
    let doc_data = load_document_desc(document_id).await?.document;
    let img_dir = temp_dir().join(&document_id.to_string());

    // Create directories
    create_dir_all(img_dir.as_path()).map_err(|e| IoError(Some(e)))?;
    create_dir_all(target_path.as_path().parent().ok_or(IoError(None))?).map_err(|e| IoError(Some(e)))?;

    // load images
    download_yumpu_pages_as_jpg(yumpu_url, &img_dir).await?;

    // create pdf
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

        let mut image_file = File::open(img_dir.join(format!("{}.jpg", count + 1))).map_err(|e| IoError(Some(e)))?;
        let image = Image::try_from(
            JpegDecoder::new(&mut image_file)
                .map_err(|e| ImageError(e))?
        ).map_err(|e| ImageError(e))?;
        image.add_to_layer(current_layer.clone(), ImageTransform::default());
    }
    let mut out_file = File::create(&target_path).map_err(|e| IoError(Some(e)))?;
    doc.save(&mut BufWriter::new(&mut out_file)).map_err(|e| PdfError(e))?;

    remove_dir_all(img_dir).map_err(|e| IoError(Some(e)))?;
    Ok(())
}

pub async fn download_yumpu_pages_as_jpg(yumpu_url: &str, folder_path: &PathBuf) -> Result<(), Error> {
    let document_id = parse_document_id(yumpu_url)?;
    let doc_data = load_document_desc(document_id).await?.document;
    create_dir_all(folder_path.as_path()).map_err(|e| IoError(Some(e)))?;

    let client = reqwest::Client::new();
    for page in doc_data.pages.iter() {
        let mut image = File::create(folder_path.join(format!("{}.jpg", page.nr))).map_err(|e| IoError(Some(e)))?;
        let data = client
            .get(format!("{}{}?{}", &doc_data.base_path, page.images.large, page.qss.large))
            .header("User-Agent", "reqwest-rs/0.11.11")
            .send()
            .await.map_err(|e| HttpError(e))?
            .bytes()
            .await.map_err(|e| HttpError(e))?;
        image.write_all(data.as_ref()).unwrap();
    }
    Ok(())
}

pub async fn load_document_desc(yumpu_doc_id: u64) -> Result<JsonResponse, Error> {
    let url = format!("http://www.yumpu.com/en/document/json2/{}", yumpu_doc_id);
    let response = reqwest::Client::new()
        .get(&url)
        .header("User-Agent", "reqwest-rs/0.11.11")
        .send()
        .await.map_err(|e| HttpError(e))?
        .json()
        .await.map_err(|e| HttpError(e))?;
    Ok(response)
}

pub fn parse_document_id(yumpu_url: &str) -> Result<u64, Error> {
    let regex = Regex::new("http(s)?://www.yumpu.com/\\S+/\\S+/\\S+/(?P<id>\\d+)/\\S+").unwrap();
    let captures = regex.captures(yumpu_url).ok_or(InvalidUrl)?;
    Ok(captures.name("id").ok_or(InvalidUrl)?.as_str().parse().map_err(|_| InvalidUrl)?)
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
    use std::env::temp_dir;
    use crate::{download_yumpu_to_pdf, parse_document_id};

    #[test]
    fn url_parses() {
        let expected: u64 = 66625223;
        let actual = parse_document_id("https://www.yumpu.com/en/document/read/66625223/lebaron-manuals-92en").unwrap();
        assert_eq!(expected, actual);
    }

    #[tokio::test]
    async fn smoke_test_download_pdf() {
        download_yumpu_to_pdf("https://www.yumpu.com/en/document/read/66625223/lebaron-manuals-92en", &temp_dir().join("test.pdf")).await.unwrap();
    }
}
