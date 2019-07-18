#![feature(proc_macro_hygiene, decl_macro)]
#![feature(plugin, custom_attribute)]

extern crate multipart;
#[macro_use]
extern crate rocket;

extern crate tempfile;

use multipart::mock::StdoutTee;
use multipart::server::save::Entries;
use multipart::server::save::SaveResult::*;
use multipart::server::save::SavedData;
use multipart::server::save::SavedData::*;
use multipart::server::Multipart;

use rocket::http::{ContentType, Status};
use rocket::response::status::Custom;
use rocket::response::Stream;
use rocket::Data;

use tempfile::tempfile;

use std::io::{self, Cursor, Write};

#[get("/")]
fn index() -> &'static str {
    "Hello world! "
}
#[post("/upload", data = "<data>")]
// signature requires the request to have a `Content-Type`
fn multipart_upload(
    cont_type: &ContentType,
    data: Data,
) -> Result<Stream<Cursor<Vec<u8>>>, Custom<String>> {
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
        Ok(resp) => Ok(Stream::from(Cursor::new(resp))),
        Err(err) => Err(Custom(Status::InternalServerError, err.to_string())),
    }
}

fn process_upload(boundary: &str, data: Data) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();

    Multipart::with_body(data.open(), boundary).foreach_entry(|multipart| {
        let headers = multipart.headers;
        println!("name: {}", headers.name);
        println!("Filename: {:?}", headers.filename);

        writeln!(out, "name: {}", headers.name);
        writeln!(out, "Filename: {:?}", headers.filename);

        let mut data = multipart.data;

        match data
            .save()
            .size_limit(20 * 1024 * 1024)
            .memory_threshold(0)
            .with_dir("/tmp")
        {
            Full(entries) => {
                match entries {
                    Text(s) => {
                        writeln!(out, "Got text saved data: {}", s);
                        println!("Got text: {}", s)
                    }
                    Bytes(_) => {
                        writeln!(out, "Got bytes!");
                        println!("Got bytes")
                    }
                    File(path, size) => {
                        writeln!(out, "Wrote to file of size {} at {:?}", size, path);
                        println!("Wrote to file of size {} at {:?}", size, path)
                    }
                };
                writeln!(out, "Saved data processed.");
            }
            Partial(_, _) => println!("Request only partially processed."),
            Error(e) => {}
        };

        writeln!(out, "-----");
    });

    Ok(out)
}

fn main() {
    rocket::ignite()
        .mount("/", routes![index, multipart_upload])
        .launch();
}
