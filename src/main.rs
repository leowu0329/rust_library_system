use mongodb::{options::ClientOptions, Client};
use rust_library_system::create_app;
use tracing::Level;
use tracing_subscriber::{filter::filter_fn, layer::SubscriberExt, util::SubscriberInitExt, Layer};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    // 使用內建 filter_fn：過濾掉 tower_sessions_core 的非 ERROR 級別開發警告，其餘維持 INFO 以上輸出
    let session_filter = filter_fn(|metadata| {
        if metadata.target().starts_with("tower_sessions_core") {
            *metadata.level() == Level::ERROR
        } else {
            *metadata.level() <= Level::INFO
        }
    });

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(session_filter))
        .init();

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

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    tracing::info!("伺服器已安全關閉");
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("無法註冊 Ctrl+C 關機訊號監聽");
    tracing::info!("收到 Ctrl+C 訊號，正在關閉伺服器...");
}