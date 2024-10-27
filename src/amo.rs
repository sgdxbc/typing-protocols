use std::collections::BTreeMap;

use crate::{Addr, App, Id, Op, Res};

pub mod messages {
    use crate::{Addr, Id, Op, Res};

    #[derive(Debug, Clone)]
    pub struct Request {
        pub id: Id,
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

pub trait ClientContext {
    fn finalize_invoke(&mut self, result: Res);
    fn send_to_server(&mut self, request: messages::Request);
    // TODO resend timeout
}

pub trait ServerContext {
    fn send(&mut self, addr: Addr, reply: messages::Reply);
}

mod typed {
    use crate::{Addr, App, Id, Op, Res};

    use super::messages;

    #[derive(Debug, Clone)]
    pub struct Replied(u32);

    #[derive(Debug, Clone)]
    pub struct WaitingReply(u32, pub Op);

    impl Replied {
        pub fn new() -> Self {
            Self(0)
        }

        // transition makes both constraints of
        // * what state can move to (similar to cannot directly Clean->Clean, must go through
        //   Clean->Dirty->InFlight->Clean)
        // * what side effect can/has to be made during transition (similar to InFlight->Clean must
        //   go through a blocking hardware-synchronized `fence()`)
        //   in the ideal cases this builds a bidirectional mapping between transition and side 
        //   effects
        //   + if a Replied->WaitingReply happens, a `send_to_server(...)` must be issued
        //   + `send_to_server(...)` is only issued in Replied->WaitingReply and
        //     WaitingReply->WaitingReply transitions
        //   so both direction of reasoning can be benefitted
        pub fn request(
            self,
            id: Id,
            op: Op,
            client_addr: Addr,
            context: &mut impl super::ClientContext,
        ) -> WaitingReply {
            let seq = self.0 + 1;
            let request = messages::Request {
                id,
                seq,
                op: op.clone(),
                client_addr,
            };
            context.send_to_server(request);
            // TODO set timer
            WaitingReply(seq, op)
        }
    }

    impl WaitingReply {
        pub fn replied(self, result: Res, context: &mut impl super::ClientContext) -> Replied {
            context.finalize_invoke(result);
            // TODO if necessary unset timer here
            Replied(self.0)
        }

        pub fn timeout(
            self,
            id: Id,
            client_addr: Addr,
            context: &mut impl super::ClientContext,
        ) -> Self {
            let Self(seq, op) = self;
            let request = messages::Request {
                id,
                seq,
                op: op.clone(),
                client_addr,
            };
            context.send_to_server(request);
            // TODO if necessary reset timer here
            Self(seq, op)
        }

        // utilities
        pub fn is_replied(&self, seq: u32) -> bool {
            seq == self.0
        }
    }

    #[derive(Debug, Clone)]
    pub struct Replying(u32, pub Res);

    impl Replied {
        pub fn execute(
            self,
            seq: u32,
            op: Op,
            addr: Addr,
            app: &mut App,
            context: &mut impl super::ServerContext,
        ) -> Replying {
            // alternative: check for seq > self.0 i.e. the transition is permitted by semantic
            // this "inline checking" style probably won't work for even slightly more involved
            // cases, so currently taking this "just transition" style and rely on protocol side
            // checking
            let result = app.execute(op);
            let reply = messages::Reply {
                seq,
                result: result.clone(),
            };
            context.send(addr, reply);
            Replying(seq, result)
        }
    }

    impl Replying {
        pub fn received(self) -> Replied {
            Replied(self.0)
        }

        pub fn reply(self, addr: Addr, context: &mut impl super::ServerContext) -> Self {
            let Self(seq, result) = self;
            let reply = messages::Reply {
                seq,
                result: result.clone(),
            };
            context.send(addr, reply);
            Replying(seq, result)
        }

        // utilities
        pub fn is_received(&self, seq: u32) -> bool {
            seq > self.0
        }

        pub fn is_replying(&self, seq: u32) -> bool {
            seq == self.0
        }
    }
}

#[derive(Debug, Clone)]
enum ClientRequestState {
    Replied(typed::Replied),
    WaitingReply(typed::WaitingReply),
}

#[derive(Debug)]
pub struct ClientState {
    id: Id,
    addr: Addr,
    request: ClientRequestState,
}

impl ClientState {
    pub fn new(id: Id, addr: Addr) -> Self {
        Self {
            id,
            addr,
            request: ClientRequestState::Replied(typed::Replied::new()),
        }
    }
}

impl ClientState {
    pub fn invoke(&mut self, op: Op, context: &mut impl ClientContext) {
        let ClientRequestState::Replied(state) = self.request.clone() else {
            unimplemented!()
        };
        let state = state.request(self.id, op.clone(), self.addr, context);
        self.request = ClientRequestState::WaitingReply(state)
    }

    pub fn handle_timeout(&mut self, context: &mut impl ClientContext) {
        let ClientRequestState::WaitingReply(state) = self.request.clone() else {
            unimplemented!() // or return, depending on unset/reset style timer API
        };
        let state = state.timeout(self.id, self.addr, context);
        self.request = ClientRequestState::WaitingReply(state)
    }

    pub fn handle_reply(&mut self, reply: messages::Reply, context: &mut impl ClientContext) {
        let ClientRequestState::WaitingReply(state) = self.request.clone() else {
            return;
        };
        if state.is_replied(reply.seq) {
            let state = state.replied(reply.result, context);
            self.request = ClientRequestState::Replied(state)
        }
    }
}

#[derive(Debug)]
pub struct ServerState {
    app: App,
    replying: BTreeMap<Id, typed::Replying>,
}

impl ServerState {
    pub fn new(app: App) -> Self {
        Self {
            app,
            replying: Default::default(),
        }
    }

    pub fn handle_request(&mut self, request: messages::Request, context: &mut impl ServerContext) {
        let state = if let Some(state) = self.replying.get_mut(&request.id) {
            if state.is_received(request.seq) {
                state.clone().received()
            } else {
                if state.is_replying(request.seq) {
                    *state = state.clone().reply(request.client_addr, context)
                }
                return;
            }
        } else {
            typed::Replied::new()
        };
        let state = state.execute(
            request.seq,
            request.op,
            request.client_addr,
            &mut self.app,
            context,
        );
        self.replying.insert(request.id, state);
    }
}
