use proc_macro_hsm1::{handled, hsm1, hsm1_state, not_handled, transition_to};
use state_result::*;
use std::collections::VecDeque;

struct NoMessages;

#[test]
fn test_initialization() {
    hsm1!(
        struct Test {}

        #[hsm1_state]
        // This state has hdl 0
        fn initial(&mut self, _msg: &NoMessages) -> StateResult {
            StateResult::Handled
        }
    );

    let mut fsm = Test::new();
    assert_eq!(fsm.smi.current_state_fns_hdl as usize, 0);
    assert_eq!(fsm.smi.previous_state_fns_hdl as usize, 0);
    assert!(fsm.smi.current_state_changed);

    fsm.dispatch(&NoMessages);
    assert_eq!(fsm.smi.current_state_fns_hdl as usize, 0);
    assert_eq!(fsm.smi.previous_state_fns_hdl as usize, 0);
    assert!(!fsm.smi.current_state_changed);
}

#[test]
fn test_dispatch() {
    hsm1!(
        struct TestDispatch {}

        #[hsm1_state]
        // This state has hdl 0
        fn initial(&mut self, _msg: &NoMessages) -> StateResult {
            StateResult::TransitionTo(1usize)
        }

        #[hsm1_state]
        // This state is hdl 1
        fn done(&mut self, _msg: &NoMessages) -> StateResult {
            StateResult::Handled
        }
    );

    let mut fsm = TestDispatch::new();
    assert_eq!(fsm.smi.current_state_fns_hdl as usize, 0);
    assert_eq!(fsm.smi.previous_state_fns_hdl as usize, 0);
    assert!(fsm.smi.current_state_changed);

    fsm.dispatch(&NoMessages);
    assert_eq!(fsm.smi.current_state_fns_hdl as usize, 1);
    assert_eq!(fsm.smi.previous_state_fns_hdl as usize, 0);
    assert!(fsm.smi.current_state_changed);

    fsm.dispatch(&NoMessages);
    assert_eq!(fsm.smi.current_state_fns_hdl as usize, 1);
    assert_eq!(fsm.smi.previous_state_fns_hdl as usize, 0);
    assert!(!fsm.smi.current_state_changed);

    fsm.dispatch(&NoMessages);
    assert_eq!(fsm.smi.current_state_fns_hdl as usize, 1);
    assert_eq!(fsm.smi.previous_state_fns_hdl as usize, 0);
    assert!(!fsm.smi.current_state_changed);
}

#[test]
fn test_transition_to() {
    hsm1!(
        struct Test {}

        #[hsm1_state]
        // This state has hdl 0
        fn initial(&mut self, _msg: &NoMessages) -> StateResult {
            StateResult::TransitionTo(1)
        }

        #[hsm1_state]
        // This state has hdl 1
        fn done(&mut self, _msg: &NoMessages) -> StateResult {
            StateResult::Handled
        }
    );

    let mut fsm = Test::new();
    assert_eq!(fsm.smi.current_state_fns_hdl as usize, 0); //Test::initial as usize);
    assert_eq!(fsm.smi.previous_state_fns_hdl as usize, 0); //Test::initial as usize);
    assert!(fsm.smi.current_state_changed);

    fsm.dispatch(&NoMessages);
    assert_eq!(fsm.smi.current_state_fns_hdl as usize, 1); //Test::done as usize);
    assert_eq!(fsm.smi.previous_state_fns_hdl as usize, 0); //Test::initial as usize);
    assert!(fsm.smi.current_state_changed);
}

#[test]
fn test_no_enter_exit() {
    hsm1!(
        struct Test {
            initial_enter_cnt: usize,
            initial_cnt: usize,
            initial_exit_cnt: usize,
            done_enter_cnt: usize,
            done_cnt: usize,
            done_exit_cnt: usize,
        }

        #[hsm1_state]
        // This state has hdl 0
        fn initial(&mut self, _msg: &NoMessages) -> StateResult {
            self.initial_cnt += 1;
            StateResult::TransitionTo(1usize) //Test::done)
        }

        #[hsm1_state]
        // This state has hdl 1
        fn done(&mut self, _msg: &NoMessages) -> StateResult {
            self.done_cnt += 1;
            StateResult::Handled
        }
    );

    let mut fsm = Test::new();
    assert_eq!(fsm.initial_enter_cnt, 0);
    assert_eq!(fsm.initial_cnt, 0);
    assert_eq!(fsm.initial_exit_cnt, 0);
    assert_eq!(fsm.done_enter_cnt, 0);
    assert_eq!(fsm.done_cnt, 0);
    assert_eq!(fsm.done_exit_cnt, 0);

    fsm.dispatch(&NoMessages);
    assert_eq!(fsm.initial_enter_cnt, 0);
    assert_eq!(fsm.initial_cnt, 1);
    assert_eq!(fsm.initial_exit_cnt, 0);
    assert_eq!(fsm.done_enter_cnt, 0);
    assert_eq!(fsm.done_cnt, 0);
    assert_eq!(fsm.done_exit_cnt, 0);

    fsm.dispatch(&NoMessages);
    assert_eq!(fsm.initial_enter_cnt, 0);
    assert_eq!(fsm.initial_cnt, 1);
    assert_eq!(fsm.initial_exit_cnt, 0);
    assert_eq!(fsm.done_enter_cnt, 0);
    assert_eq!(fsm.done_cnt, 1);
    assert_eq!(fsm.done_exit_cnt, 0);
}

#[test]
fn test_enter() {
    hsm1!(
        struct Test {
            initial_enter_cnt: usize,
            initial_cnt: usize,
            initial_exit_cnt: usize,
            done_enter_cnt: usize,
            done_cnt: usize,
            done_exit_cnt: usize,
        }

        fn initial_enter(&mut self, _msg: &NoMessages) {
            println!("test_enter: initial_enter");
            self.initial_enter_cnt += 1;
        }

        #[hsm1_state]
        // This state has hdl 0
        fn initial(&mut self, _msg: &NoMessages) -> StateResult {
            println!("test_enter: initial");
            self.initial_cnt += 1;
            StateResult::TransitionTo(1usize) //Test::done)
        }

        #[hsm1_state]
        // This state has hdl 1
        fn done(&mut self, _msg: &NoMessages) -> StateResult {
            println!("test_enter: done");
            self.done_cnt += 1;
            StateResult::Handled
        }

        fn done_enter(&mut self, _msg: &NoMessages) {
            println!("test_enter: done_enter");
            self.done_enter_cnt += 1;
        }
    );

    let mut fsm = Test::new();
    assert_eq!(fsm.initial_enter_cnt, 0);
    assert_eq!(fsm.initial_cnt, 0);
    assert_eq!(fsm.initial_exit_cnt, 0);
    assert_eq!(fsm.done_enter_cnt, 0);
    assert_eq!(fsm.done_cnt, 0);
    assert_eq!(fsm.done_exit_cnt, 0);

    fsm.dispatch(&NoMessages);
    assert_eq!(fsm.initial_enter_cnt, 1);
    assert_eq!(fsm.initial_cnt, 1);
    assert_eq!(fsm.initial_exit_cnt, 0);
    assert_eq!(fsm.done_enter_cnt, 0);
    assert_eq!(fsm.done_cnt, 0);
    assert_eq!(fsm.done_exit_cnt, 0);

    fsm.dispatch(&NoMessages);
    assert_eq!(fsm.initial_enter_cnt, 1);
    assert_eq!(fsm.initial_cnt, 1);
    assert_eq!(fsm.initial_exit_cnt, 0);
    assert_eq!(fsm.done_enter_cnt, 1);
    assert_eq!(fsm.done_cnt, 1);
    assert_eq!(fsm.done_exit_cnt, 0);
}

#[test]
fn test_exit() {
    hsm1!(
        struct Test {
            initial_enter_cnt: usize,
            initial_cnt: usize,
            initial_exit_cnt: usize,
            done_enter_cnt: usize,
            done_cnt: usize,
            done_exit_cnt: usize,
        }

        #[hsm1_state]
        // This state has hdl 0
        fn initial(&mut self, _msg: &NoMessages) -> StateResult {
            self.initial_cnt += 1;
            StateResult::TransitionTo(1usize) //Test::done)
        }

        fn initial_exit(&mut self, _msg: &NoMessages) {
            self.initial_exit_cnt += 1;
        }

        fn done_exit(&mut self, _msg: &NoMessages) {
            self.done_exit_cnt += 1;
        }

        #[hsm1_state]
        // This state has hdl 1
        fn done(&mut self, _msg: &NoMessages) -> StateResult {
            self.done_cnt += 1;
            StateResult::Handled
        }
    );

    let mut fsm = Test::new();
    assert_eq!(fsm.initial_enter_cnt, 0);
    assert_eq!(fsm.initial_cnt, 0);
    assert_eq!(fsm.initial_exit_cnt, 0);
    assert_eq!(fsm.done_enter_cnt, 0);
    assert_eq!(fsm.done_cnt, 0);
    assert_eq!(fsm.done_exit_cnt, 0);

    fsm.dispatch(&NoMessages);
    assert_eq!(fsm.initial_enter_cnt, 0);
    assert_eq!(fsm.initial_cnt, 1);
    assert_eq!(fsm.initial_exit_cnt, 1);
    assert_eq!(fsm.done_enter_cnt, 0);
    assert_eq!(fsm.done_cnt, 0);
    assert_eq!(fsm.done_exit_cnt, 0);

    fsm.dispatch(&NoMessages);
    assert_eq!(fsm.initial_enter_cnt, 0);
    assert_eq!(fsm.initial_cnt, 1);
    assert_eq!(fsm.initial_exit_cnt, 1);
    assert_eq!(fsm.done_enter_cnt, 0);
    assert_eq!(fsm.done_cnt, 1);
    assert_eq!(fsm.done_exit_cnt, 0);
}

#[test]
fn test_both_enter_exit() {
    hsm1!(
        struct Test {
            initial_enter_cnt: usize,
            initial_cnt: usize,
            initial_exit_cnt: usize,
            do_work_enter_cnt: usize,
            do_work_cnt: usize,
            do_work_exit_cnt: usize,
            done_enter_cnt: usize,
            done_cnt: usize,
            done_exit_cnt: usize,
        }

        fn initial_enter(&mut self, _msg: &NoMessages) {
            self.initial_enter_cnt += 1;
        }

        #[hsm1_state]
        // This state has hdl 0
        fn initial(&mut self, _msg: &NoMessages) -> StateResult {
            self.initial_cnt += 1;
            StateResult::TransitionTo(1) //Test::do_work)
        }

        fn initial_exit(&mut self, _msg: &NoMessages) {
            self.initial_exit_cnt += 1;
        }

        fn do_work_exit(&mut self, _msg: &NoMessages) {
            self.do_work_exit_cnt += 1;
        }

        #[hsm1_state]
        // This state has hdl 1
        fn do_work(&mut self, _msg: &NoMessages) -> StateResult {
            self.do_work_cnt += 1;
            if self.do_work_cnt < 3 {
                StateResult::Handled
            } else {
                StateResult::TransitionTo(2) //Test::done
            }
        }

        fn do_work_enter(&mut self, _msg: &NoMessages) {
            self.do_work_enter_cnt += 1;
        }

        fn done_exit(&mut self, _msg: &NoMessages) {
            self.done_exit_cnt += 1;
        }

        #[hsm1_state]
        // This state has hdl 2
        fn done(&mut self, _msg: &NoMessages) -> StateResult {
            self.done_cnt += 1;
            StateResult::Handled
        }

        fn done_enter(&mut self, _msg: &NoMessages) {
            self.done_enter_cnt += 1;
        }
    );

    let mut fsm = Test::new();
    assert_eq!(fsm.initial_enter_cnt, 0);
    assert_eq!(fsm.initial_cnt, 0);
    assert_eq!(fsm.initial_exit_cnt, 0);
    assert_eq!(fsm.do_work_enter_cnt, 0);
    assert_eq!(fsm.do_work_cnt, 0);
    assert_eq!(fsm.do_work_exit_cnt, 0);
    assert_eq!(fsm.done_enter_cnt, 0);
    assert_eq!(fsm.done_cnt, 0);
    assert_eq!(fsm.done_exit_cnt, 0);

    fsm.dispatch(&NoMessages);
    assert_eq!(fsm.initial_enter_cnt, 1);
    assert_eq!(fsm.initial_cnt, 1);
    assert_eq!(fsm.initial_exit_cnt, 1);
    assert_eq!(fsm.do_work_enter_cnt, 0);
    assert_eq!(fsm.do_work_cnt, 0);
    assert_eq!(fsm.do_work_exit_cnt, 0);
    assert_eq!(fsm.done_enter_cnt, 0);
    assert_eq!(fsm.done_cnt, 0);
    assert_eq!(fsm.done_exit_cnt, 0);

    fsm.dispatch(&NoMessages);
    assert_eq!(fsm.initial_enter_cnt, 1);
    assert_eq!(fsm.initial_cnt, 1);
    assert_eq!(fsm.initial_exit_cnt, 1);
    assert_eq!(fsm.do_work_enter_cnt, 1);
    assert_eq!(fsm.do_work_cnt, 1);
    assert_eq!(fsm.do_work_exit_cnt, 0);
    assert_eq!(fsm.done_enter_cnt, 0);
    assert_eq!(fsm.done_cnt, 0);
    assert_eq!(fsm.done_exit_cnt, 0);

    fsm.dispatch(&NoMessages);
    assert_eq!(fsm.initial_enter_cnt, 1);
    assert_eq!(fsm.initial_cnt, 1);
    assert_eq!(fsm.initial_exit_cnt, 1);
    assert_eq!(fsm.do_work_enter_cnt, 1);
    assert_eq!(fsm.do_work_cnt, 2);
    assert_eq!(fsm.do_work_exit_cnt, 0);
    assert_eq!(fsm.done_enter_cnt, 0);
    assert_eq!(fsm.done_cnt, 0);
    assert_eq!(fsm.done_exit_cnt, 0);

    fsm.dispatch(&NoMessages);
    assert_eq!(fsm.initial_enter_cnt, 1);
    assert_eq!(fsm.initial_cnt, 1);
    assert_eq!(fsm.initial_exit_cnt, 1);
    assert_eq!(fsm.do_work_enter_cnt, 1);
    assert_eq!(fsm.do_work_cnt, 3);
    assert_eq!(fsm.do_work_exit_cnt, 1);
    assert_eq!(fsm.done_enter_cnt, 0);
    assert_eq!(fsm.done_cnt, 0);
    assert_eq!(fsm.done_exit_cnt, 0);

    fsm.dispatch(&NoMessages);
    assert_eq!(fsm.initial_enter_cnt, 1);
    assert_eq!(fsm.initial_cnt, 1);
    assert_eq!(fsm.initial_exit_cnt, 1);
    assert_eq!(fsm.do_work_enter_cnt, 1);
    assert_eq!(fsm.do_work_cnt, 3);
    assert_eq!(fsm.do_work_exit_cnt, 1);
    assert_eq!(fsm.done_enter_cnt, 1);
    assert_eq!(fsm.done_cnt, 1);
    assert_eq!(fsm.done_exit_cnt, 0);

    fsm.dispatch(&NoMessages);
    assert_eq!(fsm.initial_enter_cnt, 1);
    assert_eq!(fsm.initial_cnt, 1);
    assert_eq!(fsm.initial_exit_cnt, 1);
    assert_eq!(fsm.do_work_enter_cnt, 1);
    assert_eq!(fsm.do_work_cnt, 3);
    assert_eq!(fsm.do_work_exit_cnt, 1);
    assert_eq!(fsm.done_enter_cnt, 1);
    assert_eq!(fsm.done_cnt, 2);
    assert_eq!(fsm.done_exit_cnt, 0);
}

#[test]
fn test_parent() {
    hsm1!(
        struct Test {
            parent_enter_cnt: usize,
            parent_cnt: usize,
            parent_exit_cnt: usize,
            initial_enter_cnt: usize,
            initial_cnt: usize,
            initial_exit_cnt: usize,
        }

        #[hsm1_state]
        // This state has hdl 0
        fn parent(&mut self, _msg: &NoMessages) -> StateResult {
            self.parent_cnt += 1;
            handled!()
        }

        #[hsm1_state(parent)]
        // This state has hdl 1
        fn initial(&mut self, _msg: &NoMessages) -> StateResult {
            self.initial_cnt += 1;
            not_handled!()
        }
    );

    let mut hsm = Test::new();
    assert_eq!(hsm.parent_enter_cnt, 0);
    assert_eq!(hsm.parent_cnt, 0);
    assert_eq!(hsm.parent_exit_cnt, 0);
    assert_eq!(hsm.initial_enter_cnt, 0);
    assert_eq!(hsm.initial_cnt, 0);
    assert_eq!(hsm.initial_exit_cnt, 0);

    hsm.dispatch(&NoMessages);
    assert_eq!(hsm.parent_enter_cnt, 0);
    assert_eq!(hsm.parent_cnt, 1);
    assert_eq!(hsm.parent_exit_cnt, 0);
    assert_eq!(hsm.initial_enter_cnt, 0);
    assert_eq!(hsm.initial_cnt, 1);
    assert_eq!(hsm.initial_exit_cnt, 0);
}

#[test]
fn test_parent_with_enter_exit() {
    hsm1!(
        struct Test {
            parent_enter_cnt: usize,
            parent_cnt: usize,
            parent_exit_cnt: usize,
            initial_enter_cnt: usize,
            initial_cnt: usize,
            initial_exit_cnt: usize,
        }

        fn parent_enter(&mut self, _msg: &NoMessages) {
            self.parent_enter_cnt += 1;
        }

        #[hsm1_state]
        // This state has hdl 0
        fn parent(&mut self, _msg: &NoMessages) -> StateResult {
            self.parent_cnt += 1;
            handled!()
        }

        fn parent_exit(&mut self, _msg: &NoMessages) {
            self.parent_exit_cnt += 1;
        }

        #[hsm1_state(parent)]
        // This state has hdl 1
        fn initial(&mut self, _msg: &NoMessages) -> StateResult {
            self.initial_cnt += 1;
            not_handled!()
        }
    );

    let mut hsm = Test::new();
    assert_eq!(hsm.parent_enter_cnt, 0);
    assert_eq!(hsm.parent_cnt, 0);
    assert_eq!(hsm.parent_exit_cnt, 0);
    assert_eq!(hsm.initial_enter_cnt, 0);
    assert_eq!(hsm.initial_cnt, 0);
    assert_eq!(hsm.initial_exit_cnt, 0);

    hsm.dispatch(&NoMessages);
    assert_eq!(hsm.parent_enter_cnt, 1);
    assert_eq!(hsm.parent_cnt, 1);
    assert_eq!(hsm.parent_exit_cnt, 0);
    assert_eq!(hsm.initial_enter_cnt, 0);
    assert_eq!(hsm.initial_cnt, 1);
    assert_eq!(hsm.initial_exit_cnt, 0);
}

#[test]
fn test_one_tree() {
    hsm1!(
        struct Test {
            parent_enter_cnt: usize,
            parent_cnt: usize,
            parent_exit_cnt: usize,
            initial_enter_cnt: usize,
            initial_cnt: usize,
            initial_exit_cnt: usize,
            do_work_enter_cnt: usize,
            do_work_cnt: usize,
            do_work_exit_cnt: usize,
            done_enter_cnt: usize,
            done_cnt: usize,
            done_exit_cnt: usize,
        }

        fn parent_enter(&mut self, _msg: &NoMessages) {
            self.parent_enter_cnt += 1;
        }

        #[hsm1_state]
        // This state has hdl 0
        fn parent(&mut self, _msg: &NoMessages) -> StateResult {
            self.parent_cnt += 1;
            handled!()
        }

        fn parent_exit(&mut self, _msg: &NoMessages) {
            self.parent_exit_cnt += 1;
        }

        fn initial_enter(&mut self, _msg: &NoMessages) {
            self.initial_enter_cnt += 1;
        }

        #[hsm1_state(parent)]
        // This state has hdl 1
        fn initial(&mut self, _msg: &NoMessages) -> StateResult {
            self.initial_cnt += 1;
            match self.initial_cnt {
                1 => not_handled!(),
                2 => handled!(),
                _ => transition_to!(do_work),
            }
        }

        fn initial_exit(&mut self, _msg: &NoMessages) {
            self.initial_exit_cnt += 1;
        }

        fn do_work_enter(&mut self, _msg: &NoMessages) {
            self.do_work_enter_cnt += 1;
        }

        #[hsm1_state(parent)]
        // This state has hdl 2
        fn do_work(&mut self, _msg: &NoMessages) -> StateResult {
            self.do_work_cnt += 1;
            match self.do_work_cnt {
                1 => handled!(),
                2 => not_handled!(),
                _ => transition_to!(done),
            }
        }

        fn do_work_exit(&mut self, _msg: &NoMessages) {
            self.do_work_exit_cnt += 1;
        }

        fn done_enter(&mut self, _msg: &NoMessages) {
            self.done_enter_cnt += 1;
        }

        #[hsm1_state(parent)]
        // This state has hdl 3
        fn done(&mut self, _msg: &NoMessages) -> StateResult {
            self.done_cnt += 1;
            transition_to!(parent)
        }

        fn done_exit(&mut self, _msg: &NoMessages) {
            self.done_exit_cnt += 1;
        }
    );

    let mut hsm = Test::new();
    assert_eq!(hsm.parent_enter_cnt, 0);
    assert_eq!(hsm.parent_cnt, 0);
    assert_eq!(hsm.parent_exit_cnt, 0);
    assert_eq!(hsm.initial_enter_cnt, 0);
    assert_eq!(hsm.initial_cnt, 0);
    assert_eq!(hsm.initial_exit_cnt, 0);
    assert_eq!(hsm.do_work_enter_cnt, 0);
    assert_eq!(hsm.do_work_cnt, 0);
    assert_eq!(hsm.do_work_exit_cnt, 0);
    assert_eq!(hsm.done_enter_cnt, 0);
    assert_eq!(hsm.done_cnt, 0);
    assert_eq!(hsm.done_exit_cnt, 0);

    // Into initial which returned not_handled!()
    hsm.dispatch(&NoMessages);
    assert_eq!(hsm.parent_enter_cnt, 1);
    assert_eq!(hsm.parent_cnt, 1);
    assert_eq!(hsm.parent_exit_cnt, 0);
    assert_eq!(hsm.initial_enter_cnt, 1);
    assert_eq!(hsm.initial_cnt, 1);
    assert_eq!(hsm.initial_exit_cnt, 0);
    assert_eq!(hsm.do_work_enter_cnt, 0);
    assert_eq!(hsm.do_work_cnt, 0);
    assert_eq!(hsm.do_work_exit_cnt, 0);
    assert_eq!(hsm.done_enter_cnt, 0);
    assert_eq!(hsm.done_cnt, 0);
    assert_eq!(hsm.done_exit_cnt, 0);

    // In initial which returned handled!()
    hsm.dispatch(&NoMessages);
    assert_eq!(hsm.parent_enter_cnt, 1);
    assert_eq!(hsm.parent_cnt, 1);
    assert_eq!(hsm.parent_exit_cnt, 0);
    assert_eq!(hsm.initial_enter_cnt, 1);
    assert_eq!(hsm.initial_cnt, 2);
    assert_eq!(hsm.initial_exit_cnt, 0);
    assert_eq!(hsm.do_work_enter_cnt, 0);
    assert_eq!(hsm.do_work_cnt, 0);
    assert_eq!(hsm.do_work_exit_cnt, 0);
    assert_eq!(hsm.done_enter_cnt, 0);
    assert_eq!(hsm.done_cnt, 0);
    assert_eq!(hsm.done_exit_cnt, 0);

    // In initial which returned transition_to!(do_work)
    hsm.dispatch(&NoMessages);
    assert_eq!(hsm.parent_enter_cnt, 1);
    assert_eq!(hsm.parent_cnt, 1);
    assert_eq!(hsm.parent_exit_cnt, 0);
    assert_eq!(hsm.initial_enter_cnt, 1);
    assert_eq!(hsm.initial_cnt, 3);
    assert_eq!(hsm.initial_exit_cnt, 1);
    assert_eq!(hsm.do_work_enter_cnt, 0);
    assert_eq!(hsm.do_work_cnt, 0);
    assert_eq!(hsm.do_work_exit_cnt, 0);
    assert_eq!(hsm.done_enter_cnt, 0);
    assert_eq!(hsm.done_cnt, 0);
    assert_eq!(hsm.done_exit_cnt, 0);

    // Into do_work returned handled!()
    hsm.dispatch(&NoMessages);
    assert_eq!(hsm.parent_enter_cnt, 1);
    assert_eq!(hsm.parent_cnt, 1);
    assert_eq!(hsm.parent_exit_cnt, 0);
    assert_eq!(hsm.initial_enter_cnt, 1);
    assert_eq!(hsm.initial_cnt, 3);
    assert_eq!(hsm.initial_exit_cnt, 1);
    assert_eq!(hsm.do_work_enter_cnt, 1);
    assert_eq!(hsm.do_work_cnt, 1);
    assert_eq!(hsm.do_work_exit_cnt, 0);
    assert_eq!(hsm.done_enter_cnt, 0);
    assert_eq!(hsm.done_cnt, 0);
    assert_eq!(hsm.done_exit_cnt, 0);

    // In do_work returned not_handled!()
    hsm.dispatch(&NoMessages);
    assert_eq!(hsm.parent_enter_cnt, 1);
    assert_eq!(hsm.parent_cnt, 2);
    assert_eq!(hsm.parent_exit_cnt, 0);
    assert_eq!(hsm.initial_enter_cnt, 1);
    assert_eq!(hsm.initial_cnt, 3);
    assert_eq!(hsm.initial_exit_cnt, 1);
    assert_eq!(hsm.do_work_enter_cnt, 1);
    assert_eq!(hsm.do_work_cnt, 2);
    assert_eq!(hsm.do_work_exit_cnt, 0);
    assert_eq!(hsm.done_enter_cnt, 0);
    assert_eq!(hsm.done_cnt, 0);
    assert_eq!(hsm.done_exit_cnt, 0);

    // In do_work returned transition_to!(done)
    hsm.dispatch(&NoMessages);
    assert_eq!(hsm.parent_enter_cnt, 1);
    assert_eq!(hsm.parent_cnt, 2);
    assert_eq!(hsm.parent_exit_cnt, 0);
    assert_eq!(hsm.initial_enter_cnt, 1);
    assert_eq!(hsm.initial_cnt, 3);
    assert_eq!(hsm.initial_exit_cnt, 1);
    assert_eq!(hsm.do_work_enter_cnt, 1);
    assert_eq!(hsm.do_work_cnt, 3);
    assert_eq!(hsm.do_work_exit_cnt, 1);
    assert_eq!(hsm.done_enter_cnt, 0);
    assert_eq!(hsm.done_cnt, 0);
    assert_eq!(hsm.done_exit_cnt, 0);

    // Into done always returns transition_to!(parent)
    hsm.dispatch(&NoMessages);
    assert_eq!(hsm.parent_enter_cnt, 1);
    assert_eq!(hsm.parent_cnt, 2);
    assert_eq!(hsm.parent_exit_cnt, 1);
    assert_eq!(hsm.initial_enter_cnt, 1);
    assert_eq!(hsm.initial_cnt, 3);
    assert_eq!(hsm.initial_exit_cnt, 1);
    assert_eq!(hsm.do_work_enter_cnt, 1);
    assert_eq!(hsm.do_work_cnt, 3);
    assert_eq!(hsm.do_work_exit_cnt, 1);
    assert_eq!(hsm.done_enter_cnt, 1);
    assert_eq!(hsm.done_cnt, 1);
    assert_eq!(hsm.done_exit_cnt, 1);

    // Into parent always returns handled!()
    hsm.dispatch(&NoMessages);
    assert_eq!(hsm.parent_enter_cnt, 2);
    assert_eq!(hsm.parent_cnt, 3);
    assert_eq!(hsm.parent_exit_cnt, 1);
    assert_eq!(hsm.initial_enter_cnt, 1);
    assert_eq!(hsm.initial_cnt, 3);
    assert_eq!(hsm.initial_exit_cnt, 1);
    assert_eq!(hsm.do_work_enter_cnt, 1);
    assert_eq!(hsm.do_work_cnt, 3);
    assert_eq!(hsm.do_work_exit_cnt, 1);
    assert_eq!(hsm.done_enter_cnt, 1);
    assert_eq!(hsm.done_cnt, 1);
    assert_eq!(hsm.done_exit_cnt, 1);

    // Into parent always returns handled!()
    hsm.dispatch(&NoMessages);
    assert_eq!(hsm.parent_enter_cnt, 2);
    assert_eq!(hsm.parent_cnt, 4);
    assert_eq!(hsm.parent_exit_cnt, 1);
    assert_eq!(hsm.initial_enter_cnt, 1);
    assert_eq!(hsm.initial_cnt, 3);
    assert_eq!(hsm.initial_exit_cnt, 1);
    assert_eq!(hsm.do_work_enter_cnt, 1);
    assert_eq!(hsm.do_work_cnt, 3);
    assert_eq!(hsm.do_work_exit_cnt, 1);
    assert_eq!(hsm.done_enter_cnt, 1);
    assert_eq!(hsm.done_cnt, 1);
    assert_eq!(hsm.done_exit_cnt, 1);
}
