use std::sync::mpsc::Sender;

use custom_logger::env_logger_init;

use hsm0_executor::{DynError, Executor, Handled, StateInfo, StateResult};

#[derive(Debug, Clone)]
enum Messages {
    Value { val: i32 },
    Done { val: i32 },
}

#[derive(Debug)]
struct SendMsgToSelfSm {
    self_tx: Sender<Messages>,
    val: i32,
}

const MAX_STATES: usize = 2;
const IDX_BASE: usize = 0;
const IDX_DONE: usize = 0;

impl SendMsgToSelfSm {
    pub fn new(sender: Sender<Messages>) -> Result<Executor<Self, Messages>, DynError> {
        let sm = SendMsgToSelfSm {
            self_tx: sender,
            val: 0,
        };
        let mut sme = Executor::new(sm, MAX_STATES);

        sme.state(StateInfo::new("base", None, Self::base, None, None))
            .state(StateInfo::new("done", None, Self::done, None, None))
            .initialize(IDX_BASE)
            .expect("Unexpected error initializing");

        log::info!(
            "new: inital state={} idxs_enter_fns={:?}",
            sme.get_current_state_name(),
            sme.idxs_enter_fns
        );

        Ok(sme)
    }

    fn base(&mut self, msg: &Messages) -> StateResult {
        match msg {
            Messages::Value { val } => {
                log::info!("base Messages::Value:+ val={}", val);
                if self.val < 10 {
                    // Doing work
                    self.val += val;
                    if self.self_tx.send(msg.clone()).is_ok() {
                        log::info!("base Messages::Value:- self.val={}", self.val);
                        (Handled::Yes, None)
                    } else {
                        log::info!("base Messages::Value:- ERR so DONE self.val={}", self.val);
                        (Handled::Yes, Some(IDX_DONE))
                    }
                } else {
                    // We're done
                    self.send_done();

                    log::info!("base Messages::Value:- Done self.val={}", self.val);
                    (Handled::Yes, Some(IDX_DONE))
                }
            }
            Messages::Done { val: _ } => {
                self.send_done();
                (Handled::Yes, Some(IDX_DONE))
            }
        }
    }

    fn done(&mut self, _msg: &Messages) -> StateResult {
        // Responsed with Done for any messages
        self.send_done();
        log::info!("base:+- self.val={}", self.val);
        (Handled::Yes, None)
    }

    fn send_done(&mut self) {
        self.self_tx.send(Messages::Done { val: self.val }).ok();
    }
}

fn main() {
    env_logger_init("info");
    log::info!("main:+");

    let (tx, rx) = std::sync::mpsc::channel::<Messages>();
    let mut sme = SendMsgToSelfSm::new(tx).unwrap();

    // Dispatch the first message
    let msg = Messages::Value { val: 1 };
    sme.dispatch(&msg);

    // Receive messages until SendMsgToSelfSm reports Done or rx is closed
    while let Ok(m) = rx.recv() {
        match m {
            Messages::Value { val: _ } => {
                // Dispatch the message received
                sme.dispatch(&m);
            }
            Messages::Done { val } => {
                println!("main: Done val={val}");
                break;
            }
        }
    }

    log::info!("main:-");
}
