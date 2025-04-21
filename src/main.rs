use actix_web::{web, App, HttpResponse, HttpServer, get, post};
use sqlx::postgres::PgPool;
use serde_json::json;
use actix_cors::Cors;
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};
use chrono::{DateTime, Utc};

// Получение очков пользователя
#[get("/points/{telegram_id}")]
async fn get_points(pool: web::Data<PgPool>, telegram_id: web::Path<i64>) -> HttpResponse {
    match sqlx::query!(
        "SELECT game_points FROM users WHERE telegram_id = $1",
        telegram_id.into_inner()
    )
    .fetch_one(pool.get_ref())
    .await {
        Ok(record) => HttpResponse::Ok().json(json!({ "points": record.game_points })),
        Err(sqlx::Error::RowNotFound) => HttpResponse::NotFound().json(json!({ "error": "User not found" })),
        Err(_) => HttpResponse::InternalServerError().json(json!({ "error": "Database error" })),
    }
}

// Обновление очков пользователя
#[post("/points/{telegram_id}")]
async fn update_points(
    pool: web::Data<PgPool>,
    telegram_id: web::Path<i64>,
    data: web::Json<i64>,
) -> HttpResponse {
    let points = data.into_inner();
    
    match sqlx::query!(
        "UPDATE users SET game_points = game_points + $1 WHERE telegram_id = $2 RETURNING game_points",
        points,
        telegram_id.into_inner()
    )
    .fetch_one(pool.get_ref())
    .await {
        Ok(record) => HttpResponse::Ok().json(json!({ "points": record.game_points })),
        Err(sqlx::Error::RowNotFound) => HttpResponse::NotFound().json(json!({ "error": "User not found" })),
        Err(_) => HttpResponse::InternalServerError().json(json!({ "error": "Failed to update points" })),
    }
}

// Получение попыток пользователя
#[get("/attempts/{telegram_id}")]
async fn get_attempts(pool: web::Data<PgPool>, telegram_id: web::Path<i64>) -> HttpResponse {
    match sqlx::query!(
        "SELECT game_attempts FROM users WHERE telegram_id = $1",
        telegram_id.into_inner()
    )
    .fetch_one(pool.get_ref())
    .await {
        Ok(record) => HttpResponse::Ok().json(json!({ "attempts": record.game_attempts })),
        Err(sqlx::Error::RowNotFound) => HttpResponse::NotFound().json(json!({ "error": "User not found" })),
        Err(_) => HttpResponse::InternalServerError().json(json!({ "error": "Database error" })),
    }
}

// Добавление попыток пользователю
#[post("/attempts/{telegram_id}/add")]
async fn add_attempts(
    pool: web::Data<PgPool>,
    telegram_id: web::Path<i64>,
    data: web::Json<i64>,
) -> HttpResponse {
    let attempts_to_add = data.into_inner();
    
    match sqlx::query!(
        "UPDATE users SET game_attempts = game_attempts + $1 WHERE telegram_id = $2 RETURNING game_attempts",
        attempts_to_add,
        telegram_id.into_inner()
    )
    .fetch_one(pool.get_ref())
    .await {
        Ok(record) => HttpResponse::Ok().json(json!({ "attempts": record.game_attempts })),
        Err(sqlx::Error::RowNotFound) => HttpResponse::NotFound().json(json!({ "error": "User not found" })),
        Err(_) => HttpResponse::InternalServerError().json(json!({ "error": "Failed to update attempts" })),
    }
}

// Обновление времени следующего клейма
#[post("/claim/{telegram_id}")]
async fn update_claim_time(
    pool: web::Data<PgPool>,
    telegram_id: web::Path<i64>,
) -> HttpResponse {
    match sqlx::query!(
        r#"
        UPDATE users 
        SET next_claim_time = NOW() + INTERVAL '10 hours'
        WHERE telegram_id = $1
        RETURNING next_claim_time as "next_claim_time: DateTime<Utc>"
        "#,
        telegram_id.into_inner()
    )
    .fetch_one(pool.get_ref())
    .await {
        Ok(record) => HttpResponse::Ok().json(json!({ 
            "next_claim_time": record.next_claim_time.to_rfc3339() 
        })),
        Err(sqlx::Error::RowNotFound) => HttpResponse::NotFound().json(json!({ "error": "User not found" })),
        Err(_) => HttpResponse::InternalServerError().json(json!({ "error": "Failed to update claim time" })),
    }
}

#[get("/claim/{telegram_id}")]
async fn get_claim_time(
    pool: web::Data<PgPool>,
    telegram_id: web::Path<i64>,
) -> HttpResponse {
    match sqlx::query!(
        r#"
        SELECT next_claim_time as "next_claim_time: DateTime<Utc>"
        FROM users 
        WHERE telegram_id = $1
        "#,
        telegram_id.into_inner()
    )
    .fetch_one(pool.get_ref())
    .await {
        Ok(record) => HttpResponse::Ok().json(json!({ 
            "next_claim_time": record.next_claim_time.map(|t| t.to_rfc3339()) 
        })),
        Err(sqlx::Error::RowNotFound) => HttpResponse::NotFound().json(json!({ "error": "User not found" })),
        Err(_) => HttpResponse::InternalServerError().json(json!({ "error": "Database error" })),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();

    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .unwrap();

    // Настройка SSL
    let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;
    builder.set_private_key_file("certs/privkey.pem", SslFiletype::PEM)?;
    builder.set_certificate_chain_file("certs/fullchain.pem")?;

    HttpServer::new(move || {
        let cors = Cors::default()
            .allowed_origin("https://kirillqa17.github.io")
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .app_data(web::Data::new(pool.clone()))
            .service(get_points)
            .service(update_points)
            .service(get_attempts)
            .service(add_attempts)
            .service(update_claim_time)
            .service(get_claim_time)
    })
    .bind_openssl("0.0.0.0:443", builder)?
    .run()
    .await
}