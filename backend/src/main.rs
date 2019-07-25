#![feature(proc_macro_hygiene, decl_macro)]
#![feature(plugin, custom_attribute)]

extern crate multipart;
#[macro_use]
extern crate rocket;

extern crate tempfile;

extern crate tizol;
use std::time::Instant;
use tizol::Spectrogram;

use multipart::mock::StdoutTee;
use multipart::server::save::Entries;
use multipart::server::save::EntriesSaveResult;
use multipart::server::save::SaveResult;
use multipart::server::save::SaveResult::*;
use multipart::server::save::SavedData;
use multipart::server::save::SavedData::*;
use multipart::server::Multipart;

use rocket::http::{ContentType, Status};
use rocket::response::status::Custom;
use rocket::response::NamedFile;
use rocket::response::Stream;
use rocket::Data;

use tempfile::tempfile;

use std::io::{self, Cursor, Write};
use std::io::{Error, ErrorKind};
use std::path::PathBuf;

#[get("/")]
fn index() -> &'static str {
    "Hello world! "
}
#[post("/upload", data = "<data>")]
// signature requires the request to have a `Content-Type`
fn multipart_upload(cont_type: &ContentType, data: Data) -> Result<NamedFile, Custom<String>> {
    // this and the next check can be implemented as a request guard but it seems like just
    // more boilerplate than necessary
    if !cont_type.is_form_data() {
        return Err(Custom(
            Status::BadRequest,
            "Content-Type not multipart/form-data".into(),
        ));
    }

    let (_, boundary) = cont_type
        .params()
        .find(|&(k, _)| k == "boundary")
        .ok_or_else(|| {
            Custom(
                Status::BadRequest,
                "`Content-Type: multipart/form-data` boundary param not provided".into(),
            )
        })?;

    match process_upload(boundary, data) {
        Ok(resp) => Ok(resp),
        Err(err) => Err(Custom(Status::InternalServerError, err.to_string())),
    }
}

fn process_upload(boundary: &str, data: Data) -> io::Result<NamedFile> {
    // Iterate over the parts of the request, creating save results for each.

    let mut save_results: Vec<(String, PathBuf)> = Vec::new();

    Multipart::with_body(data.open(), boundary).foreach_entry(|multipart| {
        let headers = multipart.headers;
        println!("name: {}", headers.name);
        println!("Filename: {:?}", headers.filename);

        let mut data = multipart.data;

        match data
            .save()
            .size_limit(20 * 1024 * 1024)
            .memory_threshold(0)
            .with_dir("/tmp")
        {
            Full(entries) => match entries {
                Text(s) => println!("Got text: {}", s),

                Bytes(_) => println!("Got bytes"),

                File(path, size) => {
                    println!("Wrote to file of size {} at {:?}", size, path);
                    match headers.filename {
                        Some(fname) => save_results.push((fname, path)),
                        _ => {}
                    }
                }
            },
            Partial(_, r) => println!("Request only partially processed, reason: {:?}", r),

            Error(e) => println!("Found error: {:?}", e),
        };
    });

    let mut files: Vec<NamedFile> = save_results
        .iter()
        .map(|(filename, savedfile)| -> io::Result<NamedFile> {
            println!("Filename: {}", filename);
            // change the extension of "filename" to "/tmp/filename.png"
            let filename = format!(
                "/tmp/{}.png",
                match filename.as_ref() {
                    "None" => Err(Error::new(
                        ErrorKind::Other,
                        "Cannot convert a filename of 'None'"
                    )),
                    st => Ok(st),
                }?
            );

            // Invoke tizol to convert it to a pretty picture
            let start = Instant::now();

            let log_time = |message: &'static str| -> () {
                println!("==== Time: {: >15?} ==== {}", start.elapsed(), message);
            };

            log_time("Starting spectrogram process");

            // Read an audio file into a spectrogram
            let sp = Spectrogram::from_file(savedfile).unwrap();
            log_time("Computed spectrogram");

            // Save it as an image
            let img = sp.as_image();
            log_time("Image generated!");

            img.save(&filename).unwrap();
            log_time("Image saved.");

            NamedFile::open(filename)
        })
        .filter_map(Result::ok)
        .collect();

    println!("Files total: {}", files.len());

    match files.get(0) {
        Some(_) => Ok(files.remove(0)),
        None => Err(Error::new(ErrorKind::Other, "No files were converted.")),
    }
}

fn main() {
    rocket::ignite()
        .mount("/", routes![index, multipart_upload])
        .launch();
}
