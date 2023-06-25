//! Server-sent events (SSE) support.

use async_sse::Decoder;
use bytes::Bytes;
use futures_util::{
    stream::{IntoAsyncRead, MapErr, MapOk},
    Stream, TryFutureExt, TryStreamExt,
};
use pin_project_lite::pin_project;
use reqwest::header::{self, HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use std::{
    future::Future,
    io,
    pin::Pin,
    task::{ready, Context, Poll},
    time::Duration,
};

use serde::Serialize;
use tracing::{debug, trace, warn};

mod types;
pub use types::*;

type TryIo = fn(reqwest::Error) -> io::Error;
type TryOk<T> = fn(async_sse::Event) -> serde_json::Result<EventOrRetry<T>>;
type ReqStream = Pin<Box<dyn Stream<Item = reqwest::Result<Bytes>> + Send>>;
type SseDecoderStream<T> = MapOk<Decoder<IntoAsyncRead<MapErr<ReqStream, TryIo>>>, TryOk<T>>;

/// The client for SSE.
///
/// This is a simple wrapper around [reqwest::Client] that provides subscription function for SSE.
#[derive(Debug, Clone)]
pub struct EventClient {
    client: reqwest::Client,
    max_retries: Option<u64>,
}

// === impl EventClient ===

impl EventClient {
    /// Create a new client with the given reqwest client.
    ///
    /// ```
    /// use mev_share_rs::EventClient;
    /// let client = EventClient::new(reqwest::Client::new());
    /// ```
    pub fn new(client: reqwest::Client) -> Self {
        Self { client, max_retries: None }
    }

    /// Set the maximum number of retries.
    pub fn with_max_retries(mut self, max_retries: u64) -> Self {
        self.max_retries = Some(max_retries);
        self
    }

    /// Subscribe to the MEV-share SSE endpoint.
    ///
    /// This connects to the endpoint and returns a stream of `T` items.
    ///
    /// See [EventClient::events] for a more convenient way to subscribe to [Event] streams.
    pub async fn subscribe<T: DeserializeOwned>(
        &self,
        endpoint: &str,
    ) -> reqwest::Result<EventStream<T>> {
        let st = new_stream(&self.client, endpoint, None::<()>).await?;

        let endpoint = endpoint.to_string();
        let inner =
            EventStreamInner { num_retries: 0, endpoint, client: self.clone(), query: None };
        let st = EventStream { inner, state: Some(State::Active(Box::pin(st))) };

        Ok(st)
    }

    /// Subscribe to the MEV-share SSE endpoint with additional query params.
    ///
    /// This connects to the endpoint and returns a stream of `T` items.
    ///
    /// See [EventClient::events] for a more convenient way to subscribe to [Event] streams.
    pub async fn subscribe_with_query<T: DeserializeOwned, S: Serialize>(
        &self,
        endpoint: &str,
        query: S,
    ) -> reqwest::Result<EventStream<T>> {
        let query = Some(serde_json::to_value(query).expect("serialization failed"));
        let st = new_stream(&self.client, endpoint, query.as_ref()).await?;

        let endpoint = endpoint.to_string();
        let inner = EventStreamInner { num_retries: 0, endpoint, client: self.clone(), query };
        let st = EventStream { inner, state: Some(State::Active(Box::pin(st))) };

        Ok(st)
    }

    /// Subscribe to a a stream of [Event]s.
    ///
    /// This is a convenience function for [EventClient::subscribe].
    ///
    /// # Example
    ///
    /// ```
    /// use futures_util::StreamExt;
    /// use mev_share_rs::EventClient;
    /// # async fn demo() {
    ///   let client = EventClient::default();
    ///   let mut stream = client.events("https://mev-share.flashbots.net").await.unwrap();
    ///   while let Some(event) = stream.next().await {
    ///    dbg!(&event);
    ///   }
    /// # }
    /// ```
    pub async fn events(&self, endpoint: &str) -> reqwest::Result<EventStream<Event>> {
        self.subscribe(endpoint).await
    }

    /// Gets past events that were broadcast via the SSE event stream.
    ///
    /// Such as `https://mev-share.flashbots.net/api/v1/history`.
    ///
    /// # Example
    ///
    /// ```
    /// use mev_share_rs::EventClient;
    /// use mev_share_rs::sse::EventHistoryParams;
    /// # async fn demo() {
    ///   let client = EventClient::default();
    ///   let params = EventHistoryParams::default();
    ///   let history = client.event_history("https://mev-share.flashbots.net/api/v1/history", params).await.unwrap();
    ///   dbg!(&history);
    /// # }
    /// ```
    pub async fn event_history(
        &self,
        endpoint: &str,
        params: EventHistoryParams,
    ) -> reqwest::Result<Vec<EventHistory>> {
        self.client.get(endpoint).query(&params).send().await?.json().await
    }

    /// Gets information about the event history endpoint
    ///
    /// Such as `https://mev-share.flashbots.net/api/v1/history/info`.
    ///
    /// # Example
    ///
    /// ```
    /// use mev_share_rs::EventClient;
    /// # async fn demo() {
    ///   let client = EventClient::default();
    ///   let info = client.event_history_info("https://mev-share.flashbots.net/api/v1/history/info").await.unwrap();
    ///   dbg!(&info);
    /// # }
    /// ```
    pub async fn event_history_info(&self, endpoint: &str) -> reqwest::Result<EventHistoryInfo> {
        self.get_json(endpoint).await
    }

    async fn get_json<T: DeserializeOwned>(&self, endpoint: &str) -> reqwest::Result<T> {
        self.client.get(endpoint).send().await?.json().await
    }
}

impl Default for EventClient {
    fn default() -> Self {
        Self::new(
            reqwest::Client::builder()
                .default_headers(HeaderMap::from_iter([
                    (header::ACCEPT, HeaderValue::from_static("text/event-stream")),
                    (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
                ]))
                .build()
                .expect("Reqwest client build failed, TLS backend not available?"),
        )
    }
}

/// A stream of SSE items
#[must_use = "streams do nothing unless polled"]
pub struct EventStream<T> {
    inner: EventStreamInner,
    /// State the stream is in
    state: Option<State<T>>,
}

// === impl EventStream ===

impl<T> EventStream<T> {
    /// The endpoint this stream is connected to.
    pub fn endpoint(&self) -> &str {
        &self.inner.endpoint
    }

    /// Resets all retry attempts
    pub fn reset_retries(&mut self) {
        self.inner.num_retries = 0;
    }
}

impl<T: DeserializeOwned> EventStream<T> {
    /// Retries the stream by establishing a new connection.
    pub async fn retry(&mut self) -> Result<(), SseError> {
        let st = self.inner.retry().await?;
        self.state = Some(State::Active(Box::pin(st)));
        Ok(())
    }

    /// Retries the stream by establishing a new connection using the given endpoint.
    pub async fn retry_with(&mut self, endpoint: impl Into<String>) -> Result<(), SseError> {
        self.inner.endpoint = endpoint.into();
        let st = self.inner.retry().await?;
        self.state = Some(State::Active(Box::pin(st)));
        Ok(())
    }
}

impl<T: DeserializeOwned> Stream for EventStream<T> {
    type Item = Result<T, SseError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let mut res = Poll::Pending;

        loop {
            match this.state.take().expect("EventStream polled after completion") {
                State::End => return Poll::Ready(None),
                State::Retry(mut fut) => match fut.as_mut().poll(cx) {
                    Poll::Ready(Ok(st)) => {
                        this.state = Some(State::Active(Box::pin(st)));
                        continue
                    }
                    Poll::Ready(Err(err)) => {
                        this.state = Some(State::End);
                        return Poll::Ready(Some(Err(err)))
                    }
                    Poll::Pending => {
                        this.state = Some(State::Retry(fut));
                        return Poll::Pending
                    }
                },
                State::Active(mut st) => {
                    // Active state
                    match st.as_mut().poll_next(cx) {
                        Poll::Ready(None) => {
                            this.state = Some(State::End);
                            debug!("stream finished");
                            return Poll::Ready(None)
                        }
                        Poll::Ready(Some(Ok(maybe_event))) => match maybe_event {
                            EventOrRetry::Event(event) => {
                                res = Poll::Ready(Some(Ok(event)));
                            }
                            EventOrRetry::Retry(duration) => {
                                let mut client = this.inner.clone();
                                let fut = Box::pin(async move {
                                    tokio::time::sleep(duration).await;
                                    client.retry().await
                                });
                                this.state = Some(State::Retry(fut));
                                continue
                            }
                        },
                        Poll::Ready(Some(Err(err))) => {
                            warn!(?err, "active stream error");
                            res = Poll::Ready(Some(Err(err)));
                        }
                        Poll::Pending => {}
                    }
                    this.state = Some(State::Active(st));
                    break
                }
            }
        }

        res
    }
}

impl<T> std::fmt::Debug for EventStream<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventStream")
            .field("endpoint", &self.inner.endpoint)
            .field("num_retries", &self.inner.num_retries)
            .field("client", &self.inner.client.client)
            .finish_non_exhaustive()
    }
}

enum State<T> {
    End,
    Retry(Pin<Box<dyn Future<Output = Result<ActiveEventStream<T>, SseError>> + Send>>),
    Active(Pin<Box<ActiveEventStream<T>>>),
}

#[derive(Clone)]
struct EventStreamInner {
    num_retries: u64,
    endpoint: String,
    client: EventClient,
    query: Option<serde_json::Value>,
}

// === impl EventStreamInner ===

impl EventStreamInner {
    /// Create a new subscription stream.
    async fn retry<T: DeserializeOwned>(&mut self) -> Result<ActiveEventStream<T>, SseError> {
        self.num_retries += 1;
        if let Some(max_retries) = self.client.max_retries {
            if self.num_retries > max_retries {
                return Err(SseError::MaxRetriesExceeded(max_retries))
            }
        }
        debug!(retries = self.num_retries, "retrying SSE stream");
        new_stream(&self.client.client, &self.endpoint, self.query.as_ref())
            .map_err(SseError::RetryError)
            .await
    }
}

pin_project! {
    /// A stream of SSE events.
    struct ActiveEventStream<T> {
        #[pin]
        st: SseDecoderStream<T>
    }
}

impl<T: DeserializeOwned> Stream for ActiveEventStream<T> {
    type Item = Result<EventOrRetry<T>, SseError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        match ready!(this.st.poll_next(cx)) {
            None => {
                // Stream ended
                Poll::Ready(None)
            }
            Some(res) => {
                // Stream has a new event
                let item = match res {
                    Ok(Ok(e)) => Ok(e),
                    Ok(Err(e)) => Err(SseError::SerdeJsonError(e)),
                    Err(e) => Err(SseError::Http(e)),
                };
                Poll::Ready(Some(item))
            }
        }
    }
}

async fn new_stream<T: DeserializeOwned, S: Serialize>(
    client: &reqwest::Client,
    endpoint: &str,
    query: Option<S>,
) -> reqwest::Result<ActiveEventStream<T>> {
    let mut builder = client.get(endpoint);
    if let Some(query) = query {
        builder = builder.query(&query);
    }
    let resp = builder.send().await?;
    let map_io_err: TryIo = |e| io::Error::new(io::ErrorKind::Other, e);
    let o: TryOk<_> = |e| match e {
        async_sse::Event::Message(msg) => {
            trace!(
                message = ?String::from_utf8_lossy(msg.data()),
                "received message"
            );
            serde_json::from_slice::<T>(msg.data()).map(EventOrRetry::Event)
        }
        async_sse::Event::Retry(duration) => Ok(EventOrRetry::Retry(duration)),
    };
    let event_stream: ReqStream = Box::pin(resp.bytes_stream());
    let st = async_sse::decode(event_stream.map_err(map_io_err).into_async_read()).map_ok(o);
    Ok(ActiveEventStream { st })
}

enum EventOrRetry<T> {
    Retry(Duration),
    Event(T),
}

/// Error variants that can occur while handling an SSE subscription,
#[derive(Debug, thiserror::Error)]
pub enum SseError {
    /// Failed to deserialize the SSE event data.
    #[error("Failed to deserialize event: {0}")]
    SerdeJsonError(serde_json::Error),
    /// Connection related error
    #[error("{0}")]
    Http(http_types::Error),
    /// Request related error
    #[error("Failed to establish a retry connection: {0}")]
    RetryError(reqwest::Error),
    /// Request related error
    #[error("Exceeded all retries: {0}")]
    MaxRetriesExceeded(u64),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    const HISTORY_V1: &str = "https://mev-share.flashbots.net/api/v1/history";
    const HISTORY_INFO_V1: &str = "https://mev-share.flashbots.net/api/v1/history/info";

    fn init_tracing() {
        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(EnvFilter::from_default_env())
            .init();
    }

    #[tokio::test]
    #[ignore]
    async fn get_event_history_info() {
        init_tracing();
        let client = EventClient::default();
        let _info = client.event_history_info(HISTORY_INFO_V1).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn get_event_history() {
        init_tracing();
        let client = EventClient::default();
        let _history = client.event_history(HISTORY_V1, Default::default()).await.unwrap();
    }
}
