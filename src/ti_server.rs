use log::{error, info, warn};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::VecDeque, env, net::SocketAddr, sync::Arc};
use ti_protocol::{
    get_header_size, PackType, Packet, PacketHeader, Task, TaskResult, TaskResultError, TiPack,
    TiUnPack,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Mutex,
    time::Duration,
};

// 产品结构体
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub name: String,
    pub rank: i32,
    pub count: i32,
    pub comment: String,
}

impl Product {
    pub fn new(name: String) -> Product {
        Product {
            name,
            rank: 0,
            count: 0,
            comment: String::new(),
        }
    }

    pub fn new_with_rank(name: String, rank: i32) -> Product {
        Product {
            name,
            rank,
            count: 0,
            comment: String::new(),
        }
    }
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct ProductChange {
    pub name: String,
    pub old_count: i32,
    pub count: i32,
    pub comment: String,
}

// 自定义服务端任务结构体，方便记录任务执行情况，方便后期扩展，比如记录执行情况，定时执行等
#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub struct TiTask {
    pub task_id: i32,
    pub product: Product,
    pub created_at: i64,
}

impl TiTask {
    pub fn new(task_id: i32, product: Product) -> TiTask {
        TiTask {
            task_id,
            product,
            // 记录创建任务的时间，超过指定时间还没有返回结果，就认为任务失败
            created_at: chrono::Utc::now().timestamp(),
        }
    }

    // 判断任务是否超时
    pub fn is_timeout(&self) -> bool {
        // 获取当前时间
        let now = chrono::Utc::now().timestamp();
        info!("时间差: {:?}", now - self.created_at);
        // 如果当前时间 - 创建任务的时间 > 超时时间，就认为任务超时
        now - self.created_at > 9
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ActionCard {
    title: String,
    text: String,
    #[serde(rename = "btnOrientation")]
    btn_orientation: i32, // 0:按钮竖直排列，1：按钮横向排列
    btns: Vec<ActionBtn>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ActionBtn {
    title: String,
    #[serde(rename = "actionURL")]
    action_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DingTalkMsg {
    msgtype: String,
    #[serde(rename = "actionCard")]
    action_card: ActionCard,
}

impl DingTalkMsg {
    fn new(title: &str, text: &str) -> Self {
        DingTalkMsg {
            msgtype: "actionCard".to_string(),
            action_card: ActionCard {
                title: title.to_string(),
                text: text.to_string(),
                btn_orientation: 1,
                btns: vec![],
            },
        }
    }
}

pub async fn product_change_notify(product_change: ProductChange) {
    // 库存没变不用通知
    if product_change.old_count == product_change.count {
        return;
    }

    println!("product_change_notify: {:?}", product_change);
    // reqwest post 请求
    let url = "https://oapi.dingtalk.com/robot/send?access_token=84fe9edd9b15be147615e1a3380304657f6ca1769e423b9b56b1d2994e604d17";
    let client = reqwest::Client::new();

    let mut title = String::new();
    let mut text = String::new();

    // 北京时间 时分秒
    let now = chrono::Local::now();
    let now_str = now.format("%Y-%m-%d %H:%M:%S").to_string();

    if product_change.old_count > product_change.count {
        title = format!(
            "库存减少 {}",
            product_change.old_count - product_change.count
        );
        text += format!(
            r#"<font color=#999>库存下降通知 - {}</font>


型号：{}


库存：{}


库存减少：{}"#,
            now_str,
            product_change.name,
            product_change.count,
            product_change.old_count - product_change.count
        )
        .as_str()
    } else {
        title = format!(
            "库存增加 {}",
            product_change.count - product_change.old_count
        );
        text += format!(
            r#"<font color=#006600>库存增加 - {}</font>


<font color=#e00>型号：{}</font>


<font color=#e00>库存：{}</font>


库存增加：{}"#,
            now_str,
            product_change.name,
            product_change.count,
            product_change.count - product_change.old_count
        )
        .as_str()
    }

    // 如果备注信息不为空，添加备注
    if !product_change.comment.is_empty() {
        text += format!("\n\n备注：{}", product_change.comment).as_str();
    }

    let mut msg = DingTalkMsg::new(&title, text.as_str());
    msg.action_card.btns.push(ActionBtn {
        title: "型号详情".to_string(),
        action_url: format!(
            "dingtalk://dingtalkclient/page/link?url={}&pc_slide=false",
            urlencoding::encode(
                &("https://www.ti.com.cn/store/ti/zh/p/product/?p=".to_string()
                    + &product_change.name)
            )
        ),
    });
    msg.action_card.btns.push(ActionBtn {
        title: "交叉搜索下单".to_string(),
        action_url: format!(
            "dingtalk://dingtalkclient/page/link?url={}&pc_slide=false",
            urlencoding::encode(
                &("https://www.ti.com.cn/cross-reference-search/zh-cn/singlepart?searchTerm="
                    .to_string()
                    + &product_change.name)
            )
        ),
    });
    println!("msg: {}", serde_json::to_string(&msg).unwrap());
    let res = match client.post(url).json(&msg).send().await {
        Ok(res) => res,
        Err(e) => {
            error!("产品：{:?}\t通知失败:{:?}", product_change, e);
            return;
        }
    };
    let body = res.text().await;
    println!("{:?}", body);
}

pub async fn handle_connection(
    mut stream: TcpStream,
    addr: SocketAddr,
    products: Arc<Mutex<VecDeque<Product>>>,
    tasks: Arc<Mutex<Vec<TiTask>>>,
    tx: tokio::sync::mpsc::Sender<ProductChange>,
) {
    let len: usize = get_header_size();
    loop {
        // thread::sleep(Duration::from_millis(10));

        let mut header = vec![0; len];
        stream.read(&mut header).await.unwrap();
        let header = PacketHeader::unpack(&header).unwrap();

        // 检查标志位，不对就跳过
        if !header.check_flag() {
            continue;
        }

        // 如果有内容才读取
        let body = if header.body_size > 0 {
            let mut v = vec![0; header.body_size as usize];
            stream.read(&mut v).await.unwrap();
            v
        } else {
            vec![]
        };

        // 根据数据类型解析数据
        match header.pack_type {
            // 表示客户端要获取任务
            PackType::GetTask => {
                // 生成一个任务列表中不存在的随机数，作为任务的id
                let mut tasks = tasks.lock().await;
                let mut products = products.lock().await;
                let task_id = loop {
                    let x: i32 = {
                        let mut rng = rand::thread_rng();
                        rng.gen_range(1..99999999)
                    };
                    // 判断id是否在任务列表中存在
                    if !tasks.iter().any(|t| t.task_id == x) {
                        break x;
                    }
                };

                // 根据产品列表生成一个任务，返回给客户端
                let task = loop {
                    match products.pop_front() {
                        Some(mut product) => {
                            let task = Task::new(task_id, product.name.clone());

                            // 如果rank为负数，就加1，并把product放回到队列中,然后继续获取下一个产品
                            // 在获取到产品有库存后，会降低rank为负数的产品的rank，
                            // 减少已经有库存的查询次数，留更多机会给其他产品执行
                            if product.rank < 0 {
                                product.rank += 1;
                                products.push_back(product);
                                continue;
                            }

                            // 将任务添加到任务列表中，用于后面返回结果时查询任务信息
                            tasks.push(TiTask::new(task.task_id, product));
                            break task;
                        }
                        None => break Task::new(0, "".to_string()),
                    };
                };

                info!("发送：{:?}\t到\t{:?}", task, addr);

                // 封包 发送
                let packet = Packet::new(PackType::Task, task).unwrap();
                let data = packet.pack().unwrap();
                stream.write_all(&data).await.unwrap();
                stream.flush().await.unwrap();
            }
            PackType::TaskResult => {
                let task_result = TaskResult::unpack(&body).unwrap();
                info!("{:?}", task_result);

                // 根据任务信息生成新的产品，根据执行结果决定添加到 头部 还是 尾部
                let mut tasks = tasks.lock().await;
                let mut products = products.lock().await;

                let task = match tasks.iter().find(|t| t.task_id == task_result.task_id) {
                    Some(t) => t,
                    // 任务可能超时后才返回结果，所以这里可能没有找到任务
                    None => {
                        info!("任务不存在");
                        continue;
                    }
                };

                info!("返回任务：{:?}", task);

                // 任务列表中保存的之前的产品信息
                let mut product = Product {
                    name: task.product.name.clone(),
                    rank: task.product.rank,
                    count: task.product.count,
                    comment: task.product.comment.clone(),
                };

                // 从任务列表中删除
                tasks.retain(|t| t.task_id != task_result.task_id);

                match task_result.result {
                    // 成功返回结果，就直接把产品添加到产品列表末尾
                    Ok(product_count) => {
                        // 如果产品数量大于0，并且产品rank小于等于0的话，就将rank-1，降低执行优先级
                        // 因为rank大于0，表示用户特别关注，所以不受库存和rank影响
                        if product_count > 0 && product.rank <= 0 {
                            product.rank -= 1;
                        }

                        // 如果产品数量和之前的不同
                        if product.count != product_count {
                            if let Err(e) = tx
                                .send(ProductChange {
                                    name: product.name.clone(),
                                    count: product_count,
                                    old_count: product.count,
                                    comment: product.comment.clone(),
                                })
                                .await
                            {
                                error!("发送通知消息失败：{:?}", e);
                            }
                            // 修改产品数量
                            product.count = product_count;
                        }

                        products.push_back(product);
                        info!("获取到的产品个数：{:?}", product_count);
                    }
                    // 如果失败，就把产品添加到产品列表前面，方便其他客户端获取
                    Err(e) => {
                        if e == TaskResultError::ProductNotFound {
                            info!("产品不存在");
                            continue;
                        }
                        products.push_front(product);
                        info!("获取到的产品失败：{:?}", e);
                    }
                }
            }
            _ => {}
        }
    }
}

pub async fn check_tasks_timeout(
    products: Arc<Mutex<VecDeque<Product>>>,
    tasks: Arc<Mutex<Vec<TiTask>>>,
) {
    tokio::spawn(async move {
        loop {
            // 睡眠5秒,睡眠放前面，放后面的话tasks.lock()会被阻塞
            tokio::time::sleep(Duration::from_millis(2000)).await;

            // 检查任务是否超时
            let mut tasks = tasks.lock().await;
            let mut products = products.lock().await;

            // 循环遍历所有任务
            for task in tasks.iter() {
                // 如果任务超时，就把任务从任务列表中移除
                if task.is_timeout() {
                    // 根据任务创建产品，并且添加到产品队列的头部
                    let product = Product {
                        name: task.product.name.clone(),
                        rank: task.product.rank,
                        count: task.product.count,
                        comment: task.product.comment.clone(),
                    };
                    products.push_front(product);
                }
            }
            // 删除超时的任务
            tasks.retain(|task| !task.is_timeout());

            info!("任务列表：{:?}", tasks);
            // 产品列表
            info!("产品列表：{:?}", products);
        }
    });
}

#[cfg(test)]
mod tests {
    use crate::{product_change_notify, ProductChange};

    // test product_change_notify
    #[test]
    fn test_product_change_notify() {
        // block on the future
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            product_change_notify(ProductChange {
                name: "111111111111".to_string(),
                old_count: 1000,
                count: 200,
                comment: "不限购".to_string(),
            })
            .await;
        });
    }
}
