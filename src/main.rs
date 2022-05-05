use axum::{
    routing::{get, post},
    Extension, Router,
};
use log::{error, info, warn};
use tower_http::cors::{CorsLayer, any, Any};
use std::{collections::VecDeque, env, net::SocketAddr, sync::Arc};

use ti_server::{
    api::{get_product_list, get_task_list, set_product_list},
    ProductChange, product_change_notify,
};
use ti_server::{check_tasks_timeout, handle_connection, Product, TiTask};
use tokio::{net::TcpListener, sync::Mutex};

#[tokio::main]
async fn main() {
    // 命令行参数
    let args: Vec<String> = env::args().collect();

    let default_bind = "0.0.0.0:4321".to_string();
    let bind = args.get(1).unwrap_or(&default_bind).clone();

    let default_bind = "0.0.0.0:3210".to_string();
    let api_server = args.get(2).unwrap_or(&default_bind).clone();

    log4rs::init_file("log.yml", Default::default()).unwrap();

    // 创建一个多线程安全的产品队列
    let products = Arc::new(Mutex::new(VecDeque::new()));

    // 创建一个多线程安全的任务链表，方便管理任务
    let tasks = Arc::new(Mutex::new(Vec::<TiTask>::new()));

    check_tasks_timeout(products.clone(), tasks.clone()).await;

    let products_clone = Arc::clone(&products);
    let tasks_clone = Arc::clone(&tasks);

    // 创建一个管道，用于接收产品个数变更的消息
    let (tx, mut rx) = tokio::sync::mpsc::channel::<ProductChange>(100);

    // 创建一个线程，通知任务
    tokio::spawn(async move {
        while let Some(product_change) = rx.recv().await {
            info!("收到产品变更消息：{:?}", product_change);
            product_change_notify(product_change).await;
        }
    });

    // ti_server 分发任务给 ti_worker
    tokio::spawn(async move {
        // 监听127.0.0.1:8000
        let listener = TcpListener::bind(bind).await.unwrap();
        // 循环接收连接

        loop {
            // 接收请求
            let (stream, addr) = listener.accept().await.unwrap();

            let products = Arc::clone(&products_clone);
            let tasks = Arc::clone(&tasks_clone);
            let tx = tx.clone();

            tokio::spawn(async move {
                handle_connection(stream, addr, products, tasks, tx.clone()).await;
            });
        }
    });

    // 提供web接口
    // build our application with a route
    let app = Router::new()
        .route("/products", get(get_product_list))
        .route("/tasks", get(get_task_list))
        .route("/products", post(set_product_list))
        .layer(Extension(products.clone()))
        .layer(Extension(tasks.clone()))
        .layer(
            CorsLayer::new()
                .allow_headers(Any)
                .allow_origin(Any)
                .allow_methods(Any),
        );

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    let addr: SocketAddr = api_server.parse().unwrap();

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
