use std::time::Duration;

use chrono::NaiveDateTime;
use lazy_static::lazy_static;
use tokio::sync::RwLock;
use tracing::{error, trace};

use crate::prelude::*;
pub use crate::{client::Client, client_options::ClientOptions, event::Event};

mod client;
mod client_options;
mod event;
mod prelude;

lazy_static! {
    static ref QUEUE: RwLock<Vec<(String, Vec<(String, String)>, NaiveDateTime)>> =
        RwLock::new(Vec::new());
}

pub fn load(distinct_id: String) -> Result<()> {
    dotenv::dotenv()?;
    let post_hog_api_key = std::env::var("POSTHOG_API_KEY")?;

    let mut options = ClientOptions::new(post_hog_api_key);
    if let Ok(post_hog_api_endpoint) = std::env::var("POSTHOG_API_ENDPOINT") {
        if !post_hog_api_endpoint.is_empty() {
            options.api_endpoint(post_hog_api_endpoint);
        }
    }

    if let Ok(post_hog_timeout) = std::env::var("POSTHOG_TIMEOUT") {
        if !post_hog_timeout.is_empty() {
            options.timeout(Duration::from_millis(post_hog_timeout.parse()?));
        }
    }

    let client = options.build();
    let queue_retry_delay = std::env::var("POSTHOG_QUEUE_RETRY_DELAY")
        .unwrap_or_else(|_| "4".to_string())
        .parse()
        .unwrap();

    tokio::spawn(async move {
        start_posthog_queue_handler(client, distinct_id, queue_retry_delay).await;
    });

    Ok(())
}

pub fn submit_event_to_queue(event_name: String, properties: Vec<(String, String)>) {
    tokio::spawn(async move {
        QUEUE
            .write()
            .await
            .push((event_name, properties, chrono::Utc::now().naive_utc()))
    });
}

pub async fn start_posthog_queue_handler(
    client: Client,
    distinct_id: String,
    queue_retry_delay: u64,
) {
    loop {
        let mut queue = QUEUE.write().await;

        if queue.is_empty() {
            continue;
        }

        let events: Vec<Event> = queue
            .drain(..)
            .collect::<Vec<(String, Vec<(String, String)>, NaiveDateTime)>>()
            .iter()
            .map(|(event_name, properties, timestamp)| {
                let mut event = Event::new(event_name, &distinct_id);

                for property in properties {
                    match event.insert_prop(&property.0, &property.1) {
                        Ok(_) => (),
                        Err(e) => {
                            error!("Failed to insert property into event: {:?}", e.to_string());
                        }
                    }
                }

                event.timestamp(*timestamp);

                event
            })
            .collect();

        trace!("Sending {} events to PostHog", events.len());
        loop {
            match client.capture_batch(events.clone()).await {
                Ok(_) => {
                    trace!("Successfully submitted events to PostHog");
                    break;
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to submit events to PostHog: {:?}! Retrying in {} seconds...",
                        e.to_string(),
                        queue_retry_delay
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(queue_retry_delay)).await;
                }
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load() {
        assert!(load("test".to_string()).is_ok())
    }
}
