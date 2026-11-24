use std::sync::OnceLock;

use futures::Stream;
use futures::StreamExt;
use futures::stream::BoxStream;
use gtk::glib;

fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        // Network I/O does not need one worker per CPU on many-core mobile devices.
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(
                std::thread::available_parallelism()
                    .map(usize::from)
                    .unwrap_or(1)
                    .min(2),
            )
            .enable_all()
            .build()
            .expect("Failed to create the I/O runtime")
    })
}

pub(crate) struct Promise<Fut>(Fut);

impl<Fut> Promise<Fut> {
    pub(crate) fn new(future: Fut) -> Self {
        Self(future)
    }
}

impl<Fut> Promise<Fut>
where
    Fut: Future,
{
    pub(crate) fn block_on(self) -> Fut::Output {
        runtime().block_on(self.0)
    }
}

impl<Fut> Promise<Fut>
where
    Fut: Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    pub(crate) fn spawn(self) -> tokio::task::JoinHandle<Fut::Output> {
        runtime().spawn(self.0)
    }

    pub(crate) async fn exec(self) -> Fut::Output {
        self.spawn().await.expect("Failed to spawn future")
    }

    pub(crate) fn defer<F>(self, op: F)
    where
        F: FnOnce(Fut::Output) + 'static,
    {
        glib::spawn_future_local({
            let handle = self.spawn();
            async move {
                op(handle.await.unwrap());
            }
        });
    }
}

pub(crate) struct Pipe<A, P> {
    api: A,
    producer: P,
}

impl<A, P, I> Pipe<A, P>
where
    A: Send + 'static,
    for<'r> P: FnOnce(&'r A) -> BoxStream<'r, I> + Send + 'static,
    I: Send + 'static,
{
    pub(crate) fn new(api: A, producer: P) -> Self {
        Self { api, producer }
    }

    pub(crate) fn stream(self) -> impl Stream<Item = I> {
        let (tx, rx) = tokio::sync::mpsc::channel(10);

        Promise::new(async move {
            let mut stream = (self.producer)(&self.api);

            // A quiet event/log stream may never yield another item after the view closes.
            // Stop it as soon as its receiver is dropped, not only on the next send.
            while let futures::future::Either::Right((Some(item), _)) =
                futures::future::select(Box::pin(tx.closed()), stream.next()).await
            {
                if tx.send(item).await.is_err() {
                    break;
                }
            }
        })
        .spawn();

        tokio_stream::wrappers::ReceiverStream::new(rx)
    }

    pub(crate) fn on_next<F>(self, mut op: F) -> PipeFinish
    where
        F: FnMut(I) -> glib::ControlFlow + 'static,
    {
        glib::spawn_future_local({
            let mut stream = self.stream();
            async move {
                while let Some(item) = stream.next().await {
                    if op(item) == glib::ControlFlow::Break {
                        break;
                    }
                }
            }
        })
        .into()
    }
}

#[derive(Debug)]
pub(crate) struct PipeFinish(glib::JoinHandle<()>);
impl From<glib::JoinHandle<()>> for PipeFinish {
    fn from(value: glib::JoinHandle<()>) -> Self {
        Self(value)
    }
}
impl PipeFinish {
    pub(crate) fn abort(&self) {
        self.0.abort();
    }

    pub(crate) fn on_finish<F>(self, mut op: F)
    where
        F: FnMut() + 'static,
    {
        glib::spawn_future_local(async move {
            self.0.await.unwrap();
            op();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DropSignal(Option<tokio::sync::oneshot::Sender<()>>);

    impl Drop for DropSignal {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }

    #[test]
    fn dropping_receiver_releases_an_idle_producer() {
        runtime().block_on(async {
            let (sender, receiver) = tokio::sync::oneshot::channel();
            let stream = Pipe::new(DropSignal(Some(sender)), |_| {
                futures::stream::pending::<()>().boxed()
            })
            .stream();

            drop(stream);

            tokio::time::timeout(std::time::Duration::from_secs(2), receiver)
                .await
                .expect("An idle producer should be cancelled")
                .expect("The producer should release its API handle");
        });
    }
}
