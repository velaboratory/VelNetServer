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
use std::fs::{File, OpenOptions};
use std::path::Path;
use serde::{Serialize, Deserialize};
use std::process::{Command};
use std::cmp;
use std::error::Error;
use simplelog;

#[derive(Serialize, Deserialize)]
struct ControlPanelConfig {
    port: u16,
    user: String,
    server_log_file: String,
    control_panel_log_file: String,
    server_dir: String,
    handlebars_dev_mode: bool,
}


fn lines_from_file(filename: impl AsRef<Path>) -> Vec<String> {
    if filename.as_ref().is_file() {
        let file = File::open(filename).expect("no such file");
        let buf = BufReader::new(file);
        buf.lines()
            .map(|l| l.expect("Could not parse line"))
            .collect()
    } else {
        vec![]
    }
}

#[get("/")]
async fn index(hb: web::Data<Handlebars<'_>>) -> HttpResponse {
    let config = read_config_file().unwrap();

    //read the log file
    let log_file = lines_from_file(config.server_log_file);

    // let restarts_log = lines_from_file(config.restarts_log_file);

    let uptime = Command::new("tuptime")
        .output()
        .expect("failed to execute process");

    // let _onefetch = Command::new("sh")
    //     .arg("onefetch_file.sh")
    //     .arg(config.user)
    //     .output()
    //     .expect("failed");

    // let onefetch = fs::read_to_string("onefetch.out").unwrap();

    let data = json!({
        "log_output": &log_file[(cmp::max((log_file.len() as i64) - 1000, 0) as usize)..],
        // "restarts_output": &restarts_log[(cmp::max((restarts_log.len() as i64) - 1000, 0) as usize)..],
        "uptime": format!("{}", String::from_utf8_lossy(&uptime.stdout)),
        //"onefetch": format!("{}", String::from_utf8_lossy(&onefetch.stdout))
        "onefetch": ""
    });
    let body = hb.render("index", &data).unwrap();

    HttpResponse::Ok().body(body)
}

#[get("/restart_server")]
async fn restart_server() -> HttpResponse {

    // Restart the process
    // let _output = Command::new("sh")
    //     .arg("../restart_server.sh")
    //     .stdin(Stdio::null())
    //     .spawn();
    // let _output = Command::new("systemd")
    //     .arg("restart")
    //     .arg("velnet")
    //     .stdin(Stdio::null())
    //     .spawn();

    systemctl::restart("velnet.service").unwrap();

    let ret = systemctl::status("velnet.service").unwrap();

    HttpResponse::Ok().body(ret)
}


#[get("/git_pull")]
async fn git_pull() -> HttpResponse {

    //read the config file
    let config = read_config_file().unwrap();

    let output = Command::new("sh")
        .arg("git_pull.sh")
        .arg(config.user)
        .output()
        .expect("failed to execute process");

    HttpResponse::Ok().body(output.stdout)
}


#[get("/compile")]
async fn compile() -> HttpResponse {

    //read the config file
    let config = read_config_file().unwrap();

    log::debug!("before");

    let output = Command::new("sh")
        .arg("compile_server.sh")
        .arg(config.user)
        .output()
        .expect("failed to execute process");

    log::debug!("after");

    HttpResponse::Ok().body(output.stdout)
}

fn read_config_file() -> Result<ControlPanelConfig, Box<dyn Error>> {
    // Open the file in read-only mode with buffer.
    let file = File::open("config.json")?;
    let reader = BufReader::new(file);

    let config = serde_json::from_reader(reader)?;

    Ok(config)
}


#[actix_web::main]
async fn main() -> io::Result<()> {

    //read the config file
    let config: ControlPanelConfig = read_config_file().unwrap();

    let f = OpenOptions::new()
        .write(true)
        .append(true)
        .create(true)
        .open(config.control_panel_log_file)
        .unwrap();
    simplelog::CombinedLogger::init(
        vec![
            simplelog::TermLogger::new(simplelog::LevelFilter::Info, simplelog::Config::default(), simplelog::TerminalMode::Mixed, simplelog::ColorChoice::Auto),
            simplelog::WriteLogger::new(simplelog::LevelFilter::Debug, simplelog::Config::default(), f),
        ]
    ).unwrap();


    log::info!("Starting control panel server.");


    let mut handlebars = Handlebars::new();
    handlebars.set_dev_mode(config.handlebars_dev_mode);
    handlebars.register_templates_directory(".hbs", "./static/templates").unwrap();
    let handlebars_ref = web::Data::new(handlebars);


    log::info!("http://127.0.0.1:{}", config.port);

    HttpServer::new(move || {
        App::new()
            .wrap(error_handlers())
            .app_data(handlebars_ref.clone())
            .service(index)
            .service(restart_server)
            .service(git_pull)
            .service(compile)
            .service(actix_files::Files::new("/static", "./static"))
    })
        .bind(("0.0.0.0", config.port))?
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
