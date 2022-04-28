use ti_protocol::{get_header_size, PackType, PacketHeader, Task, TaskResult, TiUnPack};
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
};

async fn handle_connection(mut stream: TcpStream) {
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

        // 根据包头长度读取数据
        let mut body = vec![0; header.body_size as usize];
        stream.read(&mut body).await.unwrap();

        // 根据数据类型解析数据
        match header.pack_type {
            PackType::Task => {
                let task = Task::unpack(&body).unwrap();
                println!("{:?}", task);
            }
            PackType::TaskResult => {
                let task = TaskResult::unpack(&body).unwrap();
                println!("{:?}", task);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    // 监听127.0.0.1:8000
    let listener = TcpListener::bind("0.0.0.0:8000").await.unwrap();
    // 循环接收连接

    loop {
        // 接收请求
        let (stream, _) = listener.accept().await.unwrap();
        // 处理
        tokio::spawn(async move {
            handle_connection(stream).await;
        });
    }
}
