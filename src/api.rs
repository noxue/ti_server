use std::{collections::VecDeque, sync::Arc};

use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{res::Res, Product, TiTask};

pub async fn get_product_list(
    Extension(products): Extension<Arc<Mutex<VecDeque<Product>>>>,
) -> Json<Res<Vec<Product>>> {
    let products = products.lock().await;
    let mut product_list = Vec::new();
    for product in products.iter() {
        product_list.push(Product {
            name: product.name.clone(),
            rank: product.rank,
            count: product.count,
            comment: product.comment.clone(),
        });
    }
    let mut res = Res::default();
    res.set_data(product_list);
    Json(res)
}

pub async fn get_task_list(
    Extension(tasks): Extension<Arc<Mutex<Vec<TiTask>>>>,
) -> Json<Res<Vec<TiTask>>> {
    let tasks = tasks.lock().await;
    let mut task_list = Vec::new();
    for task in tasks.iter() {
        task_list.push(TiTask {
            task_id: task.task_id,
            product: task.product.clone(),
            created_at: task.created_at,
        });
    }
    let mut res = Res::default();
    res.set_data(task_list);
    Json(res)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SetProduct {
    name: String,
    comment:Option<String>,
    rank: Option<i32>,
}

pub async fn set_product_list(
    Extension(products): Extension<Arc<Mutex<VecDeque<Product>>>>,
    Extension(tasks): Extension<Arc<Mutex<Vec<TiTask>>>>,
    Json(product_list): Json<Vec<SetProduct>>,
) -> Json<Res> {
    // 清空任务和产品列表，然后重新添加
    let mut products = products.lock().await;
    products.clear();
    let mut tasks = tasks.lock().await;
    tasks.clear();

    for product in product_list.iter() {
        products.push_back(Product{
            name: product.name.clone(),
            comment: product.comment.as_ref().unwrap_or(&"".to_string()).to_string(),
            rank: product.rank.unwrap_or_default(),
            count: 0,
        });
    }
    Json(Res::default())
}
