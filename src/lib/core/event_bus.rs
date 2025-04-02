use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use crate::storage::{self, Storage};
use crate::core::{Message, Metadata};
use chrono::Utc;

struct PatternNode {
    children: HashMap<String, PatternNode>,
    wildcard_child: Option<Box<PatternNode>>,      // For single-level wildcard "*"
    multi_wildcard: Option<Box<PatternNode>>,      // For multi-level wildcard "#"
    channels: Vec<broadcast::Sender<Message>>,     // Subscribers for this pattern
}

impl PatternNode {
    fn new() -> Self {
        Self {
            children: HashMap::new(),
            wildcard_child: None,
            multi_wildcard: None,
            channels: Vec::new(),
        }
    }
}

pub struct EventBus<S: Storage + Send + Sync + 'static> {
    pattern_trie: Arc<RwLock<PatternNode>>,
    storage: Option<Arc<S>>,
    next_seq: Arc<RwLock<u64>>,
}

impl <S: Storage + Send + Sync + 'static>  EventBus<S> {
    pub fn new(storage: Option<Arc<S>>) -> Self {
        Self {
            pattern_trie: Arc::new(RwLock::new(PatternNode::new())),
            storage,
            next_seq: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn publish(&self, topic: &str, payload: Vec<u8>) -> anyhow::Result<()> {
        let mut seq_guard = self.next_seq.write().await;
        let seq = *seq_guard;
        *seq_guard += 1;
        drop(seq_guard);

        let msg = Message {
            seq,
            topic: topic.to_string(),
            payload,
            metadata: Metadata {
                timestamp: Utc::now().timestamp(),
                content_type: "application/json".to_string(),
            },
        };

        // Persist the message before publishing
        if let Some(storage) = &self.storage {
            storage.insert_message(&msg).await?;
        }

        // Publish to all matching subscribers
        let trie = self.pattern_trie.read().await;
        let segments = topic.split('.').collect::<Vec<_>>();
        let matching_channels = Self::collect_channels(&trie, &segments);
        for tx in matching_channels {
            let _ = tx.send(msg.clone());
        }
        Ok(())
    }

    pub async fn subscribe(&self, pattern: &str, starting_seq: Option<u64>) -> broadcast::Receiver<Message> {
        let (tx, rx) = broadcast::channel(1024);
        let mut trie = self.pattern_trie.write().await;
        Self::add_subscriber(&mut *trie, &pattern.split('.').collect::<Vec<_>>(), tx.clone());

        // Replay past messages if a starting sequence is provided
        if let Some(start_seq) = starting_seq {
            if let Some(storage) = &self.storage {
                if let Ok(messages) = storage.get_messages_after(start_seq, pattern).await {
                    for msg in messages {
                        let _ = tx.send(msg);
                    }
                }
            }
            
        }

        rx
    }

    fn add_subscriber(node: &mut PatternNode, segments: &[&str], tx: broadcast::Sender<Message>) {
        if segments.is_empty() {
            node.channels.push(tx);
            return;
        }

        let segment = segments[0];
        let remaining = &segments[1..];

        if segment == "*" {
            let wildcard = node.wildcard_child.get_or_insert_with(|| Box::new(PatternNode::new()));
            Self::add_subscriber(wildcard, remaining, tx);
        } else if segment == "#" {
            let multi = node.multi_wildcard.get_or_insert_with(|| Box::new(PatternNode::new()));
            Self::add_subscriber(multi, remaining, tx);
        } else {
            let child = node.children.entry(segment.to_string()).or_insert_with(PatternNode::new);
            Self::add_subscriber(child, remaining, tx);
        }
    }

    fn collect_channels(node: &PatternNode, segments: &[&str]) -> Vec<broadcast::Sender<Message>> {
        let mut result = vec![];

        // Collect from multi-level wildcard if present
        if let Some(multi) = &node.multi_wildcard {
            result.extend(multi.channels.clone());
        }

        if segments.is_empty() {
            result.extend(node.channels.clone());
            if let Some(hash_node) = &node.children.get("#") {
                result.extend(hash_node.channels.clone());
            }
            return result;
        }

        let segment = segments[0];
        let remaining = &segments[1..];

        // Exact match
        if let Some(child) = node.children.get(segment) {
            result.extend(Self::collect_channels(child, remaining));
        }

        // Single-level wildcard
        if let Some(wildcard) = &node.wildcard_child {
            result.extend(Self::collect_channels(wildcard, remaining));
        }

        // Multi-level wildcard continuation
        if let Some(multi) = &node.multi_wildcard {
            result.extend(Self::collect_channels(multi, remaining));
        }

        result
    }
}