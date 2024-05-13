use reqwest::{Client as HttpClient, Method};
use serde::{de::DeserializeOwned, Serialize};

use crate::{
    client_options::ClientOptions,
    event::{Event, InnerEvent, InnerEventBatch},
    prelude::*,
};

pub struct Client {
    options: ClientOptions,
    http_client: HttpClient,
}

impl Client {
    pub(crate) fn new(options: ClientOptions) -> Self {
        let http_client = HttpClient::builder()
            .timeout(options.timeout)
            .build()
            .unwrap(); // Unwrap here is as safe as `HttpClient::new`
        
        Client {
            options,
            http_client,
        }
    }

    async fn send_request<P: AsRef<str>, Body: Serialize, Res: DeserializeOwned>(
        &self,
        method: Method,
        path: P,
        body: &Body,
    ) -> Result<Res> {
        let res = self
            .http_client
            .request(
                method,
                format!("{}{}", self.options.api_endpoint, path.as_ref()),
            )
            .json(body)
            .send()
            .await?
            .error_for_status()?
            .json::<Res>()
            .await?;

        Ok(res)
    }

    pub async fn capture(&self, event: Event) -> Result<()> {
        let inner_event = InnerEvent::new(event, self.options.api_key.clone());
        self.send_request::<_, _, serde_json::Value>(Method::POST, "/capture/", &inner_event)
            .await?;
        Ok(())
    }

    pub async fn capture_batch(&self, events: Vec<Event>) -> Result<()> {
        let inner_event_batch = InnerEventBatch::new(events, self.options.api_key.clone());
        self.send_request::<_, _, serde_json::Value>(Method::POST, "/batch/", &inner_event_batch)
            .await?;
        Ok(())
    }
}
