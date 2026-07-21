use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

pub struct CountToTwo{
    pub polled_once: bool
}

impl Future for CountToTwo{
    type Output = &'static str;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.polled_once{
            Poll::Ready("done!")
        }else{
            self.polled_once = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

struct NoopWaker;
impl Wake for NoopWaker{
    fn wake(self: std::sync::Arc<Self>) {}
}

pub fn block_on<F: Future>(mut fut: F) -> F::Output{
    let waker = Waker::from(Arc::new(NoopWaker));
    let mut cx = Context::from_waker(&waker);
    let mut fut = unsafe { Pin::new_unchecked(&mut fut) };
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(val) => return val,
            Poll::Pending => continue,
        }
    }
}

