use rust_eventbus::client::EventBusClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut client = EventBusClient::connect("ws://localhost:8080/ws").await?;
    println!("Connected to server");
    
    client.subscribe("test_topic", None).await?;
    println!("Subscribed to test_topic");
    
    client.publish("test_topic", b"{\"data\":\"test payload\"}", None).await?;
    println!("Published message");

    while let Some(msg) = client.next_message().await {
        match msg {
            Ok(message) => {
                let _ = client.acknowledge(message.seq, message.message_id).await;    
                println!("Received: {:?}", message);
            },
            Err(e) => eprintln!("Error: {}", e),
        }
    }
    
    Ok(())
}