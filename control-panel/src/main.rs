use actix_web::body::BoxBody;
use actix_web::dev::ServiceResponse;
use actix_web::http::header::ContentType;
use actix_web::http::StatusCode;
use actix_web::middleware::{ErrorHandlerResponse, ErrorHandlers};
use actix_web::{get, web, App, HttpResponse, HttpServer, Result};
use handlebars::Handlebars;
use serde_json::json;
use std::io;
use std::io::{prelude::*, BufReader};
use std::fs;
use std::fs::File;
use std::path::Path;
use serde::{Serialize, Deserialize};
use std::process::{Command, Stdio};
use std::env;

#[derive(Serialize, Deserialize)]
struct Config {
    port: u16,
    log_file: String,
    server_dir: String
}


fn lines_from_file(filename: impl AsRef<Path>) -> Vec<String> {
    let file = File::open(filename).expect("no such file");
    let buf = BufReader::new(file);
    buf.lines()
        .map(|l| l.expect("Could not parse line"))
        .collect()
}

#[get("/")]
async fn index(hb: web::Data<Handlebars<'_>>) -> HttpResponse {

    //read the config file
    let file = fs::read_to_string("config.json").unwrap();
    let config: Config = serde_json::from_str(&file).unwrap();

    //read the log file
    let log_file = lines_from_file(config.log_file);
    let restarts_log = lines_from_file("../restarts.log");

    let uptime = Command::new("uptime")
        .output()
        .expect("failed to execute process");

    let _onefetch = Command::new("sh")
        .arg("onefetch_file.sh")
        .output()
        .expect("failed");

    let onefetch = fs::read_to_string("onefetch.out").unwrap();


    let data = json!({
        "log_output": log_file,
        "restarts_output": restarts_log,
        "uptime": format!("{}", String::from_utf8_lossy(&uptime.stdout)),
        //"onefetch": format!("{}", String::from_utf8_lossy(&onefetch.stdout))
        "onefetch": onefetch
    });
    let body = hb.render("index", &data).unwrap();

    HttpResponse::Ok().body(body)
}

#[get("/restart_server")]
async fn restart_server() -> HttpResponse {

    // Restart the process
    let _output = Command::new("sh")
        .arg("../run.sh")
        .stdin(Stdio::null())
        //.stdout(Stdio::null())
        //.stderr(Stdio::null())
        .spawn();

    HttpResponse::Ok().body("DONE")
}



#[get("/git_pull")]
async fn git_pull() -> HttpResponse {

    let output = Command::new("git")
        .arg("pull")
        .output()
        .expect("failed to execute process");

    HttpResponse::Ok().body(output.stdout)
}


#[get("/compile")]
async fn compile() -> HttpResponse {

    //read the config file
    let file = fs::read_to_string("config.json").unwrap();
    let config: Config = serde_json::from_str(&file).unwrap();

    let orig_dir = std::env::current_dir().unwrap();

    let root = Path::new(&config.server_dir);
    let _new_dir = env::set_current_dir(&root);

    print!("before");

    let output = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .output()
        .expect("failed to execute process");

    print!("after");
        
    let _new_dir = env::set_current_dir(orig_dir);

    HttpResponse::Ok().body(output.stdout)
}



#[actix_web::main]
async fn main() -> io::Result<()> {

    let mut handlebars = Handlebars::new();
    handlebars.set_dev_mode(true);
    handlebars.register_templates_directory(".hbs", "./static/templates").unwrap();
    let handlebars_ref = web::Data::new(handlebars);

    HttpServer::new(move || {
        App::new()
            .wrap(error_handlers())
            .app_data(handlebars_ref.clone())
            .service(index)
            .service(restart_server)
            .service(git_pull)
            .service(compile)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}

// Custom error handlers, to return HTML responses when an error occurs.
fn error_handlers() -> ErrorHandlers<BoxBody> {
    ErrorHandlers::new().handler(StatusCode::NOT_FOUND, not_found)
}

// Error handler for a 404 Page not found error.
fn not_found<B>(res: ServiceResponse<B>) -> Result<ErrorHandlerResponse<BoxBody>> {
    let response = get_error_response(&res, "Page not found");
    Ok(ErrorHandlerResponse::Response(ServiceResponse::new(
        res.into_parts().0,
        response.map_into_left_body(),
    )))
}

// Generic error handler.
fn get_error_response<B>(res: &ServiceResponse<B>, error: &str) -> HttpResponse<BoxBody> {
    let request = res.request();

    // Provide a fallback to a simple plain text response in case an error occurs during the
    // rendering of the error page.
    let fallback = |e: &str| {
        HttpResponse::build(res.status())
            .content_type(ContentType::plaintext())
            .body(e.to_string())
    };

    let hb = request
        .app_data::<web::Data<Handlebars>>()
        .map(|t| t.get_ref());
    match hb {
        Some(hb) => {
            let data = json!({
                "error": error,
                "status_code": res.status().as_str()
            });
            let body = hb.render("error", &data);

            match body {
                Ok(body) => HttpResponse::build(res.status())
                    .content_type(ContentType::html())
                    .body(body),
                Err(_) => fallback(error),
            }
        }
        None => fallback(error),
    }
}
