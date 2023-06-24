//! Server-sent events (SSE) support.

use crate::types::Event;
use async_sse::Decoder;
use bytes::Bytes;
use futures_util::stream::{IntoAsyncRead, MapErr, MapOk};
use futures_util::{Stream, TryFutureExt, TryStreamExt};
use pin_project_lite::pin_project;
use reqwest::header::{self, HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{ready, Context, Poll};
use std::time::Duration;
use tracing::{debug, trace, warn};

type TryIo = fn(reqwest::Error) -> io::Error;
type TryOk<T> = fn(async_sse::Event) -> serde_json::Result<EventOrRetry<T>>;
type ReqStream = Pin<Box<dyn Stream<Item = reqwest::Result<Bytes>>>>;
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
    ///
    /// ```
    ///
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            max_retries: None,
        }
    }

    /// Subscribe to the MEV-share SSE endpoint.
    ///
    /// This connects to the endpoint and returns a stream of `T` items.
    ///
    /// See [EventClient::events] for a more convenient way to subscribe to [Event] streams.
    pub async fn subscribe<T: DeserializeOwned>(
        &self,
        endpoint: &str,
    ) -> reqwest::Result<SseEventStream<T>> {
        let st = new_stream(&self.client, endpoint).await?;

        let endpoint = endpoint.to_string();
        let inner = SseEventStreamInner {
            num_retries: 0,
            endpoint,
            client: self.clone(),
        };
        let st = SseEventStream {
            inner,
            state: Some(State::Active(Box::pin(st))),
        };

        Ok(st)
    }

    /// Subscribe to a a stream of [Event]s.
    ///
    /// This is a convenience function for [EventClient::subscribe].
    pub async fn events(&self, endpoint: &str) -> reqwest::Result<SseEventStream<Event>> {
        self.subscribe(endpoint).await
    }
}

impl Default for EventClient {
    fn default() -> Self {
        Self::new(
            reqwest::Client::builder()
                .default_headers(HeaderMap::from_iter([
                    (
                        header::ACCEPT,
                        HeaderValue::from_static("text/event-stream"),
                    ),
                    (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
                ]))
                .build()
                .expect("Reqwest client build failed, TLS backend not available?"),
        )
    }
}

/// A stream of SSE items
#[must_use = "streams do nothing unless polled"]
pub struct SseEventStream<T> {
    inner: SseEventStreamInner,
    /// State the stream is in
    state: Option<State<T>>,
}

// === impl SseEventStream ===

impl<T> SseEventStream<T> {
    /// The endpoint this stream is connected to.
    pub fn endpoint(&self) -> &str {
        &self.inner.endpoint
    }

    /// Resets all retry attempts
    pub fn reset_retries(&mut self) {
        self.inner.num_retries = 0;
    }
}

impl<T: DeserializeOwned> SseEventStream<T> {
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

impl<T: DeserializeOwned> Stream for SseEventStream<T> {
    type Item = Result<T, SseError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let mut res = Poll::Pending;

        loop {
            match this
                .state
                .take()
                .expect("SseEventStream polled after completion")
            {
                State::End => return Poll::Ready(None),
                State::Retry(mut fut) => match fut.as_mut().poll(cx) {
                    Poll::Ready(Ok(st)) => {
                        this.state = Some(State::Active(Box::pin(st)));
                        continue;
                    }
                    Poll::Ready(Err(err)) => {
                        this.state = Some(State::End);
                        return Poll::Ready(Some(Err(err)));
                    }
                    Poll::Pending => {
                        this.state = Some(State::Retry(fut));
                        return Poll::Pending;
                    }
                },
                State::Active(mut st) => {
                    // Active state
                    match st.as_mut().poll_next(cx) {
                        Poll::Ready(None) => {
                            this.state = Some(State::End);
                            return Poll::Ready(None);
                        }
                        Poll::Ready(Some(Ok(maybe_event))) => match maybe_event {
                            EventOrRetry::Event(event) => {
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
                                continue;
                            }
                        },
                        Poll::Ready(Some(Err(err))) => {
                            warn!(?err, "active stream error");
                            res = Poll::Ready(Some(Err(err)));
                        }
                        Poll::Pending => {}
                    }
                    this.state = Some(State::Active(st));
                    break;
                }
            }
        }

        res
    }
}

impl<T> std::fmt::Debug for SseEventStream<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SseEventStream")
            .field("endpoint", &self.inner.endpoint)
            .field("num_retries", &self.inner.num_retries)
            .field("client", &self.inner.client.client)
            .finish_non_exhaustive()
    }
}

enum State<T> {
    End,
    Retry(Pin<Box<dyn Future<Output = Result<ActiveSseEventStream<T>, SseError>>>>),
    Active(Pin<Box<ActiveSseEventStream<T>>>),
}

#[derive(Clone)]
struct SseEventStreamInner {
    num_retries: u64,
    endpoint: String,
    client: EventClient,
}

// === impl SseEventStreamInner ===

impl SseEventStreamInner {
    /// Create a new subscription stream.
    async fn retry<T: DeserializeOwned>(&mut self) -> Result<ActiveSseEventStream<T>, SseError> {
        self.num_retries += 1;
        if let Some(max_retries) = self.client.max_retries {
            if self.num_retries > max_retries {
                return Err(SseError::MaxRetriesExceeded(max_retries));
            }
        }
        debug!(retries = self.num_retries, "retrying SSE stream");
        new_stream(&self.client.client, &self.endpoint)
            .map_err(SseError::RetryError)
            .await
    }
}

pin_project! {
    /// A stream of SSE events.
    struct ActiveSseEventStream<T> {
        #[pin]
        st: SseDecoderStream<T>
    }
}

impl<T: DeserializeOwned> Stream for ActiveSseEventStream<T> {
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

async fn new_stream<T: DeserializeOwned>(
    client: &reqwest::Client,
    endpoint: &str,
) -> reqwest::Result<ActiveSseEventStream<T>> {
    let resp = client.get(endpoint).send().await?;
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
    Ok(ActiveSseEventStream { st })
}

enum EventOrRetry<T> {
    Retry(Duration),
    Event(T),
}

/// Error variants that can occur while handling a SSE subscription,
#[derive(Debug, thiserror::Error)]
pub enum SseError {
    /// Failed to deserialize the SSE event data.
    #[error("Failed to serialize serde JSON object")]
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
