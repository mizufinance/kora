use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::{Duration, SystemTime},
};

use commonware_runtime::{self, tokio};
use governor::clock::{Clock as GovernorClock, ReasonablyRealtime};
use prometheus_client::registry::Metric;
use rand::rngs::OsRng;

/// Tokio context wrapper that forces simulated networking to bind on localhost.
pub(crate) struct TransportContext {
    inner: tokio::Context,
    force_base_addr: bool,
}

const PORT_OFFSET: u16 = 40_000;

fn remap_socket(socket: SocketAddr) -> SocketAddr {
    let port = socket.port();
    if port >= 1024 {
        return socket;
    }
    let remapped = port + PORT_OFFSET;
    match socket.ip() {
        IpAddr::V4(ip) => SocketAddr::new(IpAddr::V4(ip), remapped),
        IpAddr::V6(ip) => SocketAddr::new(IpAddr::V6(ip), remapped),
    }
}

impl TransportContext {
    pub(crate) fn new(inner: tokio::Context) -> Self {
        Self {
            inner,
            force_base_addr: true,
        }
    }
}

impl Clone for TransportContext {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            force_base_addr: false,
        }
    }
}

impl GovernorClock for TransportContext {
    type Instant = SystemTime;

    fn now(&self) -> Self::Instant {
        <tokio::Context as GovernorClock>::now(&self.inner)
    }
}

impl ReasonablyRealtime for TransportContext {}

impl commonware_runtime::Clock for TransportContext {
    fn current(&self) -> SystemTime {
        self.inner.current()
    }

    fn sleep(&self, duration: Duration) -> impl std::future::Future<Output = ()> + Send + 'static {
        self.inner.sleep(duration)
    }

    fn sleep_until(
        &self,
        deadline: SystemTime,
    ) -> impl std::future::Future<Output = ()> + Send + 'static {
        self.inner.sleep_until(deadline)
    }
}

impl commonware_runtime::Metrics for TransportContext {
    fn label(&self) -> String {
        self.inner.label()
    }

    fn with_label(&self, label: &str) -> Self {
        Self {
            inner: self.inner.with_label(label),
            force_base_addr: false,
        }
    }

    fn register<N: Into<String>, H: Into<String>>(
        &self,
        name: N,
        help: H,
        metric: impl Metric,
    ) {
        self.inner.register(name, help, metric);
    }

    fn encode(&self) -> String {
        self.inner.encode()
    }
}

impl commonware_runtime::Spawner for TransportContext {
    fn shared(mut self, blocking: bool) -> Self {
        self.inner = self.inner.shared(blocking);
        self
    }

    fn dedicated(mut self) -> Self {
        self.inner = self.inner.dedicated();
        self
    }

    fn instrumented(mut self) -> Self {
        self.inner = self.inner.instrumented();
        self
    }

    fn spawn<F, Fut, T>(self, f: F) -> commonware_runtime::Handle<T>
    where
        F: FnOnce(Self) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.inner.spawn(|context| {
            let context = TransportContext {
                inner: context,
                force_base_addr: false,
            };
            f(context)
        })
    }

    fn stop(
        self,
        value: i32,
        timeout: Option<Duration>,
    ) -> impl std::future::Future<Output = Result<(), commonware_runtime::Error>> + Send {
        self.inner.stop(value, timeout)
    }

    fn stopped(&self) -> commonware_runtime::signal::Signal {
        self.inner.stopped()
    }
}

impl commonware_runtime::Network for TransportContext {
    type Listener = <tokio::Context as commonware_runtime::Network>::Listener;

    fn bind(
        &self,
        socket: SocketAddr,
    ) -> impl std::future::Future<Output = Result<Self::Listener, commonware_runtime::Error>> + Send
    {
        self.inner.bind(remap_socket(socket))
    }

    fn dial(
        &self,
        socket: SocketAddr,
    ) -> impl std::future::Future<
        Output = Result<
            (
                commonware_runtime::SinkOf<Self>,
                commonware_runtime::StreamOf<Self>,
            ),
            commonware_runtime::Error,
        >,
    > + Send {
        self.inner.dial(remap_socket(socket))
    }
}

impl rand::RngCore for TransportContext {
    fn next_u32(&mut self) -> u32 {
        if self.force_base_addr {
            self.force_base_addr = false;
            return u32::from(Ipv4Addr::LOCALHOST);
        }
        let mut rng = OsRng;
        rand::RngCore::next_u32(&mut rng)
    }

    fn next_u64(&mut self) -> u64 {
        let mut rng = OsRng;
        rand::RngCore::next_u64(&mut rng)
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut rng = OsRng;
        rand::RngCore::fill_bytes(&mut rng, dest);
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        let mut rng = OsRng;
        rand::RngCore::try_fill_bytes(&mut rng, dest)
    }
}
