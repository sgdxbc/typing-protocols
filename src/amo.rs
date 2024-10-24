use crate::{join, publish, serve, subscribe, App, Handle, Op, Res, Source};

#[derive(Debug, Clone, Copy)]
pub struct Addr;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Id;

pub mod messages {
    use crate::{Op, Res};

    use super::Addr;

    #[derive(Debug, Clone)]
    pub struct Request {
        pub seq: u32,
        pub op: Op,
        pub client_addr: Addr,
    }

    #[derive(Debug, Clone)]
    pub struct Reply {
        pub seq: u32,
        pub result: Res,
    }
}

pub struct Invoke;

pub async fn client_task(id: Id, addr: Addr) {
    subscribe::<Addr, messages::Reply, _>(move |source| {
        struct ServeTask {
            seq: u32,
            source: Source<messages::Reply>,
            id: Id,
            addr: Addr,
        }
        impl Handle<Op, Res> for ServeTask {
            async fn handle(&mut self, request: Op) -> Res {
                self.seq += 1;
                let request = messages::Request {
                    seq: self.seq,
                    op: request,
                    client_addr: self.addr,
                };
                publish(self.id, request);
                while let Some(reply) = self.source.recv().await {
                    if reply.seq != self.seq {
                        continue;
                    }
                    return reply.result;
                }
                unreachable!()
            }
        }
        serve::<Invoke, Op, _>(ServeTask {
            seq: 0,
            source,
            id,
            addr,
        })
    })
    .await;
}

pub async fn server_task(app: App) {
    struct Execute;

    let reply_tasks = subscribe::<Id, messages::Request, _>(|mut source| {
        let mut saved_reply = Option::<messages::Reply>::None;
        async move {
            while let Some(request) = source.recv().await {
                if let Some(reply) = &saved_reply {
                    if request.seq < reply.seq {
                        continue;
                    }
                    if request.seq == reply.seq {
                        publish(request.client_addr, reply.clone());
                    }
                }
                let result = crate::request(Execute, request.op).await;
                let reply = messages::Reply {
                    seq: request.seq,
                    result,
                };
                saved_reply = Some(reply.clone());
                publish(request.client_addr, reply)
            }
        }
    });

    struct ExecuteTask(App);
    impl Handle<Op, Res> for ExecuteTask {
        async fn handle(&mut self, request: Op) -> Res {
            let Self(app) = self;
            app.execute(request)
        }
    }
    let execute_task = serve::<Execute, Op, _>(ExecuteTask(app));

    join(reply_tasks, execute_task).await;
}
