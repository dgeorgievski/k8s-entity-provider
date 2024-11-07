use crate::routes::{
    api::v1 as api_v1,
    health_check, 
    bs_provider_version};
use crate::configuration::Settings;
use crate::ax_types::Db;
use actix_web::dev::Server;
use actix_web::{web, get, App, HttpServer, HttpResponse};
use std::net::TcpListener;
use actix_web::dev::{ServiceResponse, ServiceRequest};
use tracing_actix_web::{TracingLogger, DefaultRootSpanBuilder, RootSpanBuilder, Level};
use actix_web::Error;
use tracing::Span;
use crate::backstage::entities;

pub struct CustomLevelRootSpanBuilder;


impl RootSpanBuilder for CustomLevelRootSpanBuilder {
    fn on_request_start(request: &ServiceRequest) -> Span {
        let level = match request.path() {
            "/healthz" => Level::DEBUG,
            "/api/v1/entities" => Level::INFO,
            _ => Level::INFO
        };
        tracing_actix_web::root_span!(level = level, request)
    }

    fn on_request_end<B: actix_web::body::MessageBody>(span: Span, outcome: &Result<ServiceResponse<B>, Error>) {
        DefaultRootSpanBuilder::on_request_end(span, outcome);
    }
}


#[get("/")]
async fn index(data: web::Data<Settings>) -> HttpResponse {
    let welcome = format!("Welcome to {}!", data.display);
    HttpResponse::Ok().body(welcome)
}

pub fn run(listener: TcpListener, 
    conf: &Settings,
    cache: Db) -> Result<Server, std::io::Error> {
    let web_config = web::Data::new(conf.clone()); 
    let bs_groups = entities::Group::groups_from_config(conf.backstage.clone());    
    let bs_users = entities::User::users_from_config(conf.backstage.clone());    
    let bs_domains = entities::Domain::domains_from_config(conf.backstage.clone());

    let server = HttpServer::new(move || {
        let api_v1 = web::scope("/api/v1")
                    .app_data(web::Data::new(cache.clone()))
                    .app_data(web::Data::new(bs_groups.clone()))
                    .app_data(web::Data::new(bs_users.clone()))
                    .app_data(web::Data::new(bs_domains.clone()))
                    .service(web::resource("/entities").to(api_v1::entities::get_entities))
                    .service(web::resource("/redis/status").to(api_v1::entities::redis_status));

        App::new()
            .app_data(web_config.clone())
            .wrap(TracingLogger::<CustomLevelRootSpanBuilder>::new())
            .service(index)
            .service(bs_provider_version)
            .service(api_v1)
            .route("/healthz", web::get().to(health_check))
    })
    .listen(listener)?
    .run();

    Ok(server)
}
