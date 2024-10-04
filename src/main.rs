use std::sync::Arc;
use tokio::sync::Mutex;
use warp::ws::{Message, WebSocket};
use warp::Filter;
use futures::{StreamExt, SinkExt};
use serde::{Deserialize, Serialize};
use warp::http::Response;

type Users = Arc<Mutex<Vec<tokio::sync::mpsc::UnboundedSender<Message>>>>;
 
#[derive(Serialize, Deserialize, Debug)]
struct ChatMessage {
    username: String,
    content: String,
}
 
#[tokio::main]
async fn main() {
    let users: Users = Arc::new(Mutex::new(Vec::new()));
 
    let users_filter = warp::any().map(move || Arc::clone(&users));
 
    let websocket_route = warp::path("ws")
        .and(warp::ws())
        .and(users_filter)
        .map(|ws: warp::ws::Ws, users| {
            ws.on_upgrade(move |socket| handle_connection(socket, users))
        });
 
    let index_route = warp::path::end()
        .map(|| {
            Response::builder()
                .header("content-type", "text/html")
                .body(include_str!("templates/index.html"))
        });
 
    let routes = warp::get().and(websocket_route.or(index_route));
 
    println!("Server running on http://127.0.0.1:3030");
 
    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}
 
async fn handle_connection(ws: WebSocket, users: Users) {
    let (mut sender, mut receiver) = ws.split();
 
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    {
        let mut users = users.lock().await;
        users.push(tx);
    }
 
    tokio::task::spawn(async move {
        let mut rx = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
        while let Some(msg) = rx.next().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });
 
    while let Some(result) = receiver.next().await {
        match result {
            Ok(msg) => {
                if let Ok(text) = msg.to_str() {
                    if let Ok(chat_message) = serde_json::from_str::<ChatMessage>(text) {
                        broadcast_message(&chat_message, &users).await;
                    }
                }
            }
            Err(e) => {
                eprintln!("error: {}", e);
                break;
            }
        }
    }
}
 
async fn broadcast_message(message: &ChatMessage, users: &Users) {
    let message_str = serde_json::to_string(message).unwrap();
    let msg = Message::text(message_str);
 
    let users = users.lock().await;
    for user in users.iter() {
        if let Err(_disconnected) = user.send(msg.clone()) {
            println!("User disconnected");
        }
    }
}