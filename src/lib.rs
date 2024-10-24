use std::future::Future;

pub mod amo;
pub mod pb;

pub async fn subscribe<K, M, F>(create_task: impl FnMut(Source<M>) -> F)
where
    F: Future<Output = ()>,
{
    let _ = create_task;
}

pub struct Source<M>(M);

impl<M> Source<M> {
    pub async fn recv(&mut self) -> Option<M> {
        todo!()
    }
}

pub fn publish<K, M>(selector: K, message: M) {
    let _ = (selector, message);
}

pub async fn serve<K, Q, P>(handle: impl Handle<Q, P>) {
    let _ = handle;
}

pub trait Handle<Q, P> {
    fn handle(&mut self, request: Q) -> impl Future<Output = P>;
}

pub async fn request<K, Q, P>(selector: K, message: Q) -> P {
    let _ = (selector, message);
    todo!()
}

pub async fn join<T, U>(
    task: impl Future<Output = T>,
    other_task: impl Future<Output = U>,
) -> (T, U) {
    let _ = (task, other_task);
    todo!()
}

#[derive(Debug, Clone)]
pub struct Op;
#[derive(Debug, Clone)]
pub struct Res;

#[derive(Debug)]
pub struct App;

impl App {
    pub fn execute(&mut self, op: Op) -> Res {
        let _ = op;
        Res
    }
}

pub struct Invoke;

#[derive(Debug, Clone, Copy)]
pub struct Addr;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Id;
