use actix_web::{web, App, HttpResponse, HttpServer};
use sqlx::postgres::PgPool;
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};
use actix_cors::Cors;


async fn result(pool: web::Data<PgPool>, telegram_id: web::Path<i64>, data: web::Json<i64>) -> HttpResponse {
    let game_result = data.into_inner();
    let telegram_id = telegram_id.into_inner();
    
    // Обновляем количество очков в базе данных
    let result = match sqlx::query!(
        r#"
        UPDATE users 
        SET game_points = game_points + $1
        WHERE telegram_id = $2
        "#,
        game_result,
        telegram_id
    )
    .execute(pool.get_ref())
    .await {
        Ok(result) => {
            if result.rows_affected() == 0 {
                HttpResponse::NotFound().body("User not found")
            } else {
                HttpResponse::Ok().body("Game points updated successfully")
            }
        }
        Err(_) => HttpResponse::InternalServerError().body("Failed to update game results"),
    };
    
    result
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
        .allowed_origin("https://https://kirillqa17.github.io/")
        .allow_any_method()
        .allow_any_header()
        .max_age(3600);

        App::new()
            .wrap(cors)
            .app_data(web::Data::new(pool.clone()))
            .service(web::resource("/result/{telegram_id}").route(web::post().to(result)))
    })
    .bind_openssl("0.0.0.0:443", builder)?
    .run()
    .await
}
