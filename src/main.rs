use std::{collections::VecDeque, net::SocketAddr, sync::Arc};
use log::{info, warn, error};
use rand::Rng;
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

async fn handle_connection(
    mut stream: TcpStream,
    addr: SocketAddr,
    products: Arc<Mutex<VecDeque<Product>>>,
    tasks: Arc<Mutex<Vec<TiTask>>>,
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
                            tasks.push(TiTask::new(task_id, product.name.clone()));
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

                let mut product = Product::new(task.product_name.clone());

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

// 产品结构体
#[derive(PartialEq, Debug)]
struct Product {
    name: String,
    rank: i32,
}

impl Product {
    fn new(name: String) -> Product {
        Product { name, rank: 0 }
    }

    fn new_with_rank(name: String, rank: i32) -> Product {
        Product { name, rank }
    }
}

// 自定义服务端任务结构体，方便记录任务执行情况，方便后期扩展，比如记录执行情况，定时执行等
#[derive(PartialEq, Debug)]
struct TiTask {
    task_id: i32,
    product_name: String,
    created_at: i64,
}

impl TiTask {
    fn new(task_id: i32, product_name: String) -> TiTask {
        TiTask {
            task_id,
            product_name,
            // 记录创建任务的时间，超过指定时间还没有返回结果，就认为任务失败
            created_at: chrono::Utc::now().timestamp(),
        }
    }

    // 判断任务是否超时
    fn is_timeout(&self) -> bool {
        // 获取当前时间
        let now = chrono::Utc::now().timestamp();
        info!("时间差: {:?}", now - self.created_at);
        // 如果当前时间 - 创建任务的时间 > 超时时间，就认为任务超时
        now - self.created_at > 9
    }
}

async fn check_tasks_timeout(
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
                    let product = Product::new(task.product_name.clone());
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

#[tokio::main]
async fn main() {

    log4rs::init_file("log.yml", Default::default()).unwrap();

    // 创建一个多线程安全的产品队列
    let products = Arc::new(Mutex::new(VecDeque::new()));

    // 创建一个多线程安全的任务链表，方便管理任务
    let tasks = Arc::new(Mutex::new(Vec::<TiTask>::new()));

    check_tasks_timeout(products.clone(), tasks.clone()).await;

    let product_names = r#"
    1111
    MSP430F2274IRHAR
MSP430F2274IRHAT
eeee
AM3894CCYG120
AM3894CCYGA120
TMS320F2808PZA
66666666
AM3354BZCZ80
AM5716AABCDA
55555
MSP430F5659IZCAR
TM4C1231H6PMI7
77777
TM4C1237H6PZI
66AK2H12DAAW24
F280045PZS
999999
F280045PZSR
MSP430F6726IPNR
TMS320F28069UPZPS
TM4C123GH6PMI7
    "#;

    // 循环添加产品到队列
    for name in product_names.lines() {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        info!("添加产品：{}", name);
        let product = Product::new_with_rank(name.to_owned(), 0);
        products.lock().await.push_back(product);
    }

    // 监听127.0.0.1:8000
    let listener = TcpListener::bind("0.0.0.0:8000").await.unwrap();
    // 循环接收连接

    loop {
        // 接收请求
        let (stream, addr) = listener.accept().await.unwrap();

        let products = Arc::clone(&products);
        let tasks = Arc::clone(&tasks);

        tokio::spawn(async move {
            handle_connection(stream, addr, products, tasks).await;
        });
    }
}
