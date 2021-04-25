use crate::args::DaemonArgs;
use crate::chall;
use crate::config::Config;
use crate::errors::*;
use crate::http_responses::*;
use crate::sandbox;
use crate::utils;
use actix_web::{get, web, HttpRequest, HttpResponse, Responder};
use actix_web::{middleware, App, HttpServer};
use nix::sys::stat::Mode;
use std::env;
use std::fs;
use std::net::TcpListener;
use std::path::Path;

fn get_host(req: &HttpRequest) -> Option<&str> {
    if let Some(host) = req.headers().get("Host") {
        if let Ok(host) = host.to_str() {
            Some(host)
        } else {
            None
        }
    } else {
        None
    }
}

#[inline]
fn bad_request() -> HttpResponse {
    HttpResponse::BadRequest().body(BAD_REQUEST)
}

#[inline]
fn not_found() -> HttpResponse {
    HttpResponse::NotFound().body(NOT_FOUND)
}

#[get("/{p:.*}")]
async fn redirect(req: HttpRequest) -> impl Responder {
    debug!("REQ: {:?}", req);

    let host = if let Some(host) = get_host(&req) {
        host
    } else {
        return bad_request();
    };
    debug!("host: {:?}", host);

    let path = req.uri();
    debug!("path: {:?}", path);

    let url = format!("https://{}{}", host, path);
    if url.chars().any(|c| c == '\n' || c == '\r') {
        return bad_request();
    }

    HttpResponse::MovedPermanently()
        .header("Location", url)
        .body(REDIRECT)
}

#[get("/.well-known/acme-challenge/{chall}")]
async fn acme(token: web::Path<String>, req: HttpRequest) -> impl Responder {
    debug!("REQ: {:?}", req);
    info!("acme: {:?}", token);

    if !chall::valid_token(&token) {
        return bad_request();
    }

    let path = Path::new("challs").join(token.as_ref());
    debug!("Reading challenge proof: {:?}", path);
    if let Ok(proof) = fs::read(path) {
        HttpResponse::Ok().body(proof)
    } else {
        not_found()
    }
}

#[actix_web::main]
pub async fn spawn(socket: TcpListener) -> Result<()> {
    HttpServer::new(move || {
        App::new()
            // enable logger
            .wrap(middleware::Logger::default())
            .service(acme)
            .service(redirect)
    })
    .listen(socket)
    .context("Failed to bind socket")?
    .run()
    .await
    .context("Failed to start http daemon")?;
    Ok(())
}

fn setup(config: &Config, args: &DaemonArgs) -> Result<()> {
    let path = config.data_dir.as_path();
    let mode = Mode::from_bits(0o750).unwrap();

    if !path.exists() {
        debug!("Creating data directory: {:?}", path);
        nix::unistd::mkdir(path, mode).context("Failed to create data directory")?;
    }

    let md = fs::metadata(path).context("Failed to stat data directory")?;
    utils::ensure_chmod(path, &md, mode.bits())
        .context("Failed to set permissions of data directory")?;

    if let Some(name) = &args.user {
        let user = users::get_user_by_name(name)
            .ok_or_else(|| anyhow!("Failed to resolve user: {:?}", name))?;
        utils::ensure_chown(path, &md, &user)?;
    }

    if let Some(name) = &config.group {
        let group = users::get_group_by_name(name)
            .ok_or_else(|| anyhow!("Failed to resolve group: {:?}", name))?;
        utils::ensure_chgrp(path, &md, &group)?;
    }

    Ok(())
}

pub fn run(config: Config, args: DaemonArgs) -> Result<()> {
    setup(&config, &args).context("Failed to run setup")?;

    env::set_current_dir(&config.chall_dir)?;
    let socket = TcpListener::bind(&args.bind_addr).context("Failed to bind socket")?;
    sandbox::init(&args).context("Failed to drop privileges")?;
    spawn(socket)
}
