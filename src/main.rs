use actix_web::{web, App, HttpResponse, HttpServer, get, post};
use sqlx::postgres::PgPool;
use serde_json::json;
use actix_cors::Cors;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use reqwest::Client as HttpClient;

#[derive(Debug, Serialize, Deserialize)]
struct Task {
    id: i32,
    title: String,
    description: String,
    reward_coins: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserTask {
    task_id: i32,
    title: String,
    description: String,
    reward_coins: i64,
    progress: i32,
    target: i32,
    is_completed: bool,
}

// Получение списка заданий для пользователя
#[get("/tasks/{telegram_id}")]
async fn get_user_tasks(pool: web::Data<PgPool>, telegram_id: web::Path<i64>) -> HttpResponse {
    match sqlx::query_as!(
        UserTask,
        r#"
        SELECT 
            t.id as task_id,
            t.title,
            t.description,
            t.reward_coins,
            ut.progress,
            ut.target,
            ut.is_completed
        FROM tasks t
        JOIN user_tasks ut ON t.id = ut.task_id
        WHERE ut.user_id = $1
        ORDER BY ut.is_completed, t.id
        "#,
        telegram_id.into_inner()
    )
    .fetch_all(pool.get_ref())
    .await {
        Ok(tasks) => HttpResponse::Ok().json(tasks),
        Err(sqlx::Error::RowNotFound) => HttpResponse::NotFound().json(json!({ "error": "User not found" })),
        Err(e) => {
            println!("Database error: {:?}", e);
            HttpResponse::InternalServerError().json(json!({ "error": "Database error" }))
        },
    }
}

// Обновление прогресса задания
#[post("/tasks/{telegram_id}/update/{task_id}")]
async fn update_task_progress(
    pool: web::Data<PgPool>,
    path: web::Path<(i64, i32)>,
    data: web::Json<i32>,
) -> HttpResponse {
    let (telegram_id, task_id) = path.into_inner();
    let progress = data.into_inner();
    
    match sqlx::query!(
        r#"
        WITH updated AS (
            UPDATE user_tasks 
            SET progress = LEAST($1, target),
                is_completed = (LEAST($1, target) >= target),
                completed_at = CASE 
                    WHEN (LEAST($1, target) >= target) AND NOT is_completed THEN NOW()
                    ELSE completed_at 
                END
            WHERE user_id = $2 AND task_id = $3
            RETURNING *
        )
        SELECT 
            u.is_completed as "is_completed!",
            u.progress as "progress!",
            t.reward_coins as "reward_coins!"
        FROM updated u
        JOIN tasks t ON u.task_id = t.id
        "#,
        progress,
        telegram_id,
        task_id
    )
    .fetch_one(pool.get_ref())
    .await {
        Ok(record) => {
            let response = if record.is_completed {
                // Если задание выполнено, добавляем награду
                match sqlx::query!(
                    "UPDATE users SET game_points = game_points + $1 WHERE telegram_id = $2 RETURNING game_points",
                    record.reward_coins,
                    telegram_id
                )
                .fetch_one(pool.get_ref())
                .await {
                    Ok(user) => json!({
                        "status": "completed",
                        "progress": record.progress,
                        "reward": record.reward_coins,
                        "new_points": user.game_points
                    }),
                    Err(_) => json!({
                        "status": "completed",
                        "progress": record.progress,
                        "reward": record.reward_coins,
                        "error": "Failed to add reward"
                    })
                }
            } else {
                json!({
                    "status": "updated",
                    "progress": record.progress
                })
            };
            
            HttpResponse::Ok().json(response)
        },
        Err(sqlx::Error::RowNotFound) => HttpResponse::NotFound().json(json!({ "error": "Task not found" })),
        Err(_) => HttpResponse::InternalServerError().json(json!({ "error": "Failed to update task" })),
    }
}

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

#[post("/attempts/{telegram_id}")]
async fn update_attempts(
    pool: web::Data<PgPool>,
    telegram_id: web::Path<i64>,
    data: web::Json<i64>,
) -> HttpResponse {
    let new_attempts = data.into_inner();
    
    match sqlx::query!(
        "UPDATE users SET game_attempts = $1 WHERE telegram_id = $2 RETURNING game_attempts",
        new_attempts,
        telegram_id.into_inner()
    )
    .fetch_one(pool.get_ref())
    .await {
        Ok(record) => HttpResponse::Ok().json(json!({ "attempts": record.game_attempts })),
        Err(sqlx::Error::RowNotFound) => HttpResponse::NotFound().json(json!({ "error": "User not found" })),
        Err(_) => HttpResponse::InternalServerError().json(json!({ "error": "Failed to update attempts" })),
    }
}

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
        SELECT next_claim_time as "next_claim_time: Option<DateTime<Utc>>"
        FROM users 
        WHERE telegram_id = $1
        "#,
        telegram_id.into_inner()
    )
    .fetch_one(pool.get_ref())
    .await {
        Ok(record) => {
            let next_claim_time = record.next_claim_time.map(|t| t.to_rfc3339());
            HttpResponse::Ok().json(json!({ 
                "next_claim_time": next_claim_time
            }))
        },
        Err(sqlx::Error::RowNotFound) => HttpResponse::NotFound().json(json!({ "error": "User not found" })),
        Err(_) => HttpResponse::InternalServerError().json(json!({ "error": "Database error" })),
    }
}

#[get("/record/{telegram_id}")]
async fn get_record(pool: web::Data<PgPool>, telegram_id: web::Path<i64>) -> HttpResponse {
    match sqlx::query!(
        "SELECT record_flappy FROM users WHERE telegram_id = $1",
        telegram_id.into_inner()
    )
    .fetch_one(pool.get_ref())
    .await {
        Ok(record) => HttpResponse::Ok().json(json!({ "record": record.record_flappy })),
        Err(sqlx::Error::RowNotFound) => HttpResponse::NotFound().json(json!({ "error": "User not found" })),
        Err(_) => HttpResponse::InternalServerError().json(json!({ "error": "Database error" })),
    }
}

// Обновление рекорда пользователя
#[post("/record/{telegram_id}")]
async fn update_record(
    pool: web::Data<PgPool>,
    telegram_id: web::Path<i64>,
    data: web::Json<i64>,
) -> HttpResponse {
    let new_record = data.into_inner();
    
    match sqlx::query!(
        r#"
        UPDATE users 
        SET record_flappy = GREATEST(COALESCE(record_flappy, 0), $1)
        WHERE telegram_id = $2 
        RETURNING record_flappy
        "#,
        new_record,
        telegram_id.into_inner()
    )
    .fetch_one(pool.get_ref())
    .await {
        Ok(record) => HttpResponse::Ok().json(json!({ "record": record.record_flappy })),
        Err(sqlx::Error::RowNotFound) => HttpResponse::NotFound().json(json!({ "error": "User not found" })),
        Err(_) => HttpResponse::InternalServerError().json(json!({ "error": "Failed to update record" })),
    }
}

#[post("/exchange/{telegram_id}")]
async fn exchange_coins(
    pool: web::Data<PgPool>,
    telegram_id: web::Path<i64>,
    data: web::Json<i64>,
) -> HttpResponse {
    let days = data.into_inner();
    let telegram_id = telegram_id.into_inner();
    
    const COINS_PER_DAY: i64 = 30;
    let coins = days * COINS_PER_DAY;

    let http_client = HttpClient::new();
    
    // Начинаем транзакцию
    let mut transaction = match pool.begin().await {
        Ok(t) => t,
        Err(_e) => {
            return HttpResponse::InternalServerError().json(json!({ "error": "Failed to start transaction" }));
        },
    };
    
    // 1. Проверяем баланс и план пользователя
    let (user_points, current_plan) = match sqlx::query!(
        r#"
        SELECT 
            game_points, 
            plan 
        FROM users 
        WHERE telegram_id = $1 
        "#,
        telegram_id
    )
    .fetch_one(&mut *transaction)
    .await {
        Ok(record) => (record.game_points, record.plan),
        Err(sqlx::Error::RowNotFound) => {
            return HttpResponse::NotFound().json(json!({ "error": "User not found" }));
        }
        Err(_e) => {
            return HttpResponse::InternalServerError().json(json!({ "error": "Database error" }));
        }
    };

    if user_points < coins {
        return HttpResponse::BadRequest().json(json!({
            "error": "Not enough coins",
            "current_plan": current_plan
        }));
    }

    let remaining_coins = user_points - coins;

    // 2. Списываем монеты
    match sqlx::query!(
        "UPDATE users SET game_points = game_points - $1 WHERE telegram_id = $2 RETURNING game_points",
        coins,
        telegram_id
    )
    .fetch_one(&mut *transaction)
    .await {
        Ok(_) => (),
        Err(_e) => {
            return HttpResponse::InternalServerError().json(json!({ "error": "Failed to update coins" }));
        },
    }

    if let Err(e) = transaction.commit().await {
        return HttpResponse::InternalServerError().json(json!({ 
            "error": "Failed to commit transaction",
            "details": e.to_string()
        }));
    }
    
    // 3. Добавляем дни подписки
    let api_url = format!("http://127.0.0.1:8080/users/{}/extend", telegram_id);
    let request_body = json!({
        "days": days,
        "plan": current_plan
    });
    
    let api_response = match http_client
        .patch(&api_url)
        .header("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(30))
        .json(&request_body)
        .send()
        .await {
            Ok(resp) => resp,
            Err(e) => {
                return HttpResponse::InternalServerError().json(json!({ 
                    "error": "Failed to call extend subscription API",
                    "details": e.to_string() 
                }));
            },
        };

    if !api_response.status().is_success() {
        let status = api_response.status();
        let error_text = match api_response.text().await {
            Ok(text) => text,
            Err(e) => format!("Failed to read error response: {}", e),
        };
        
        return HttpResponse::InternalServerError().json(json!({ 
            "error": "Failed to extend subscription",
            "api_status": status.as_u16(),
            "api_response": error_text
        }));
    }

    match api_response.json::<serde_json::Value>().await {
        Ok(body) => {
            let subscription_end = body["subscription_end"].as_str().unwrap_or_default();
            HttpResponse::Ok().json(json!({
                "success": true,
                "new_coin_balance": remaining_coins,
                "subscription_end": subscription_end,
                "days_added": days,
                "is_active": 1
            }))
        },
        Err(e) => {
            HttpResponse::InternalServerError().json(json!({ 
                "error": "Failed to parse API response",
                "details": e.to_string()
            }))
        },
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();

    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .unwrap();

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
            .service(update_attempts)
            .service(update_claim_time)
            .service(get_claim_time)
            .service(get_record)
            .service(update_record)
            .service(exchange_coins)

    })
    .bind("0.0.0.0:1904")?
    .run()
    .await
}