use tokio::task::JoinHandle;
use tracing::{subscriber::set_global_default, Subscriber};
use tracing_log::LogTracer;
use tracing_subscriber::{fmt::{self, MakeWriter}, layer::SubscriberExt, EnvFilter, Registry};

pub fn get_subscriber<Sink>(env_filter: String, sink: Sink) -> impl Subscriber + Send + Sync
where
    Sink: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(env_filter));

    let fmt_layer = fmt::Layer::default()
        .with_span_events(fmt::format::FmtSpan::CLOSE)
        .json()
        .flatten_event(true)
        .with_writer(sink);

    Registry::default().with(env_filter).with(fmt_layer)
}

pub fn init_subscriber(subscriber: impl Subscriber + Send + Sync) {
    // Convert logs as tracing events
    LogTracer::init().expect("Failed to set logger");

    set_global_default(subscriber).expect("Failed to set subscriber");
}

pub fn spawn_blocking_with_tracing<F, R>(f: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static
{
    let current_span = tracing::Span::current();
    tokio::task::spawn_blocking(move || { current_span.in_scope(f) })
}
