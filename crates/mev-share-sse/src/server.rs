//! SSE server side support.

use futures_util::{Stream, StreamExt};
use http::{
    header::{CACHE_CONTROL, CONTENT_TYPE},
    StatusCode,
};
use hyper::{Body, Request, Response};
use serde::Serialize;
use std::{
    convert::Infallible,
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_util::{compat::FuturesAsyncReadCompatExt, io::ReaderStream};
use tower::{Layer, Service};
use tracing::debug;

/// A helper type that can be used to create a [`SseBroadcastLayer`].
///
/// This will broadcast serialized json messages to all subscribers.
#[derive(Clone, Debug)]
pub struct SseBroadcaster {
    /// The sender to emit broadcast messages.
    sender: broadcast::Sender<SSeBroadcastMessage>,
}

impl SseBroadcaster {
    /// Creates a new [`SseBroadcaster`] with the given sender.
    pub fn new(sender: broadcast::Sender<SSeBroadcastMessage>) -> Self {
        Self { sender }
    }

    /// Creates a new receiver that's ready to be used.
    ///
    /// This is intended as the Fn for the [SseBroadcastLayer].
    pub fn ready_stream(
        &self,
    ) -> futures_util::future::Ready<Result<SseBroadcastStream, Box<dyn Error + Send + Sync>>> {
        futures_util::future::ready(Ok(self.stream()))
    }

    /// Returns a new stream of [SSeBroadcastMessage].
    pub fn stream(&self) -> SseBroadcastStream {
        SseBroadcastStream { st: BroadcastStream::new(self.subscribe()) }
    }

    /// Creates a new Receiver handle that will receive values sent after this call to subscribe.
    pub fn subscribe(&self) -> broadcast::Receiver<SSeBroadcastMessage> {
        self.sender.subscribe()
    }

    /// Sends a message to all subscribers.
    ///
    /// See also [`Sender::send`](broadcast::Sender::send)
    pub fn send<T: Serialize>(&self, msg: &T) -> Result<usize, SseSendError> {
        let msg = SSeBroadcastMessage(Arc::from(serde_json::to_string(msg)?));
        self.sender.send(msg).map_err(|err| SseSendError::ChannelClosed(err.0.as_ref().to_string()))
    }
}

/// Error returned by [`SseBroadcastLayer`].
#[derive(Debug, thiserror::Error)]
pub enum SseSendError {
    /// Failed to serialize the message before sending.
    #[error("failed to serialize message: {0}")]
    Json(#[from] serde_json::Error),
    /// Failed to send the message.
    #[error("failed to send message because broadcast channel closed")]
    ChannelClosed(String),
}

/// Helper new type to make Arc<str> AsRef str.
///
/// Note: This is a workaround for the fact that Arc<str> does not implement AsRef<str>.
#[derive(Clone, Debug)]
pub struct SSeBroadcastMessage(Arc<str>);

impl AsRef<str> for SSeBroadcastMessage {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl fmt::Display for SSeBroadcastMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for SSeBroadcastMessage {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_ref().serialize(serializer)
    }
}

/// A Stream that emits SSE messages.
#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
pub struct SseBroadcastStream {
    st: BroadcastStream<SSeBroadcastMessage>,
}

impl Stream for SseBroadcastStream {
    type Item = SSeBroadcastMessage;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            match ready!(this.st.poll_next_unpin(cx)) {
                None => return Poll::Ready(None),
                Some(Ok(item)) => return Poll::Ready(Some(item)),
                Some(Err(err)) => {
                    debug!("broadcast stream is lagging: {err}");
                    continue
                }
            }
        }
    }
}

/// A service impl that handles SSE requests.
///
///
/// # Example
///
/// ```
/// use mev_share_sse::server::{SseBroadcaster, SseBroadcastService};
/// let (tx, _rx) = tokio::sync::broadcast::channel(1000);
/// let broadcaster = SseBroadcaster::new(tx);
/// let svc = SseBroadcastService::new(move || broadcaster.ready_stream());
/// ```
#[derive(Debug, Clone)]
pub struct SseBroadcastService<F> {
    handler: F,
}

impl<F> SseBroadcastService<F> {
    /// Creates a new [`SseBroadcastService`] with the given handler.
    pub fn new(handler: F) -> Self {
        Self { handler }
    }
}

impl<F, R, St, Item> Service<Request<Body>> for SseBroadcastService<F>
where
    F: Fn() -> R,
    R: Future<Output = Result<St, Box<dyn Error + Send + Sync>>> + Send + 'static,
    St: Stream<Item = Item> + Send + Unpin + 'static,
    Item: AsRef<str> + Send + Sync + 'static,
{
    type Response = Response<Body>;
    type Error = Box<dyn Error + Send + Sync>;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: Request<Body>) -> Self::Future {
        // acquire the receiver and perform the upgrade
        let get_receiver = (self.handler)();
        let fut = async {
            let st = match get_receiver.await {
                Ok(st) => st,
                Err(err) => {
                    return Ok(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from(err.to_string()))
                        .expect("failed to build response"))
                }
            };
            let (sender, encoder) = async_sse::encode();

            tokio::task::spawn(async move {
                let mut st = st;
                while let Some(data) = st.next().await {
                    let _ = sender.send(None, data.as_ref(), None).await;
                }
            });

            // Perform the handshake as described here:
            // <https://html.spec.whatwg.org/multipage/server-sent-events.html#sse-processing-model>
            Response::builder()
                .status(StatusCode::OK)
                .header(CACHE_CONTROL, "no-cache")
                .header(CONTENT_TYPE, "text/event-stream")
                .body(Body::wrap_stream(ReaderStream::new(encoder.compat())))
                .map_err(|err| err.into())
        };

        Box::pin(fut)
    }
}

/// Layer that can create [`SseBroadcastProxyService`].
#[derive(Clone)]
pub struct SseBroadcastLayer<F> {
    /// How to create a listener
    handler: F,
    /// The uri path to intercept
    path: Arc<String>,
}

impl<F> SseBroadcastLayer<F> {
    /// Creates a new [`SseBroadcastLayer`] with the given handler and the path to listen on.
    pub fn new(path: impl Into<String>, handler: F) -> Self {
        Self { path: Arc::new(path.into()), handler }
    }
}

impl<F, S> Layer<S> for SseBroadcastLayer<F>
where
    F: Clone,
{
    type Service = SseBroadcastProxyService<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        SseBroadcastProxyService {
            inner,
            svc: SseBroadcastService::new(self.handler.clone()),
            path: Arc::clone(&self.path),
        }
    }
}

/// A service that will stream messages into a http response.
///
/// Note: This will not set sse id's and will use the default "message" as event name.
#[derive(Debug)]
pub struct SseBroadcastProxyService<S, F> {
    path: Arc<String>,
    inner: S,
    svc: SseBroadcastService<F>,
}

impl<S, F> SseBroadcastProxyService<S, F> {
    /// Returns the path this service is listening on.
    pub fn path(&self) -> &str {
        self.path.as_str()
    }
}

impl<S, F, R, St, Item> Service<Request<Body>> for SseBroadcastProxyService<S, F>
where
    S: Service<Request<Body>, Response = Response<Body>, Error = Infallible>,
    S::Response: 'static,
    S::Error: Into<Box<dyn Error + Send + Sync>> + 'static,
    S::Future: Send + 'static,
    F: Fn() -> R,
    R: Future<Output = Result<St, Box<dyn Error + Send + Sync>>> + Send + 'static,
    St: Stream<Item = Item> + Send + Unpin + 'static,
    Item: AsRef<str> + Send + Sync + 'static,
{
    type Response = S::Response;
    type Error = Box<dyn Error + Send + Sync>;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        // check if the request is for the path we are listening on for SSE
        if self.path.as_str() == request.uri() {
            return self.svc.call(request)
        }

        // delegate to the inner service if path does not match
        let fut = self.inner.call(request);

        Box::pin(async move { fut.await.map_err(Into::into) })
    }
}
