use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

type Tx = tokio::sync::mpsc::UnboundedSender<String>;
type Rx = tokio::sync::mpsc::UnboundedReceiver<String>;

struct Shared {
    peers: HashMap<SocketAddr, (Tx, String)>,
}


async fn handle_whisper(state: &Mutex<Shared>, writer: &mut tokio::io::WriteHalf<TcpStream>, parts: &[&str], addr: &SocketAddr) {
    if parts.len() >= 3 {
        let target_nickname = parts[1];
        let message = parts[2..].join(" ");
        let mut found = false;
        let state_guard = state.lock().await;
        if let Some((_, sender_nickname)) = state_guard.peers.get(addr) {
            for (_, (tx, peer_nickname)) in state_guard.peers.iter() {
                if peer_nickname == target_nickname {
                    tx.send(format!("Whisper from {}: {}", sender_nickname, message)).unwrap();
                    found = true;
                    break;
                }
            }
        }
        drop(state_guard);
        if !found {
            writer.write_all(b"User not found\n").await.unwrap();
        }
    } else {
        writer.write_all(b"Invalid whisper command\n").await.unwrap();
    }
}

async fn handle_list(state: &Mutex<Shared>, writer: &mut tokio::io::WriteHalf<TcpStream>) {
    let mut list = String::new();
    for (_, (_, nickname)) in state.lock().await.peers.iter() {
        list.push_str(&format!("{}\n", nickname));
    }
    writer.write_all(list.as_bytes()).await.unwrap();
}

async fn handle_nickname(state: &Mutex<Shared>, writer: &mut tokio::io::WriteHalf<TcpStream>, addr: SocketAddr, parts: &[&str]) {
    if parts.len() == 2 {
        let new_nickname = parts[1].to_string();
        let mut state_guard = state.lock().await;
        if let Some((tx, old_nickname)) = state_guard.peers.get_mut(&addr) {
            let old_nickname_clone = old_nickname.clone();
            *old_nickname = new_nickname.clone();
            writer.write_all(b"Nickname changed\n").await.unwrap();
            tx.send(format!("Your nickname is now {}", new_nickname)).unwrap();
            drop(state_guard);
            broadcast_nickname_change(state, &old_nickname_clone, &new_nickname).await;
        }
    } else {
        writer.write_all(b"Invalid nickname command\n").await.unwrap();
    }
}


async fn handle_message(state: &Mutex<Shared>, nickname: &str, msg: &str) {
    let msg = format!("{}: {}", nickname, msg);
    broadcast_message(state, &msg).await;
}

async fn broadcast_connect(state: &Mutex<Shared>, nickname: &str) {
    let msg = format!("{} has connected", nickname);
    broadcast_message(state, &msg).await;
}

async fn broadcast_disconnect(state: &Mutex<Shared>, nickname: &str) {
    let msg = format!("{} has disconnected", nickname);
    broadcast_message(state, &msg).await;
}

async fn broadcast_nickname_change(state: &Mutex<Shared>, old_nickname: &str, new_nickname: &str) {
    let msg = format!("{} changed their nickname to {}", old_nickname, new_nickname);
    broadcast_message(state, &msg).await;
}

async fn broadcast_message(state: &Mutex<Shared>, msg: &str) {
    state.lock().await.peers.iter().for_each(|(_, (tx, _))| {
        tx.send(msg.to_string()).unwrap();
    });
}

async fn handle_connection(stream: TcpStream, addr: SocketAddr, state: Arc<Mutex<Shared>>) {
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    let nickname = format!("Client_{}", addr.port());
    state.lock().await.peers.insert(addr, (tx.clone(), nickname.clone()));

    // Broadcast the connection message to other clients
    broadcast_connect(&state, &nickname).await;

    let mut line = String::new();
    loop {
        tokio::select! {
            result = reader.read_line(&mut line) => {
                if result.unwrap() == 0 {
                    break;
                }
                let msg = line.trim();
                if msg.starts_with("/whisper") {
                    let parts: Vec<&str> = msg.split_whitespace().collect();
                    handle_whisper(&state, &mut writer, &parts, &addr).await;
                } else if msg == "/list" {
                    handle_list(&state, &mut writer).await;
                } else if msg.starts_with("/nickname") {
                    let parts: Vec<&str> = msg.split_whitespace().collect();
                    handle_nickname(&state, &mut writer, addr, &parts).await;
                } else {
                    let state_guard = state.lock().await;
                    if let Some((_, nickname)) = state_guard.peers.get(&addr) {
                        let nickname = nickname.clone();
                        drop(state_guard);
                        handle_message(&state, &nickname, msg).await;
                    }
                }
                line.clear();
            }
            
            Some(msg) = rx.recv() => {
                writer.write_all(msg.as_bytes()).await.unwrap();
                writer.write_all(b"\n").await.unwrap();
            }
        }
    }

    // Remove the disconnected client from the peers map
    let nickname = state.lock().await.peers.remove(&addr).unwrap().1;

    // Broadcast the disconnection message to other clients
    broadcast_disconnect(&state, &nickname).await;
}

#[tokio::main]
async fn main() {
    let addr = "127.0.0.1:4000".to_string();
    let listener = TcpListener::bind(&addr).await.unwrap();
    println!("Server listening on: {}", addr);

    let state = Arc::new(Mutex::new(Shared {
        peers: HashMap::new(),
    }));

    loop {
        let (stream, addr) = listener.accept().await.unwrap();
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            handle_connection(stream, addr, state).await;
        });
    }
}