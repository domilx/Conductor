use std::thread;
use std::path::PathBuf;
use crate::resources;
use std::sync::mpsc;
use actix_web::{HttpServer, web, HttpRequest, Responder, HttpResponse, App};
use actix_web::body::Body;
use resources::Resources;
use std::borrow::Cow;
use actix_web::dev::Server;

fn assets(path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();
    match Resources::get(&path) {
        Some(content) => {
            let body: Body = match content {
                Cow::Borrowed(bytes) => bytes.into(),
                Cow::Owned(bytes) => bytes.into(),
            };
            HttpResponse::Ok().content_type(mime_guess::from_path(path).first_or_octet_stream().as_ref()).body(body)
        }
        None => HttpResponse::NotFound().body("404 Not Found")
    }
}

fn index(_req: HttpRequest) -> impl Responder {
    let contents = Resources::get("index.html").unwrap();
    let body: Body = match contents {
        Cow::Borrowed(bytes) => bytes.into(),
        Cow::Owned(bytes) => bytes.into(),
    };

    HttpResponse::Ok().content_type("text/html")
        .body(body)
}

fn stdout(_req: HttpRequest) -> impl Responder {
    let contents = Resources::get("stdout.html").unwrap();
    let body: Body = match contents {
        Cow::Borrowed(bytes) => bytes.into(),
        Cow::Owned(bytes) => bytes.into(),
    };

    HttpResponse::Ok().content_type("text/html")
        .body(body)
}

pub fn launch_webserver() -> u16 {
    let (port_tx, port_rx) = mpsc::channel();

    thread::spawn(move || {
        let sys = actix_rt::System::new("http-server");
        let server = HttpServer::new(|| {
            App::new().route("/", web::get().to(index))
                .route("/stdout", web::get().to(stdout))
                .route("/{path}", web::get().to(assets))
        })
            .bind("127.0.0.1:0")
            .unwrap();

        let port = server.addrs().first().unwrap().port();
        port_tx.send(port).unwrap();

        server.run().unwrap()
    });

    port_rx.recv().unwrap()
}
