pub mod amo;

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
