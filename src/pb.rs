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

    #[derive(Debug, Clone)]
    pub struct Query;

    #[derive(Debug, Clone)]
    pub struct Ping {
        pub id: Id,
    }

    #[derive(Debug, Clone)]
    pub struct ViewReply {
        pub num: u32,
        pub primary_id: Id,
        pub backup_id: Option<Id>,
    }
}

pub mod replica;