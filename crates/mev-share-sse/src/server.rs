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
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
};
use tokio::sync::{broadcast, broadcast::error::SendError};
use tokio_stream::wrappers::BroadcastStream;
use tokio_util::{compat::FuturesAsyncReadCompatExt, io::ReaderStream};
use tower::{Layer, Service};

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
        futures_util::future::ready(Ok(SseBroadcastStream {
            st: BroadcastStream::new(self.subscribe()),
        }))
    }

    /// Creates a new Receiver handle that will receive values sent after this call to subscribe.
    pub fn subscribe(&self) -> broadcast::Receiver<SSeBroadcastMessage> {
        self.sender.subscribe()
    }

    /// Sends a message to all subscribers.
    ///
    /// See also [`Sender::send`](broadcast::Sender::send)
    pub fn send<T: Serialize>(&self, msg: &T) -> Result<usize, SendError<SSeBroadcastMessage>> {
        let msg = SSeBroadcastMessage(Arc::from(serde_json::to_string(msg).unwrap()));
        self.sender.send(msg)
    }
}

/// Helper new type to make Arc<str> AsRef str.
#[derive(Clone, Debug)]
pub struct SSeBroadcastMessage(Arc<str>);

impl AsRef<str> for SSeBroadcastMessage {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
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
                Some(Err(_)) => continue,
            }
        }
    }
}

/// A service impl that handles SSE requests.
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
        // check if the request is for the path we are listening on for SSE
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
