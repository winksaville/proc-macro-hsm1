use std::cell::RefCell;

use custom_logger::env_logger_init;

use hsm0_with_executor::{DynError, Executor, Handled, StateInfo, StateResult};

// StateMachine simply transitions back and forth
// between initial and other.
//
//  other_base=2          initial_base=0
//       ^                     ^
//       |                     |
//     other=3              initial=1

#[derive(Default, Debug)]
pub struct StateMachine;

// Create a Protocol with no messages
#[derive(Debug)]
pub struct NoMessages;

const MAX_STATES: usize = 4;
const IDX_INITIAL_BASE: usize = 0;
const IDX_INITIAL: usize = 1;
const IDX_OTHER_BASE: usize = 2;
const IDX_OTHER: usize = 3;

impl StateMachine {
    pub fn new() -> Result<Executor<Self, NoMessages>, DynError> {
        let sm = RefCell::new(StateMachine::default());
        let sme = Executor::new(sm, MAX_STATES)
            .state(
                StateInfo::new("initial_base", Self::initial_base)
                    .enter_fn(Self::initial_base_enter)
                    .exit_fn(Self::initial_base_exit),
            )
            .state(
                StateInfo::new("initial", Self::initial)
                    .enter_fn(Self::initial_enter)
                    .exit_fn(Self::initial_exit)
                    .parent_idx(IDX_INITIAL_BASE),
            )
            .state(
                StateInfo::new("other_base", Self::other_base)
                    .enter_fn(Self::other_base_enter)
                    .exit_fn(Self::other_base_exit),
            )
            .state(
                StateInfo::new("other", Self::other)
                    .enter_fn(Self::other_enter)
                    .exit_fn(Self::other_exit)
                    .parent_idx(IDX_OTHER_BASE),
            )
            .build(IDX_INITIAL)?;

        log::trace!(
            "new: inital state={} idxs_enter_fns={:?}",
            sme.get_current_state_name(),
            sme.idxs_enter_fns
        );

        Ok(sme)
    }

    fn initial_base_enter(&mut self, _msg: &NoMessages) {}

    // This state has hdl 0
    fn initial_base(&mut self, _e: &Executor<Self, NoMessages>, _msg: &NoMessages) -> StateResult {
        (Handled::Yes, None)
    }

    fn initial_base_exit(&mut self, _msg: &NoMessages) {}

    fn initial_enter(&mut self, _msg: &NoMessages) {}

    // This state has hdl 0
    fn initial(&mut self, _e: &Executor<Self, NoMessages>, _msg: &NoMessages) -> StateResult {
        (Handled::Yes, Some(IDX_OTHER))
    }

    fn initial_exit(&mut self, _msg: &NoMessages) {}

    fn other_base_enter(&mut self, _msg: &NoMessages) {}

    // This state has hdl 0
    fn other_base(&mut self, _e: &Executor<Self, NoMessages>, _msg: &NoMessages) -> StateResult {
        (Handled::Yes, None)
    }

    fn other_base_exit(&mut self, _msg: &NoMessages) {}

    fn other_enter(&mut self, _msg: &NoMessages) {}

    // This state has hdl 0
    fn other(&mut self, _e: &Executor<Self, NoMessages>, _msg: &NoMessages) -> StateResult {
        (Handled::Yes, Some(IDX_INITIAL))
    }

    fn other_exit(&mut self, _msg: &NoMessages) {}
}

fn test_transition_between_leafs_across_trees() {
    // Create a sme and validate it's in the expected state
    let mut sme = StateMachine::new().unwrap();
    assert_eq!(sme.get_state_enter_cnt(IDX_INITIAL_BASE), 0);
    assert_eq!(sme.get_state_process_cnt(IDX_INITIAL_BASE), 0);
    assert_eq!(sme.get_state_exit_cnt(IDX_INITIAL_BASE), 0);
    assert_eq!(sme.get_state_enter_cnt(IDX_INITIAL), 0);
    assert_eq!(sme.get_state_process_cnt(IDX_INITIAL), 0);
    assert_eq!(sme.get_state_exit_cnt(IDX_INITIAL), 0);
    assert_eq!(sme.get_state_enter_cnt(IDX_OTHER_BASE), 0);
    assert_eq!(sme.get_state_process_cnt(IDX_OTHER_BASE), 0);
    assert_eq!(sme.get_state_exit_cnt(IDX_OTHER_BASE), 0);
    assert_eq!(sme.get_state_enter_cnt(IDX_OTHER), 0);
    assert_eq!(sme.get_state_process_cnt(IDX_OTHER), 0);
    assert_eq!(sme.get_state_exit_cnt(IDX_OTHER), 0);

    sme.dispatch(&NoMessages);
    assert_eq!(sme.get_state_enter_cnt(IDX_INITIAL_BASE), 1);
    assert_eq!(sme.get_state_process_cnt(IDX_INITIAL_BASE), 0);
    assert_eq!(sme.get_state_exit_cnt(IDX_INITIAL_BASE), 1);
    assert_eq!(sme.get_state_enter_cnt(IDX_INITIAL), 1);
    assert_eq!(sme.get_state_process_cnt(IDX_INITIAL), 1);
    assert_eq!(sme.get_state_exit_cnt(IDX_INITIAL), 1);
    assert_eq!(sme.get_state_enter_cnt(IDX_OTHER_BASE), 0);
    assert_eq!(sme.get_state_process_cnt(IDX_OTHER_BASE), 0);
    assert_eq!(sme.get_state_exit_cnt(IDX_OTHER_BASE), 0);
    assert_eq!(sme.get_state_enter_cnt(IDX_OTHER), 0);
    assert_eq!(sme.get_state_process_cnt(IDX_OTHER), 0);
    assert_eq!(sme.get_state_exit_cnt(IDX_OTHER), 0);

    sme.dispatch(&NoMessages);
    assert_eq!(sme.get_state_enter_cnt(IDX_INITIAL_BASE), 1);
    assert_eq!(sme.get_state_process_cnt(IDX_INITIAL_BASE), 0);
    assert_eq!(sme.get_state_exit_cnt(IDX_INITIAL_BASE), 1);
    assert_eq!(sme.get_state_enter_cnt(IDX_INITIAL), 1);
    assert_eq!(sme.get_state_process_cnt(IDX_INITIAL), 1);
    assert_eq!(sme.get_state_exit_cnt(IDX_INITIAL), 1);
    assert_eq!(sme.get_state_enter_cnt(IDX_OTHER_BASE), 1);
    assert_eq!(sme.get_state_process_cnt(IDX_OTHER_BASE), 0);
    assert_eq!(sme.get_state_exit_cnt(IDX_OTHER_BASE), 1);
    assert_eq!(sme.get_state_enter_cnt(IDX_OTHER), 1);
    assert_eq!(sme.get_state_process_cnt(IDX_OTHER), 1);
    assert_eq!(sme.get_state_exit_cnt(IDX_OTHER), 1);

    sme.dispatch(&NoMessages);
    assert_eq!(sme.get_state_enter_cnt(IDX_INITIAL_BASE), 2);
    assert_eq!(sme.get_state_process_cnt(IDX_INITIAL_BASE), 0);
    assert_eq!(sme.get_state_exit_cnt(IDX_INITIAL_BASE), 2);
    assert_eq!(sme.get_state_enter_cnt(IDX_INITIAL), 2);
    assert_eq!(sme.get_state_process_cnt(IDX_INITIAL), 2);
    assert_eq!(sme.get_state_exit_cnt(IDX_INITIAL), 2);
    assert_eq!(sme.get_state_enter_cnt(IDX_OTHER_BASE), 1);
    assert_eq!(sme.get_state_process_cnt(IDX_OTHER_BASE), 0);
    assert_eq!(sme.get_state_exit_cnt(IDX_OTHER_BASE), 1);
    assert_eq!(sme.get_state_enter_cnt(IDX_OTHER), 1);
    assert_eq!(sme.get_state_process_cnt(IDX_OTHER), 1);
    assert_eq!(sme.get_state_exit_cnt(IDX_OTHER), 1);

    sme.dispatch(&NoMessages);
    assert_eq!(sme.get_state_enter_cnt(IDX_INITIAL_BASE), 2);
    assert_eq!(sme.get_state_process_cnt(IDX_INITIAL_BASE), 0);
    assert_eq!(sme.get_state_exit_cnt(IDX_INITIAL_BASE), 2);
    assert_eq!(sme.get_state_enter_cnt(IDX_INITIAL), 2);
    assert_eq!(sme.get_state_process_cnt(IDX_INITIAL), 2);
    assert_eq!(sme.get_state_exit_cnt(IDX_INITIAL), 2);
    assert_eq!(sme.get_state_enter_cnt(IDX_OTHER_BASE), 2);
    assert_eq!(sme.get_state_process_cnt(IDX_OTHER_BASE), 0);
    assert_eq!(sme.get_state_exit_cnt(IDX_OTHER_BASE), 2);
    assert_eq!(sme.get_state_enter_cnt(IDX_OTHER), 2);
    assert_eq!(sme.get_state_process_cnt(IDX_OTHER), 2);
    assert_eq!(sme.get_state_exit_cnt(IDX_OTHER), 2);

    sme.dispatch(&NoMessages);
    assert_eq!(sme.get_state_enter_cnt(IDX_INITIAL_BASE), 3);
    assert_eq!(sme.get_state_process_cnt(IDX_INITIAL_BASE), 0);
    assert_eq!(sme.get_state_exit_cnt(IDX_INITIAL_BASE), 3);
    assert_eq!(sme.get_state_enter_cnt(IDX_INITIAL), 3);
    assert_eq!(sme.get_state_process_cnt(IDX_INITIAL), 3);
    assert_eq!(sme.get_state_exit_cnt(IDX_INITIAL), 3);
    assert_eq!(sme.get_state_enter_cnt(IDX_OTHER_BASE), 2);
    assert_eq!(sme.get_state_process_cnt(IDX_OTHER_BASE), 0);
    assert_eq!(sme.get_state_exit_cnt(IDX_OTHER_BASE), 2);
    assert_eq!(sme.get_state_enter_cnt(IDX_OTHER), 2);
    assert_eq!(sme.get_state_process_cnt(IDX_OTHER), 2);
    assert_eq!(sme.get_state_exit_cnt(IDX_OTHER), 2);
}

fn main() {
    println!("main");
    env_logger_init("info");
    log::info!("main:+");

    test_transition_between_leafs_across_trees();

    log::info!("main:-");
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_leaf_transitions_across_trees() {
        test_transition_between_leafs_across_trees();
    }
}
