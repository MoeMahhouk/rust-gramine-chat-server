use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

async fn read_messages(mut stream: io::ReadHalf<TcpStream>) {
    let mut reader = io::BufReader::new(&mut stream);
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await.unwrap() == 0 {
            break;
        }
        print!("{}", line);
    }
}

async fn write_messages(mut stream: io::WriteHalf<TcpStream>) {
    let mut stdin = io::BufReader::new(io::stdin());
    loop {
        let mut line = String::new();
        if stdin.read_line(&mut line).await.unwrap() == 0 {
            break;
        }
        stream.write_all(line.as_bytes()).await.unwrap();
    }
}

#[tokio::main]
async fn main() {
    let server_addr = "127.0.0.1:4000";
    let stream = TcpStream::connect(server_addr).await.unwrap();
    println!("Connected to server: {}", server_addr);

    let (read_stream, write_stream) = io::split(stream);

    tokio::select! {
        _ = read_messages(read_stream) => {}
        _ = write_messages(write_stream) => {}
    }
}