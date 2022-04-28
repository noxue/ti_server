use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::Duration,
};
use ti_protocol::{get_header_size, PacketHeader, TaskResult, TiUnPack, PackType, Task};

fn handle_connection(mut stream: TcpStream) {
    let len: usize = get_header_size();
    println!("len: {}", len);
    loop {
        // thread::sleep(Duration::from_millis(10));

        let mut header = vec![0; len];
        stream.read(&mut header).unwrap();
        let header = PacketHeader::unpack(&header).unwrap();

        // 检查标志位，不对就跳过
        if !header.check_flag() {
            continue;
        }

        // 输出头长度
        println!("header body size: {}", header.body_size as usize);

        // 根据包头长度读取数据
        let mut body = vec![0; header.body_size as usize];
        stream.read(&mut body).unwrap();

        println!("body: {:?}", body);

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

fn main() {
    // 监听127.0.0.1:8000
    let listener = TcpListener::bind("0.0.0.0:8000").unwrap();
    // 循环接收连接
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("Connection established!");
                // 创建一个新的线程来处理连接
                std::thread::spawn(move || {
                    handle_connection(stream);
                });
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
    }
}
