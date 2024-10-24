use crate::{join, publish, serve, subscribe, Addr, App, Handle, Id, Invoke, Op, Res, Source};

pub mod messages {
    use crate::{Addr, Id, Op, Res};

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

    #[derive(Debug, Clone)]
    pub struct View {
        pub seq: u32,
        pub primary_id: Id,
        pub backup_id: Id,
    }
}

#[derive(Debug)]
pub struct Query;

pub async fn view_server_task() {
    //
}

pub async fn client_task(id: Id, addr: Addr) {
    subscribe::<Addr, messages::Reply, _>(move |source| {
        struct ServeTask {
            server_id: Id,
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
                publish((self.server_id, self.id), request);
                // TODO timeout & query view
                while let Some(reply) = self.source.recv().await {
                    if reply.seq != self.seq {
                        continue;
                    }
                    return reply.result;
                }
                unreachable!()
            }
        }
        async move {
            let server_id = crate::request::<_, _, messages::View>(Query, ())
                .await
                .primary_id;
            serve::<Invoke, Op, _>(ServeTask {
                seq: 0,
                source,
                id,
                addr,
                server_id,
            })
            .await
        }
    })
    .await;
}

pub async fn server_task(app: App, id: Id) {
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

    struct NewView;

    let sync_task = subscribe::<NewView, messages::View, _>(|mut source| async move {
        while let Some(view) = source.recv().await {
            if view.primary_id != id && view.backup_id != id {
                continue;
            }
            if view.backup_id == id {
                
            }
        }
    });
}
