//! SSE server side support.

use http::{header::HeaderValue, HeaderName, Request, Response};
use hyper::Body;
use std::{
    error::Error,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tower::{Layer, Service};

/// Layer that can create [`SseBroadcastService`].
#[derive(Clone)]
pub struct SseBroadcastLayer<F> {
    handler: F,
    path: Arc<String>,
}

impl<F> SseBroadcastLayer<F> {
    /// Creates a new [`SseBroadcastLayer`] with the given handler and the path to listen on.
    pub fn new(path: String, handler: F) -> Self {
        Self { path: Arc::new(path), handler }
    }
}

impl<F, S, R> Layer<S> for SseBroadcastLayer<F>
where
    F: Fn() -> R,
{
    type Service = SseBroadcastService<S, R>;

    fn layer(&self, inner: S) -> Self::Service {
        let get_recv = (self.handler)();
        SseBroadcastService { inner, get_recv, path: Arc::clone(&self.path) }
    }
}

pub struct SseBroadcastService<S, R> {
    path: Arc<String>,
    inner: S,
    get_recv: R,
    // TODO needs name and id
}

impl<S, R> Service<Request<Body>> for SseBroadcastService<S, R>
where
    S: Service<Request<Body>, Response = Response<Body>>,
    S::Response: 'static,
    S::Error: Into<Box<dyn Error + Send + Sync>> + 'static,
    S::Future: Send + 'static,
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
        // if self.path.as_ref() == request.uri() {
        //     // acquire the receiver and perform the upgrade
        //
        //     let (sender, encoder) = async_sse::encode();
        //
        // }

        // delegate to the inner service if path does not match
        let fut = self.inner.call(request);

        todo!()
    }
}
