//! Service implementation for queueing simulation stops.

use std::{
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::{atomic::AtomicU64, Arc},
    task::{ready, Context, Poll},
    time::Instant,
};

use crate::{BundleSimulationOutcome, BundleSimulator, SimulatedBundle};
use alloy::rpc::types::mev::{SendBundleRequest, SimBundleOverrides};
use futures_util::{stream::FuturesUnordered, Stream, StreamExt};
use mev_share_rpc_api::jsonrpsee;
use pin_project_lite::pin_project;
use tokio::sync::{mpsc, oneshot};
use tracing::debug;

/// Frontend type that can communicate with [BundleSimulatorService].
#[derive(Debug, Clone)]
pub struct BundleSimulatorHandle {
    to_service: mpsc::UnboundedSender<BundleSimulatorMessage>,
    inner: Arc<BundleSimulatorInner>,
}

#[derive(Debug)]
struct BundleSimulatorInner {
    /// tracks the number of queued jobs.
    queued_jobs: AtomicU64,
    /// The current block number.
    current_block_number: AtomicU64,
}

// === impl BundleSimulatorHandle ===

impl BundleSimulatorHandle {
    /// Returns the number of queued jobs.
    pub fn queued_jobs(&self) -> u64 {
        self.inner.queued_jobs.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Updates the current block number.
    pub fn update_block_number(&self, block_number: u64) {
        self.to_service.send(BundleSimulatorMessage::UpdateBlockNumber(block_number)).ok();
    }

    /// Clears all queued jobs.
    pub fn clear_queue(&self) {
        self.to_service.send(BundleSimulatorMessage::ClearQueue).ok();
    }

    /// Adds a new bundle simulation to the queue with [SimulationPriority::High].
    ///
    /// Returns an error when the service failed to queue in the simulation.
    pub fn send_bundle_simulation_high_prio(
        &self,
        request: SendBundleRequest,
        overrides: SimBundleOverrides,
    ) -> Result<(), AddSimulationErr> {
        self.send_bundle_simulation_with_prio(request, overrides, SimulationPriority::High)
    }
    /// Adds a new bundle simulation to the queue with [SimulationPriority::Normal].
    ///
    /// Returns an error when the service failed to queue in the simulation.
    pub fn send_bundle_simulation(
        &self,
        request: SendBundleRequest,
        overrides: SimBundleOverrides,
    ) -> Result<(), AddSimulationErr> {
        self.send_bundle_simulation_with_prio(request, overrides, SimulationPriority::Normal)
    }

    /// Adds a new bundle simulation to the queue.
    ///
    /// Returns an error when the service failed to queue in the simulation.
    pub fn send_bundle_simulation_with_prio(
        &self,
        request: SendBundleRequest,
        overrides: SimBundleOverrides,
        priority: SimulationPriority,
    ) -> Result<(), AddSimulationErr> {
        let (tx, ..) = oneshot::channel();
        self.to_service.send(BundleSimulatorMessage::AddSimulation {
            request: SimulationRequest {
                request,
                priority,
                overrides,
                backed_off_until: None,
                retries: 0,
            },
            tx,
        })?;
        Ok(())
    }

    /// Adds a new bundle simulation to the queue with [SimulationPriority::High].
    ///
    /// Returns an error when the service failed to queue in the simulation.
    pub async fn add_bundle_simulation_high_prio(
        &self,
        request: SendBundleRequest,
        overrides: SimBundleOverrides,
    ) -> Result<(), AddSimulationErr> {
        self.add_bundle_simulation_with_prio(request, overrides, SimulationPriority::High).await
    }

    /// Adds a new bundle simulation to the queue with [SimulationPriority::Normal].
    ///
    /// Returns an error when the service failed to queue in the simulation.
    pub async fn add_bundle_simulation(
        &self,
        request: SendBundleRequest,
        overrides: SimBundleOverrides,
    ) -> Result<(), AddSimulationErr> {
        self.add_bundle_simulation_with_prio(request, overrides, SimulationPriority::Normal).await
    }

    /// Adds a new bundle simulation to the queue.
    ///
    /// Returns an error when the service failed to queue in the simulation.
    pub async fn add_bundle_simulation_with_prio(
        &self,
        request: SendBundleRequest,
        overrides: SimBundleOverrides,
        priority: SimulationPriority,
    ) -> Result<(), AddSimulationErr> {
        let (tx, rx) = oneshot::channel();
        self.to_service.send(BundleSimulatorMessage::AddSimulation {
            request: SimulationRequest {
                request,
                priority,
                overrides,
                backed_off_until: None,
                retries: 0,
            },
            tx,
        })?;

        rx.await.map_err(|_| AddSimulationErr::ServiceUnavailable)?
    }

    /// Returns a new listener for simulation events.
    pub fn events(&self) -> SimulationEventStream {
        let (tx, rx) = mpsc::unbounded_channel();
        let _ = self.to_service.send(BundleSimulatorMessage::AddEventListener(tx));
        SimulationEventStream { inner: rx }
    }
}

/// Provides a service for simulating bundles.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct BundleSimulatorService<Sim: BundleSimulator> {
    /// Creates new simulations.
    simulator: Sim,
    /// The current simulations.
    simulations: FuturesUnordered<Simulation<Sim::Simulation>>,
    high_priority_queue: Vec<SimulationRequest>,
    normal_priority_queue: Vec<SimulationRequest>,
    /// incoming messages from the handle.
    from_handle: mpsc::UnboundedReceiver<BundleSimulatorMessage>,
    /// Copy of the handle sender to keep the channel open.
    to_service: mpsc::UnboundedSender<BundleSimulatorMessage>,
    /// Shared internals
    inner: Arc<BundleSimulatorInner>,
    /// Listeners for simulation events.
    listeners: Vec<mpsc::UnboundedSender<SimulationEvent>>,
    /// How this service is configured.
    config: BundleSimulatorServiceConfig,
}

impl<Sim: BundleSimulator> BundleSimulatorService<Sim> {
    /// Creates a new [BundleSimulatorService] with the given simulator.
    pub fn new(
        current_block_number: u64,
        simulator: Sim,
        config: BundleSimulatorServiceConfig,
    ) -> Self {
        let (to_service, from_handle) = mpsc::unbounded_channel();
        let inner = Arc::new(BundleSimulatorInner {
            queued_jobs: AtomicU64::new(0),
            current_block_number: AtomicU64::new(current_block_number),
        });
        Self {
            simulator,
            simulations: Default::default(),
            high_priority_queue: Default::default(),
            normal_priority_queue: Default::default(),
            from_handle,
            to_service,
            inner,
            listeners: vec![],
            config,
        }
    }

    /// Returns a new handle to this service
    pub fn handle(&self) -> BundleSimulatorHandle {
        BundleSimulatorHandle {
            to_service: self.to_service.clone(),
            inner: Arc::clone(&self.inner),
        }
    }

    fn current_block_number(&self) -> u64 {
        self.inner.current_block_number.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Notifies all listeners of the given event.
    fn notify_listeners(&mut self, event: SimulationEvent) {
        self.listeners.retain(|l| l.send(event.clone()).is_ok());
    }

    fn has_high_priority_capacity(&self) -> bool {
        self.high_priority_queue.len() < self.config.max_high_prio_queue_len
    }

    fn has_normal_priority_capacity(&self) -> bool {
        self.normal_priority_queue.len() < self.config.max_normal_prio_queue_len
    }

    fn pop_next_ready(&mut self, next_block: u64, now: Instant) -> Option<SimulationRequest> {
        loop {
            let mut outdated = false;
            let pos = self
                .high_priority_queue
                .iter()
                .chain(self.normal_priority_queue.iter())
                // skip requests not ready for inclusion
                .position(|req| {
                    if !req.is_ready_at(now) {
                        return false
                    }
                    if req.exceeds_target_block(next_block) {
                        outdated = true;
                        return true
                    }
                    req.is_min_block(next_block)
                });

            if let Some(mut pos) = pos {
                let item = if pos < self.high_priority_queue.len() {
                    self.high_priority_queue.remove(pos)
                } else {
                    pos -= self.high_priority_queue.len();
                    self.normal_priority_queue.remove(pos)
                };

                if outdated {
                    self.notify_listeners(SimulationEvent::OutdatedRequest {
                        request: item.request,
                        overrides: item.overrides,
                        next_block,
                    });
                    continue
                }

                return Some(item)
            }

            return None
        }
    }

    /// Returns a [SimulationRequest] that is ready to be simulated.
    fn pop_best_requests(&mut self, now: Instant) -> Vec<SimulationRequest> {
        let mut requests = vec![];
        let capacity =
            self.config.max_concurrent_simulations.saturating_sub(self.simulations.len());
        if capacity == 0 {
            return requests
        }

        let current_block = self.current_block_number();
        let next_block = current_block + 1;

        while requests.len() != capacity {
            if let Some(req) = self.pop_next_ready(next_block, now) {
                requests.push(req);
            } else {
                break
            }
        }

        requests
    }

    /// Processes a new message from the handle.
    fn on_message(&mut self, msg: BundleSimulatorMessage) {
        debug!("Received message: {:?}", msg);
        match msg {
            BundleSimulatorMessage::UpdateBlockNumber(num) => {
                let current_block_number = self.current_block_number();
                if num != current_block_number {
                    let old = current_block_number;
                    self.inner
                        .current_block_number
                        .store(num, std::sync::atomic::Ordering::Relaxed);
                    self.notify_listeners(SimulationEvent::BlockNumberUpdated {
                        old,
                        current: current_block_number,
                    });
                }
            }
            BundleSimulatorMessage::ClearQueue => {
                self.high_priority_queue.clear();
                self.normal_priority_queue.clear();
            }
            BundleSimulatorMessage::AddEventListener(tx) => {
                self.listeners.push(tx);
            }
            BundleSimulatorMessage::AddSimulation { request, tx } => {
                if request.priority.is_high() {
                    if self.has_high_priority_capacity() {
                        self.high_priority_queue.push(request);
                        tx.send(Ok(())).ok();
                    } else {
                        tx.send(Err(AddSimulationErr::QueueFull)).ok();
                    }
                } else if self.has_normal_priority_capacity() {
                    self.normal_priority_queue.push(request);
                    tx.send(Ok(())).ok();
                } else {
                    tx.send(Err(AddSimulationErr::QueueFull)).ok();
                }
            }
        }
    }

    /// Processes a finished simulation.
    fn on_simulation_outcome(
        &mut self,
        outcome: BundleSimulationOutcome,
        mut sim: SimulationRequest,
    ) {
        debug!("Received simulation outcome: {:?}", outcome);
        match outcome {
            BundleSimulationOutcome::Success(resp) => {
                let SimulationRequest { retries, request, overrides, .. } = sim;
                self.notify_listeners(SimulationEvent::SimulatedBundle(Ok(SimulatedBundle {
                    request,
                    overrides,
                    response: resp,
                    retries,
                })));
            }
            BundleSimulationOutcome::Fatal(error) => {
                let err = SimulatedBundleErrorInner { error, sim };
                self.notify_listeners(SimulationEvent::SimulatedBundle(Err(
                    SimulatedBundleError { inner: Arc::new(err) },
                )));
            }
            BundleSimulationOutcome::Reschedule(_error) => {
                // reschedule the simulation.
                sim.retries += 1;
                if sim.retries > self.config.max_retries {
                    self.notify_listeners(SimulationEvent::ExceededMaxRetries {
                        request: sim.request,
                        overrides: sim.overrides,
                    });
                    return
                }
                if sim.priority.is_high() {
                    self.high_priority_queue.push(sim);
                } else {
                    self.normal_priority_queue.push(sim);
                }
            }
        }
    }
}

impl<Sim> Future for BundleSimulatorService<Sim>
where
    Sim: BundleSimulator,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        loop {
            // drain incoming messages from the handle.
            while let Poll::Ready(Some(msg)) = this.from_handle.poll_recv(cx) {
                this.on_message(msg);
            }

            // process all completed simulations.
            while let Poll::Ready(Some((outcome, inner))) = this.simulations.poll_next_unpin(cx) {
                this.on_simulation_outcome(outcome, inner);
            }

            // try to queue in a new simulation request
            let ready = this.pop_best_requests(Instant::now());
            let no_more_work = ready.is_empty();
            for req in ready {
                let sim =
                    this.simulator.simulate_bundle(req.request.clone(), req.overrides.clone());
                let sim = Simulation::new(sim, req);
                debug!("Starting new simulation");
                this.simulations.push(sim);
            }

            if no_more_work {
                break
            }
        }

        Poll::Pending
    }
}

/// Config values for the [BundleSimulatorService].
#[derive(Debug, Clone)]
pub struct BundleSimulatorServiceConfig {
    /// Maximum number of retries for a simulation.
    pub max_retries: usize,
    /// Maximum number of _unprocessed_ jobs in the normal priority queue.
    pub max_normal_prio_queue_len: usize,
    /// Maximum number of _unprocessed_ jobs in the high priority queue.
    pub max_high_prio_queue_len: usize,
    /// Maximum number of concurrently active simulations.
    pub max_concurrent_simulations: usize,
}

impl Default for BundleSimulatorServiceConfig {
    fn default() -> Self {
        Self {
            max_retries: 30,
            max_normal_prio_queue_len: 1024,
            max_high_prio_queue_len: 2048,
            max_concurrent_simulations: 32,
        }
    }
}

pin_project! {
    /// A simulation future
    struct Simulation<Sim> {
        #[pin]
        sim: Sim,
        inner: Option<SimulationRequest>
    }
}

impl<Sim> Simulation<Sim> {
    /// Creates a new simulation.
    fn new(sim: Sim, inner: SimulationRequest) -> Self {
        Self { sim, inner: Some(inner) }
    }
}

impl<Sim> Future for Simulation<Sim>
where
    Sim: Future,
{
    type Output = (Sim::Output, SimulationRequest);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let out = ready!(this.sim.poll(cx));
        let inner = this.inner.take().expect("Simulation polled after completion");
        Poll::Ready((out, inner))
    }
}

/// Error thrown when adding a simulation fails.
#[derive(Debug, thiserror::Error)]
pub enum AddSimulationError {
    /// Thrown when too many jobs are queued.
    #[error("too many jobs: {0}")]
    TooManyJobs(u64),
}

/// Message type passed to [BundleSimulatorService].
#[derive(Debug)]
enum BundleSimulatorMessage {
    /// Clear all ongoing jobs.
    ClearQueue,
    /// Set current block number
    UpdateBlockNumber(u64),
    /// Add a new simulation job
    AddSimulation { request: SimulationRequest, tx: oneshot::Sender<Result<(), AddSimulationErr>> },
    /// Queues in a new event listener.
    AddEventListener(mpsc::UnboundedSender<SimulationEvent>),
}

/// Events emitted by the simulation service.
#[derive(Debug, Clone)]
pub enum SimulationEvent {
    /// Result of a simulated bundle.
    SimulatedBundle(Result<SimulatedBundle, SimulatedBundleError>),
    /// A request has been dropped because it's max number exceeds the next block
    OutdatedRequest {
        /// The request object that was used for simulation.
        request: SendBundleRequest,
        /// The overrides that were used for simulation.
        overrides: SimBundleOverrides,
        /// Currently tracked next block
        next_block: u64,
    },
    /// A request has been dropped because it's max number of retries has been exceeded.
    ExceededMaxRetries {
        /// The request object that was used for simulation.
        request: SendBundleRequest,
        /// The overrides that were used for simulation.
        overrides: SimBundleOverrides,
    },
    /// Updated block number
    BlockNumberUpdated {
        /// replaced block number.
        old: u64,
        /// new block number.
        current: u64,
    },
}

pin_project! {
    /// A stream that yields simulation events.
    pub struct SimulationEventStream {
        #[pin]
        inner: mpsc::UnboundedReceiver<SimulationEvent>,
    }
}

impl SimulationEventStream {
    /// Only yields simulation results
    pub fn results(self) -> SimulationResultStream {
        SimulationResultStream { inner: self.inner }
    }
}

impl Stream for SimulationEventStream {
    type Item = SimulationEvent;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().inner.poll_recv(cx)
    }
}

pin_project! {
    /// A stream that yields outcome of simulations.
    pub struct SimulationResultStream {
        #[pin]
        inner: mpsc::UnboundedReceiver<SimulationEvent>,
    }
}

impl Stream for SimulationResultStream {
    type Item = Result<SimulatedBundle, SimulatedBundleError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            return match ready!(self.as_mut().project().inner.poll_recv(cx)) {
                None => Poll::Ready(None),
                Some(SimulationEvent::SimulatedBundle(res)) => Poll::Ready(Some(res)),
                _ => continue,
            }
        }
    }
}

/// Failed to simulate a bundle.
#[derive(Debug, Clone)]
pub struct SimulatedBundleError {
    inner: Arc<SimulatedBundleErrorInner>,
}

impl SimulatedBundleError {
    /// Returns the simulation request that failed.
    pub fn request(&self) -> &SimulationRequest {
        &self.inner.sim
    }

    /// Consumes the type and returns the simulation request that failed.
    pub fn into_request(self) -> SimulationRequest {
        match Arc::try_unwrap(self.inner) {
            Ok(res) => res.sim,
            Err(inner) => inner.sim.clone(),
        }
    }

    /// Attempts to downcast the error to a jsonrpsee error.
    pub fn as_rpc_error(&self) -> Option<&jsonrpsee::core::Error> {
        self.inner.error.downcast_ref()
    }
}

impl fmt::Display for SimulatedBundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.error.fmt(f)
    }
}

impl Error for SimulatedBundleError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.inner.error)
    }
}

#[derive(Debug)]
struct SimulatedBundleErrorInner {
    error: Box<dyn Error + Send + Sync>,
    sim: SimulationRequest,
}

/// How to queue in a simulation.
#[derive(Debug, Clone, Default)]
pub enum SimulationPriority {
    /// The simulation is not urgent.
    #[default]
    Normal,
    /// The simulation should be prioritized.
    High,
}

impl SimulationPriority {
    /// Returns whether the priority is high.
    pub fn is_high(&self) -> bool {
        matches!(self, Self::High)
    }

    /// Returns whether the priority is normal.
    pub fn is_normal(&self) -> bool {
        matches!(self, Self::Normal)
    }
}

/// A request to simulate a bundle.
#[derive(Debug, Clone)]
pub struct SimulationRequest {
    /// How often this has been retried
    pub retries: usize,
    /// The request object that was used for simulation.
    pub request: SendBundleRequest,
    /// The overrides that were used for simulation.
    pub overrides: SimBundleOverrides,
    /// The priority of the simulation.
    pub priority: SimulationPriority,
    /// The timestamp when the request can be resent again.
    pub backed_off_until: Option<Instant>,
}

impl SimulationRequest {
    fn is_min_block(&self, block: u64) -> bool {
        block > self.request.inclusion.block_number()
    }

    fn is_ready_at(&self, now: Instant) -> bool {
        self.backed_off_until.map_or(true, |backoff| now > backoff)
    }

    fn exceeds_target_block(&self, block: u64) -> bool {
        self.request.inclusion.max_block_number().map(|target| block > target).unwrap_or_default()
    }
}

/// Errors that can occur when adding a simulation job.
#[derive(Debug, thiserror::Error)]
pub enum AddSimulationErr {
    /// Thrown when the queue is full
    #[error("queue full")]
    QueueFull,
    /// Thrown when the service is unavailable (dropped).
    #[error("simulation service unavailable")]
    ServiceUnavailable,
}

impl<T> From<mpsc::error::SendError<T>> for AddSimulationErr {
    fn from(_: mpsc::error::SendError<T>) -> Self {
        Self::ServiceUnavailable
    }
}
