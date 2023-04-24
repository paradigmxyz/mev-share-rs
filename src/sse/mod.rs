//! Server-sent events (SSE) support.

use async_sse::Decoder;
use bytes::Bytes;
use futures_util::{
    stream::{IntoAsyncRead, MapErr, MapOk},
    Stream, TryFutureExt, TryStreamExt,
};
use pin_project_lite::pin_project;
use reqwest::header::{self, HeaderMap, HeaderValue};
use std::{
    future::Future,
    io,
    pin::Pin,
    task::{ready, Context, Poll},
    time::Duration,
};
use tracing::{debug, trace, warn};

mod types;
pub use types::*;

type TryIo = fn(reqwest::Error) -> io::Error;
type TryOk = fn(async_sse::Event) -> serde_json::Result<EventOrRetry>;
type ReqStream = Pin<Box<dyn Stream<Item = reqwest::Result<Bytes>>>>;
type SseDecoderStream = MapOk<Decoder<IntoAsyncRead<MapErr<ReqStream, TryIo>>>, TryOk>;

/// The client for SSE.
///
/// This is a simple wrapper around [reqwest::Client].
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

    /// Subscribe to the MEV-share SSE endpoint.
    ///
    /// This connects to the endpoint and returns a stream of events.
    pub async fn subscribe(&self, endpoint: &str) -> reqwest::Result<EventStream> {
        let st = new_stream(&self.client, endpoint).await?;

        let endpoint = endpoint.to_string();
        let inner = EventStreamInner { num_retries: 0, endpoint, client: self.clone() };
        let st = EventStream { inner, state: Some(State::Active(Box::pin(st))) };

        Ok(st)
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

/// A stream of SSE events.
#[must_use = "streams do nothing unless polled"]
pub struct EventStream {
    inner: EventStreamInner,
    /// State the stream is in
    state: Option<State>,
}

// === impl EventStream ===

impl EventStream {
    /// The endpoint this stream is connected to.
    pub fn endpoint(&self) -> &str {
        &self.inner.endpoint
    }

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

    /// Resets all retry attempts
    pub fn reset_retries(&mut self) {
        self.inner.num_retries = 0;
    }
}

impl Stream for EventStream {
    type Item = Result<Event, SseError>;

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
                            return Poll::Ready(None)
                        }
                        Poll::Ready(Some(Ok(maybe_event))) => match maybe_event {
                            EventOrRetry::Event(event) => {
                                trace!(
                                    message = serde_json::to_string(&event).unwrap(),
                                    "received event"
                                );
                                res = Poll::Ready(Some(Ok(event)));
                            }
                            EventOrRetry::Retry(duration) => {
                                this.inner.num_retries += 1;
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

impl std::fmt::Debug for EventStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventStream")
            .field("endpoint", &self.inner.endpoint)
            .field("num_retries", &self.inner.num_retries)
            .field("client", &self.inner.client.client)
            .finish_non_exhaustive()
    }
}

enum State {
    End,
    Retry(Pin<Box<dyn Future<Output = Result<ActiveEventStream, SseError>>>>),
    Active(Pin<Box<ActiveEventStream>>),
}

#[derive(Clone)]
struct EventStreamInner {
    num_retries: u64,
    endpoint: String,
    client: EventClient,
}

// === impl EventStreamInner ===

impl EventStreamInner {
    /// Create a new subscription stream.
    async fn retry(&mut self) -> Result<ActiveEventStream, SseError> {
        self.num_retries += 1;
        if let Some(max_retries) = self.client.max_retries {
            if self.num_retries > max_retries {
                return Err(SseError::MaxRetriesExceeded(max_retries))
            }
        }
        debug!(retries = self.num_retries, "retrying SSE stream");
        new_stream(&self.client.client, &self.endpoint).map_err(SseError::RetryError).await
    }
}

pin_project! {
    /// A stream of SSE events.
    struct ActiveEventStream {
        #[pin]
        st: SseDecoderStream
    }
}

impl Stream for ActiveEventStream {
    type Item = Result<EventOrRetry, SseError>;

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

async fn new_stream(
    client: &reqwest::Client,
    endpoint: &str,
) -> reqwest::Result<ActiveEventStream> {
    let resp = client.get(endpoint).send().await?;
    let map_io_err: TryIo = |e| io::Error::new(io::ErrorKind::Other, e);
    let o: TryOk = |e| match e {
        async_sse::Event::Message(m) => {
            serde_json::from_slice::<Event>(m.data()).map(EventOrRetry::Event)
        }
        async_sse::Event::Retry(duration) => Ok(EventOrRetry::Retry(duration)),
    };
    let event_stream: ReqStream = Box::pin(resp.bytes_stream());
    let st = async_sse::decode(event_stream.map_err(map_io_err).into_async_read()).map_ok(o);
    Ok(ActiveEventStream { st })
}

enum EventOrRetry {
    Retry(Duration),
    Event(Event),
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
