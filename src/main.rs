use mongodb::{options::ClientOptions, Client};
use rust_library_system::create_app;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let mongo_uri = std::env::var("MONGODB_URI").expect("請先在 .env 設定 MONGODB_URI");
    let db_name = std::env::var("MONGODB_DB_NAME").unwrap_or_else(|_| "library_system".to_string());

    let client_options = ClientOptions::parse(&mongo_uri)
        .await
        .expect("無效的 MongoDB 連線字串");
    let client = Client::with_options(client_options).expect("建立 MongoDB Client 失敗");
    let db = client.database(&db_name);

    db.run_command(bson::doc! { "ping": 1 })
        .await
        .expect("無法連線至 MongoDB Cloud");
    tracing::info!("成功連線至 MongoDB Cloud 資料庫: {}", db_name);

    let app = create_app(db);

    let addr = format!(
        "{}:{}",
        std::env::var("APP_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
        std::env::var("APP_PORT").unwrap_or_else(|_| "3000".to_string())
    );

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("系統已啟動於 http://{}", addr);
    axum::serve(listener, app).await.unwrap();
}
