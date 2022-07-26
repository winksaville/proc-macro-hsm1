#![feature(no_coverage)]

use std::{
    cell::RefCell,
    collections::VecDeque,
    fmt::Debug,
    sync::mpsc::{Receiver, RecvError, SendError, Sender, TryRecvError},
};

pub type DynError = Box<dyn std::error::Error>;
type ProcessFn<SM, P> = fn(&mut SM, &Executor<SM, P>, &P) -> StateResult;
type EnterFn<SM, P> = fn(&mut SM, &P);
type ExitFn<SM, P> = fn(&mut SM, &P);

pub enum Handled {
    Yes,
    No,
}

pub type Transition = usize;

pub type StateResult = (Handled, Option<Transition>);

//#[derive(Clone)]
pub struct StateInfo<SM, P> {
    pub name: String,
    pub parent: Option<usize>,
    pub enter: Option<EnterFn<SM, P>>,
    pub process: ProcessFn<SM, P>,
    pub exit: Option<ExitFn<SM, P>>,
    pub active: bool,
    pub children_for_cycle_detector: Vec<usize>,
    pub enter_cnt: usize,
    pub process_cnt: usize,
    pub exit_cnt: usize,
}

impl<SM, P> StateInfo<SM, P> {
    pub fn new(name: &str, process_fn: ProcessFn<SM, P>) -> Self {
        StateInfo {
            name: name.to_owned(),
            parent: None,
            enter: None,
            process: process_fn,
            exit: None,
            active: false,
            children_for_cycle_detector: Vec::<usize>::new(),
            enter_cnt: 0,
            process_cnt: 0,
            exit_cnt: 0,
        }
    }

    pub fn enter_fn(mut self, enter_fn: EnterFn<SM, P>) -> Self {
        self.enter = Some(enter_fn);

        self
    }

    pub fn exit_fn(mut self, exit_fn: EnterFn<SM, P>) -> Self {
        self.exit = Some(exit_fn);

        self
    }

    pub fn parent_idx(mut self, idx_parent: usize) -> Self {
        self.parent = Some(idx_parent);

        self
    }
}

pub struct Executor<SM, P> {
    //pub name: String, // TODO: add StateMachineInfo::name

    // Field `sm` needs "interior mutability" because we pass &mut sm and &Self
    // to process in dispatch_idx. If we don't have `sm` as a RefCell
    // we get the following error at the call site in dispatch_idx:
    //     (self.states[idx].process)(&mut self.sm, self, msg);
    //     -------------------------- ------------  ^^^^ immutable borrow occurs here
    //     |                          |
    //     |                          mutable borrow occurs here
    //     mutable borrow later used by call
    pub sm: RefCell<SM>,

    pub states: Vec<StateInfo<SM, P>>,
    pub current_state_changed: bool,
    pub idx_transition_dest: Option<usize>,
    pub idx_current_state: usize,
    pub idx_previous_state: usize,
    pub idxs_enter_fns: Vec<usize>,
    pub idxs_exit_fns: std::collections::VecDeque<usize>,

    // These are leaf states, i.e. states with no children
    pub transition_targets: Vec<usize>,

    // Returns `true` if array idx is in transition_targets
    pub transition_targets_set: Vec<bool>,

    // Defer support
    primary_tx: Sender<P>,
    primary_rx: Receiver<P>,
    defer_tx: [Sender<P>; 2],
    defer_rx: [Receiver<P>; 2],
    current_defer_idx: usize,
}

impl<SM, P> Executor<SM, P>
where
    SM: Debug,
    P: Debug,
{
    // Begin building an executor.
    //
    // You must call add_state to add one or more states
    pub fn new(sm: RefCell<SM>, max_states: usize) -> Self {
        let (primary_tx, primary_rx) = std::sync::mpsc::channel::<P>();
        let (defer0_tx, defer0_rx) = std::sync::mpsc::channel::<P>();
        let (defer1_tx, defer1_rx) = std::sync::mpsc::channel::<P>();

        Executor {
            sm,
            states: Vec::<StateInfo<SM, P>>::with_capacity(max_states),
            current_state_changed: true,
            idx_transition_dest: None,
            idx_current_state: 0,
            idx_previous_state: 0,
            idxs_enter_fns: Vec::<usize>::with_capacity(max_states),
            idxs_exit_fns: VecDeque::<usize>::with_capacity(max_states),
            transition_targets: Vec::<usize>::with_capacity(max_states),
            transition_targets_set: Vec::<bool>::with_capacity(max_states),
            primary_tx,
            primary_rx,
            defer_tx: [defer0_tx, defer1_tx],
            defer_rx: [defer0_rx, defer1_rx],
            current_defer_idx: 0,
        }
    }

    // Add a state to the the executor
    pub fn state(mut self, state_info: StateInfo<SM, P>) -> Self {
        self.states.push(state_info);

        self
    }

    // Initialize and make the executor ready to dispatch messages.
    //
    // The first state will be the state at idx_initial_state
    pub fn build(mut self, idx_initial_state: usize) -> Result<Self, DynError> {
        // Initialize StateInfo.children_for_cycle_dector for each state
        self.initialize_children();

        // Initialize transition_targets_set to false
        for _ in 0..self.states.len() {
            self.transition_targets_set.push(false);
        }

        // Initialize transition_targets and transition_targets_set
        for idx in 0..self.states.len() {
            let cur_state = &mut self.states[idx];

            if cur_state.children_for_cycle_detector.is_empty() {
                self.transition_targets.push(idx);
                self.transition_targets_set[idx] = true;
            }
        }
        //println!("transition_targets: {:?}", self.transition_targets);
        //println!("transition_targets_set: {:?}", self.transition_targets_set);

        if self.cycle_detector() {
            return Err("Cycle detected".into());
        }

        // Validate idx_initial_state is valid.
        if idx_initial_state >= self.states.len() || !self.transition_targets_set[idx_initial_state]
        {
            panic!(
                "{idx_initial_state} is not a valid initial state, only {:?} are allowed",
                self.transition_targets
            );
        }

        // Initialize current and previuos state to initial state
        self.idx_current_state = idx_initial_state;
        self.idx_previous_state = idx_initial_state;

        // Initialize the idx_enter_fns array, start by
        // always pushing the destination
        let mut idx_enter = self.idx_current_state;
        //log::trace!("initialialize: push idx_enter={} {}", idx_enter, self.state_name(idx_enter));
        self.idxs_enter_fns.push(idx_enter);

        // Then push parents of the destination state so they are also entered.
        while let Some(idx) = self.states[idx_enter].parent {
            idx_enter = idx;

            //log::trace!("initialialize: push idx_enter={} {}", idx_enter, self.state_name(idx_enter));
            self.idxs_enter_fns.push(idx_enter);
        }

        Ok(self)
    }

    // Kahns algorithm for detecting cycles using a Breath First Search
    //   https://www.geeksforgeeks.org/detect-cycle-in-a-directed-graph-using-bfs/
    fn cycle_detector(&mut self) -> bool {
        let mut leafs = self.transition_targets.to_vec();
        //println!("cycle_dector: leafs: {leafs:?}");

        let mut visited_cnt = 0usize;
        while let Some(leaf_idx) = leafs.pop() {
            visited_cnt += 1;
            //println!("cycle_dector: leaf_idx={leaf_idx} visited_cnt={visited_cnt}");

            // Check if we have an "edge"
            if let Some(parent_idx) = self.states[leaf_idx].parent {
                // Yes, reference to that parent
                let parent_state = &mut self.states[parent_idx];

                // We need to remove the edge from leaf to parent, we'll do
                // that by creating other_children which will be children_for_cycle_dector
                // but with the "leaf_idx" removed.
                let mut other_children = Vec::<usize>::new();
                for child_idx in 0..parent_state.children_for_cycle_detector.len() {
                    if parent_state.children_for_cycle_detector[child_idx] != leaf_idx {
                        // This isn't the leaf index so save it in other_children
                        other_children.push(parent_state.children_for_cycle_detector[child_idx]);
                    }
                }

                if other_children.is_empty() {
                    // There are NO other_children so the parent_idx is now a leaf
                    leafs.push(parent_idx);
                    //println!("cycle_dector: add new leaf {parent_idx} leafs: {leafs:?}");
                } else {
                    // Thre are other_children so copy it to children_for_cycle_dector
                    //println!("cycle_dector: states[{parent_idx}] other_children: {other_children:?}");
                    parent_state.children_for_cycle_detector = other_children.to_vec();
                }
            }
        }
        //println!("cycle_dector: visited_cnt: {visited_cnt} state.len()={}", self.states.len());

        visited_cnt != self.states.len()
    }

    // Determine Transition targets, (states with no children aka leafs)
    fn initialize_children(&mut self) {
        for idx in 0..self.states.len() {
            self.initialize_states_children(idx);
            //println!( "{idx:3}: {} {:?}", self.states[idx].children_for_cycle_detector.len(), self.states[idx].children_for_cycle_detector);
        }
    }

    fn initialize_states_children(&mut self, cur_state_idx: usize) {
        // Itereate over all of the states looking for nodes that point to cur_state_idx
        for idx in 0..self.states.len() {
            if self.states[idx].parent == Some(cur_state_idx) {
                // Add a child state
                self.states[cur_state_idx]
                    .children_for_cycle_detector
                    .push(idx);
            }
        }
    }

    pub fn get_state_name(&self, idx: usize) -> &str {
        &self.states[idx].name
    }

    pub fn get_current_state_name(&self) -> &str {
        self.get_state_name(self.idx_current_state)
    }

    pub fn get_sm(&self) -> &RefCell<SM> {
        &self.sm
    }

    pub fn get_state_enter_cnt(&self, idx: usize) -> usize {
        self.states[idx].enter_cnt
    }
    pub fn get_state_process_cnt(&self, idx: usize) -> usize {
        self.states[idx].process_cnt
    }

    pub fn get_state_exit_cnt(&self, idx: usize) -> usize {
        self.states[idx].exit_cnt
    }

    fn setup_exit_enter_fns_idxs(&mut self, idx_next_state: usize) {
        let mut cur_idx = idx_next_state;

        // Setup the enter vector
        let exit_sentinel = loop {
            //log::trace!("setup_exit_enter_fns_idxs: cur_idx={} {}, TOL", cur_idx, self.state_name(cur_idx));
            self.idxs_enter_fns.push(cur_idx);

            cur_idx = if let Some(idx) = self.states[cur_idx].parent {
                idx
            } else {
                // Exit state_infos[self.current_state_infos_idx] and all its parents
                //log::trace!("setup_exit_enter_fns_idxs: cur_idx={} {} has no parent exit_sentinel=None", cur_dx, self.state_name(cur_idx));
                break None;
            };

            if self.states[cur_idx].active {
                // Exit state_infos[self.current_state_infos_idx] and
                // parents upto but excluding state_infos[cur_idx]
                //log::trace!("setup_exit_enter_fns_idxs: cur_idx={} {} is active so it's exit_sentinel", cur_idx, self.state_name(cur_idx));
                break Some(cur_idx);
            }
        };

        // Starting at self.idx_current_state generate the
        // list of StateFns that we're going to exit. If exit_sentinel is None
        // then exit from idx_current_state and all of its parents.
        // If exit_sentinel is Some then exit from the idx_current_state
        // up to but not including the exit_sentinel.
        let mut idx_exit = self.idx_current_state;

        // Always exit the first state, this handles the special case
        // where Some(idx_exit) == exit_sentinel and we need to exit anyway.
        //log::trace!("setup_exit_enter_fns_idxs: push_back(idx_exit={} {})", idx_exit, self.state_name(idx_exit));
        self.idxs_exit_fns.push_back(idx_exit);

        while let Some(idx) = self.states[idx_exit].parent {
            idx_exit = idx;

            if Some(idx_exit) == exit_sentinel {
                // Reached the exit sentinel so we're done
                //log::trace!("setup_exit_enter_fns_idxs: idx_exit={} {} == exit_sentinel={} {}, reached exit_sentinel return", idx_exit, self.state_name(idx_exit), exit_sentinel.unwrap(), self.state_name(exit_sentinel.unwrap()));
                return;
            }

            //log::trace!( "setup_exit_enter_fns_idxs: push_back(idx_exit={} {})", idx_exit, self.state_name(idx_exit));
            self.idxs_exit_fns.push_back(idx_exit);
        }
    }

    pub fn dispatch_idx(&mut self, msg: &P, idx: usize) {
        //log::trace!("dispatch_idx:+ idx={} {}", idx, self.state_name(idx));

        if self.current_state_changed {
            // Execute the enter functions
            while let Some(idx_enter) = self.idxs_enter_fns.pop() {
                if let Some(state_enter) = self.states[idx_enter].enter {
                    //log::trace!("dispatch_idx: entering idx={} {}", idx_enter, self.state_name(idx_enter));
                    self.states[idx_enter].enter_cnt += 1;
                    (state_enter)(&mut self.sm.borrow_mut(), msg);
                    self.states[idx_enter].active = true;
                }
            }
            self.current_state_changed = false;
        }

        // Invoke the current state funtion processing the result
        //log::trace!("dispatch_idx: processing idx={} {}", idx, self.state_name(idx));

        self.states[idx].process_cnt += 1;
        let (handled, transition) =
            (self.states[idx].process)(&mut self.sm.borrow_mut(), self, msg);
        if let Some(idx_next_state) = transition {
            if self.idx_transition_dest.is_none() {
                // First Transition it will be the idx_transition_dest
                self.idx_transition_dest = Some(idx_next_state);
            }
        }
        match handled {
            Handled::No => {
                if let Some(idx_parent) = self.states[idx].parent {
                    //log::trace!("dispatch_idx: idx={} {} NotHandled, recurse into dispatch_idx", idx, self.state_name(idx));
                    self.dispatch_idx(msg, idx_parent);
                }
                //} else {
                //    log::trace!("dispatch_idx: idx={} {}, NotHandled, no parent, ignoring messages", idx, self.state_name(idx));
                //}
            }
            Handled::Yes => {
                // Nothing to do
                //log::trace!("dispatch_idx: idx={} {} Handled", idx, self.state_name(idx));
            }
        }

        if let Some(idx_next_state) = self.idx_transition_dest {
            self.idx_transition_dest = None;
            if idx_next_state < self.states.len() && self.transition_targets_set[idx_next_state] {
                //log::trace!("dispatch_idx: transition_to idx={} {}", idx_next_state, self.state_name(idx_next_state));
                self.setup_exit_enter_fns_idxs(idx_next_state);

                self.idx_previous_state = self.idx_current_state;
                self.idx_current_state = idx_next_state;
                self.current_state_changed = true;
            } else {
                panic!(
                    "{idx_next_state} is not a valid transition target, only {:?} are allowed",
                    self.transition_targets
                );
            }
        }

        if self.current_state_changed {
            while let Some(idx_exit) = self.idxs_exit_fns.pop_front() {
                if let Some(state_exit) = self.states[idx_exit].exit {
                    //log::trace!("dispatch_idx: exiting idx={} {}", idx_exit, self.state_name(idx_exit));
                    self.states[idx_exit].exit_cnt += 1;
                    (state_exit)(&mut self.sm.borrow_mut(), msg);
                    self.states[idx_exit].active = false;
                }
            }
        }

        //log::trace!("dispatch_idx:- idx={} {}", idx, self.state_name(idx));
    }

    pub fn dispatch(&mut self, msg: &P) -> bool {
        //log::trace!( "dispatch:+ current_state_infos_idx={} {}", self.idx_current_state, self.current_state_name());
        self.dispatch_idx(msg, self.idx_current_state);
        //log::trace!( "dispatch:- current_state_infos_idx={} {}", self.idx_current_state, self.current_state_name());

        self.current_state_changed
    }

    // TODO: More testing at warnings are needed that defering messages
    // is "dangerous" and processing time increases for new messages. There
    // maybe other dangers too!
    pub fn dispatcher(&mut self, msg: &P) {
        //log::trace!("dispatcher:+ msg={msg:?} sm={:?}", self.get_sm());
        let mut transitioned = self.dispatch(msg);
        //log::trace!("dispatcher:  msg={msg:?} sm={:?} ret={transitioned}", self.get_sm());

        // Process all deferred messages we if we've transitioned
        // above or within the loop below.
        while transitioned {
            //log::trace!("dispatcher:  TOL transitioned");
            transitioned = false;

            // Switch to next set of deferred messages
            self.next_defer();

            // And process all of them before we do another next_defer().
            // If we didn't do this we could process newly deferred message
            // before we process previously deferred messages. In other words,
            // we guarantee that previously sent messages are always processed
            // before newly sent messages! TODO: add a messge counter or
            // timestamp so we can guarantee this when testing!
            while let Ok(m) = self.defer_try_recv() {
                //log::trace!("dispatcher:  deferred msg={m:?} sm={:?}", self.get_sm());
                transitioned |= self.dispatch(&m);
                //log::trace!("dispatcher:  deferred msg={m:?} sm={:?} ret={transitioned}", self.get_sm());
            }
        }

        // At this point we've processed the incoming message and let
        // the SM reprocessed all deferred messages at least one more
        // time after each subsequent transition.
        //
        // There may still have deferred messages but the SM didn't
        // transition so those will be processed after this fn is
        // called with a new message which causes a transition.

        //log::trace!("dispatcher:- msg={msg:?} sm={:?}", self.get_sm());
    }

    // Defer support
    pub fn recv(&self) -> Result<P, RecvError> {
        self.primary_rx.recv()
    }

    pub fn try_recv(&self) -> Result<P, TryRecvError> {
        self.primary_rx.try_recv()
    }

    pub fn send(&self, m: P) -> Result<(), SendError<P>> {
        self.primary_tx.send(m)
    }

    pub fn clone_sender(&self) -> Sender<P> {
        self.primary_tx.clone()
    }

    pub fn defer_try_recv(&self) -> Result<P, TryRecvError> {
        self.defer_rx[self.other_defer()].try_recv()
    }

    pub fn defer_send(&self, m: P) -> Result<(), SendError<P>> {
        self.defer_tx[self.current_defer()].send(m)
    }

    pub fn next_defer(&mut self) {
        self.current_defer_idx = (self.current_defer_idx + 1) % self.defer_tx.len();
    }

    pub fn current_defer(&self) -> usize {
        self.current_defer_idx
    }

    pub fn other_defer(&self) -> usize {
        (self.current_defer_idx + 1) % self.defer_tx.len()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    // Test SM with one state with one field and no enter or exit
    #[test]
    #[no_coverage]
    fn test_sm_1s_no_enter_no_exit() {
        #[derive(Debug)]
        pub struct StateMachine {
            state: i32,
        }

        // Create a Protocol
        #[derive(Debug)]
        pub struct NoMessages;

        const MAX_STATES: usize = 1;
        const IDX_STATE1: usize = 0;

        impl StateMachine {
            #[no_coverage]
            fn new() -> Executor<Self, NoMessages> {
                let sm = RefCell::new(StateMachine { state: 0 });
                let sme = Executor::new(sm, MAX_STATES)
                    .state(StateInfo::new("state1", Self::state1))
                    .build(IDX_STATE1)
                    .expect("Unexpected error initializing");

                sme
            }

            #[no_coverage]
            fn state1(&mut self, e: &Executor<Self, NoMessages>, _msg: &NoMessages) -> StateResult {
                println!("{}:+", e.get_state_name(IDX_STATE1));

                self.state += 1;

                println!("{}:-", e.get_state_name(IDX_STATE1));
                (Handled::Yes, None)
            }
        }

        // Create a sme and validate it's in the expected state
        let mut sme = StateMachine::new();
        assert_eq!(std::mem::size_of_val(sme.get_sm()), 16);
        assert_eq!(sme.get_state_enter_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_state_exit_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_sm().borrow().state, 0);

        // For code coverage
        println!("{:?}", NoMessages);
        println!("{:?}", sme.get_sm());

        sme.dispatcher(&NoMessages);
        assert_eq!(sme.get_state_enter_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_STATE1), 1);
        assert_eq!(sme.get_state_exit_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_sm().borrow().state, 1);

        sme.dispatcher(&NoMessages);
        assert_eq!(sme.get_state_enter_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_STATE1), 2);
        assert_eq!(sme.get_state_exit_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_sm().borrow().state, 2);
    }

    // Test SM with one state getting names
    #[test]
    #[no_coverage]
    fn test_sm_1s_get_names() {
        #[derive(Debug)]
        pub struct StateMachine {
            state: i32,
        }

        // Create a Protocol
        #[derive(Debug)]
        pub struct NoMessages;

        const MAX_STATES: usize = 1;
        const IDX_STATE1: usize = 0;

        impl StateMachine {
            #[no_coverage]
            fn new() -> Executor<Self, NoMessages> {
                let sm = RefCell::new(StateMachine { state: 0 });
                let sme = Executor::new(sm, MAX_STATES)
                    .state(StateInfo::new("state1", Self::state1))
                    .build(IDX_STATE1)
                    .expect("Unexpected error initializing");

                sme
            }

            #[no_coverage]
            fn state1(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                self.state += 1;

                (Handled::Yes, None)
            }
        }

        // Create a sme and validate it's in the expected state
        let mut sme = StateMachine::new();
        assert_eq!(sme.get_sm().borrow().state, 0);
        assert_eq!(sme.get_state_name(IDX_STATE1), "state1");
        assert_eq!(sme.get_current_state_name(), "state1");

        // For code coverage
        println!("{:?}", NoMessages);
        println!("{:?}", sme.get_sm());

        sme.dispatch(&NoMessages);
        assert_eq!(sme.get_sm().borrow().state, 1);
        assert_eq!(sme.get_state_name(IDX_STATE1), "state1");
        assert_eq!(sme.get_current_state_name(), "state1");

        sme.dispatch(&NoMessages);
        assert_eq!(sme.get_sm().borrow().state, 2);
        assert_eq!(sme.get_state_name(IDX_STATE1), "state1");
        assert_eq!(sme.get_current_state_name(), "state1");
    }

    // Test SM with two states getting names
    #[test]
    #[no_coverage]
    fn test_sm_2s_get_names() {
        #[derive(Debug)]
        pub struct StateMachine {
            state: i32,
        }

        // Create a Protocol
        #[derive(Debug)]
        pub struct NoMessages;

        const MAX_STATES: usize = 2;
        const IDX_STATE1: usize = 0;
        const IDX_STATE2: usize = 1;

        impl StateMachine {
            #[no_coverage]
            fn new() -> Executor<Self, NoMessages> {
                let sm = RefCell::new(StateMachine { state: 0 });
                let sme = Executor::new(sm, MAX_STATES)
                    .state(StateInfo::new("state1", Self::state1))
                    .state(StateInfo::new("state2", Self::state2))
                    .build(IDX_STATE1)
                    .expect("Unexpected error initializing");

                sme
            }

            #[no_coverage]
            fn state1(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                self.state += 1;

                (Handled::Yes, Some(IDX_STATE2))
            }

            #[no_coverage]
            fn state2(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                self.state -= 1;

                (Handled::Yes, Some(IDX_STATE1))
            }
        }

        // Create a sme and validate it's in the expected state
        let mut sme = StateMachine::new();
        assert_eq!(sme.get_sm().borrow().state, 0);
        assert_eq!(sme.get_state_name(IDX_STATE1), "state1");
        assert_eq!(sme.get_current_state_name(), "state1");

        // For code coverage
        println!("{:?}", NoMessages);
        println!("{:?}", sme.get_sm());

        sme.dispatch(&NoMessages);
        assert_eq!(sme.get_sm().borrow().state, 1);
        assert_eq!(sme.get_state_name(IDX_STATE2), "state2");
        assert_eq!(sme.get_current_state_name(), "state2");

        sme.dispatch(&NoMessages);
        assert_eq!(sme.get_sm().borrow().state, 0);
        assert_eq!(sme.get_state_name(IDX_STATE1), "state1");
        assert_eq!(sme.get_current_state_name(), "state1");
    }

    #[test]
    #[no_coverage]
    #[should_panic]
    fn test_sm_out_of_bounds_initial_transition() {
        #[derive(Debug)]
        pub struct StateMachine;

        // Create a Protocol
        #[derive(Debug)]
        pub struct NoMessages;

        const MAX_STATES: usize = 1;
        const IDX_STATE1: usize = 0;
        const INVALID_STATE: usize = 1;

        impl StateMachine {
            #[no_coverage]
            fn new() -> Executor<Self, NoMessages> {
                let sm = RefCell::new(StateMachine);
                let sme = Executor::new(sm, MAX_STATES)
                    .state(StateInfo::new("state1", Self::state1))
                    .build(INVALID_STATE)
                    .expect("Unexpected error initializing");

                sme
            }

            #[no_coverage]
            fn state1(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                // Invalid transition that is not less than MAX_STATES
                (Handled::Yes, Some(1))
            }
        }

        // For code coverage
        println!("{:?}", NoMessages);
        println!("{:?}", StateMachine);

        // Create a sme and validate it's in the expected state
        let mut sme = StateMachine::new();
        assert_eq!(std::mem::size_of_val(sme.get_sm()), 4);
        assert_eq!(sme.get_state_enter_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_state_exit_cnt(IDX_STATE1), 0);

        // This will panic because state1 returns an invalid transition
        sme.dispatch(&NoMessages);
    }

    #[test]
    #[no_coverage]
    #[should_panic]
    fn test_sm_invalid_initial_state() {
        #[derive(Debug)]
        pub struct StateMachine;

        // Create a Protocol
        #[derive(Debug)]
        pub struct NoMessages;

        const MAX_STATES: usize = 1;
        const IDX_STATE1: usize = 0;
        const _IDX_STATE2: usize = 1;

        impl StateMachine {
            #[no_coverage]
            fn new() -> Executor<Self, NoMessages> {
                let sm = RefCell::new(StateMachine);
                let sme = Executor::new(sm, MAX_STATES)
                    .state(StateInfo::new("state1", Self::state1).parent_idx(IDX_STATE1))
                    .state(StateInfo::new("state2", Self::state2).parent_idx(IDX_STATE1))
                    .build(IDX_STATE1)
                    .expect("Unexpected error initializing");

                sme
            }

            #[no_coverage]
            fn state1(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                (Handled::Yes, None)
            }

            #[no_coverage]
            fn state2(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                // Invalid transition IDX_STATE1 isn't a leaf
                (Handled::Yes, Some(IDX_STATE1))
            }
        }

        // For code coverage
        println!("{:?}", NoMessages);
        println!("{:?}", StateMachine);

        // Create a sme and validate it's in the expected state
        let _ = StateMachine::new();
    }

    #[test]
    #[no_coverage]
    #[should_panic]
    fn test_sm_2s_invalid_transition() {
        #[derive(Debug)]
        pub struct StateMachine;

        // Create a Protocol
        #[derive(Debug)]
        pub struct NoMessages;

        const MAX_STATES: usize = 1;
        const IDX_STATE1: usize = 0;
        const IDX_STATE2: usize = 1;

        impl StateMachine {
            #[no_coverage]
            fn new() -> Executor<Self, NoMessages> {
                let sm = RefCell::new(StateMachine);
                let sme = Executor::new(sm, MAX_STATES)
                    .state(StateInfo::new("state1", Self::state1))
                    .state(StateInfo::new("state1", Self::state2).parent_idx(IDX_STATE1))
                    .build(IDX_STATE2)
                    .expect("Unexpected error initializing");

                sme
            }

            #[no_coverage]
            fn state1(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                (Handled::Yes, None)
            }

            #[no_coverage]
            fn state2(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                // Invalid transition IDX_STATE1 isn't a leaf
                (Handled::Yes, Some(IDX_STATE1))
            }
        }

        // For code coverage
        println!("{:?}", NoMessages);
        println!("{:?}", StateMachine);

        // Create a sme and validate it's in the expected state
        let mut sme = StateMachine::new();
        assert_eq!(std::mem::size_of_val(sme.get_sm()), 4);
        assert_eq!(sme.get_state_enter_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_state_exit_cnt(IDX_STATE1), 0);

        // This will panic because state2 returns an invalid transition
        // to state1 which isn't a leaf
        sme.dispatch(&NoMessages);
    }

    #[test]
    #[no_coverage]
    #[should_panic]
    fn test_sm_out_of_bounds_invalid_transition() {
        #[derive(Debug)]
        pub struct StateMachine;

        // Create a Protocol
        #[derive(Debug)]
        pub struct NoMessages;

        const MAX_STATES: usize = 1;
        const IDX_STATE1: usize = 0;

        impl StateMachine {
            #[no_coverage]
            fn new() -> Executor<Self, NoMessages> {
                let sm = RefCell::new(StateMachine);
                let sme = Executor::new(sm, MAX_STATES)
                    .state(StateInfo::new("state1", Self::state1))
                    .build(IDX_STATE1)
                    .expect("Unexpected error initializing");

                sme
            }

            #[no_coverage]
            fn state1(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                // Invalid transition that is not less than MAX_STATES
                (Handled::Yes, Some(1))
            }
        }

        // For code coverage
        println!("{:?}", NoMessages);
        println!("{:?}", StateMachine);

        // Create a sme and validate it's in the expected state
        let mut sme = StateMachine::new();
        assert_eq!(std::mem::size_of_val(sme.get_sm()), 4);
        assert_eq!(sme.get_state_enter_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_state_exit_cnt(IDX_STATE1), 0);

        // This will panic because state1 returns an invalid transition
        sme.dispatch(&NoMessages);
    }

    // Test SM with one state with one field
    #[test]
    #[no_coverage]
    fn test_sm_1s_enter_no_exit() {
        #[derive(Debug)]
        pub struct StateMachine {
            state: i32,
        }

        // Create a Protocol
        #[derive(Debug)]
        pub enum Messages {
            Add { val: i32 },
            Sub { val: i32 },
        }

        const MAX_STATES: usize = 1;
        const IDX_STATE1: usize = 0;

        impl StateMachine {
            #[no_coverage]
            fn new() -> Executor<Self, Messages> {
                let sm = RefCell::new(StateMachine { state: 0 });
                let sme = Executor::new(sm, MAX_STATES)
                    .state(StateInfo::new("state1", Self::state1).enter_fn(Self::state1_enter))
                    .build(IDX_STATE1)
                    .expect("Unexpected error initializing");

                sme
            }

            #[no_coverage]
            fn state1_enter(&mut self, _msg: &Messages) {
                self.state = 100;
            }

            #[no_coverage]
            fn state1(&mut self, _e: &Executor<Self, Messages>, msg: &Messages) -> StateResult {
                match msg {
                    Messages::Add { val } => self.state += val,
                    Messages::Sub { val } => self.state -= val,
                }
                (Handled::Yes, None)
            }
        }

        // Create a sme and validate it's in the expected state
        let mut sme = StateMachine::new();
        assert_eq!(std::mem::size_of_val(sme.get_sm()), 16);
        assert_eq!(sme.get_state_enter_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_state_exit_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_sm().borrow().state, 0);

        // For code coverage
        println!("{:?}", Messages::Add { val: -1 });
        println!("{:?}", sme.get_sm());

        sme.dispatch(&Messages::Add { val: 2 });
        assert_eq!(sme.get_state_enter_cnt(IDX_STATE1), 1);
        assert_eq!(sme.get_state_process_cnt(IDX_STATE1), 1);
        assert_eq!(sme.get_state_exit_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_sm().borrow().state, 102);

        sme.dispatch(&Messages::Sub { val: 1 });
        assert_eq!(sme.get_state_enter_cnt(IDX_STATE1), 1);
        assert_eq!(sme.get_state_process_cnt(IDX_STATE1), 2);
        assert_eq!(sme.get_state_exit_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_sm().borrow().state, 101);
    }

    // Test SM with twos state with one field
    #[test]
    #[no_coverage]
    fn test_sm_2s_no_enter_no_exit() {
        #[derive(Debug)]
        pub struct StateMachine {
            state: i32,
        }

        // Create a Protocol
        #[derive(Debug)]
        pub enum Message {
            Add { val: i32 },
        }

        const MAX_STATES: usize = 2;
        const IDX_STATE1: usize = 0;
        const IDX_STATE2: usize = 1;

        impl StateMachine {
            #[no_coverage]
            fn new() -> Executor<Self, Message> {
                let sm = RefCell::new(StateMachine { state: 0 });
                let sme = Executor::new(sm, MAX_STATES)
                    .state(StateInfo::new("state1", Self::state1))
                    .state(StateInfo::new("state2", Self::state2))
                    .build(IDX_STATE1)
                    .expect("Unexpected error initializing");

                sme
            }

            #[no_coverage]
            fn state1(&mut self, _e: &Executor<Self, Message>, msg: &Message) -> StateResult {
                match msg {
                    Message::Add { val } => self.state += val,
                }
                (Handled::Yes, Some(IDX_STATE2))
            }

            #[no_coverage]
            fn state2(&mut self, _e: &Executor<Self, Message>, msg: &Message) -> StateResult {
                match msg {
                    Message::Add { val } => self.state += 2 * val,
                }
                (Handled::Yes, Some(IDX_STATE1))
            }
        }

        // Create a sme and validate it's in the expected state
        let mut sme = StateMachine::new();
        assert_eq!(std::mem::size_of_val(sme.get_sm()), 16);
        assert_eq!(sme.get_state_enter_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_state_exit_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_state_enter_cnt(IDX_STATE2), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_STATE2), 0);
        assert_eq!(sme.get_state_exit_cnt(IDX_STATE2), 0);
        assert_eq!(sme.get_sm().borrow().state, 0);

        // For code coverage
        println!("{:?}", Message::Add { val: -2 });
        println!("{:?}", sme.get_sm());

        sme.dispatch(&Message::Add { val: 2 });
        assert_eq!(sme.get_state_enter_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_STATE1), 1);
        assert_eq!(sme.get_state_exit_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_state_enter_cnt(IDX_STATE2), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_STATE2), 0);
        assert_eq!(sme.get_state_exit_cnt(IDX_STATE2), 0);
        assert_eq!(sme.get_sm().borrow().state, 2);

        sme.dispatch(&Message::Add { val: -1 });
        assert_eq!(sme.get_state_enter_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_STATE1), 1);
        assert_eq!(sme.get_state_exit_cnt(IDX_STATE1), 0);
        assert_eq!(sme.get_state_enter_cnt(IDX_STATE2), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_STATE2), 1);
        assert_eq!(sme.get_state_exit_cnt(IDX_STATE2), 0);
        assert_eq!(sme.get_sm().borrow().state, 0);
    }

    // Test SM with twos state with one field
    #[test]
    #[no_coverage]
    fn test_sm_1h_2s_not_handled_no_enter_no_exit() {
        #[derive(Debug)]
        pub struct StateMachine {
            state: i32,
        }

        // Create a Protocol
        #[derive(Debug)]
        pub enum Message {
            Add { val: i32 },
            Sub { val: i32 },
        }

        const MAX_STATES: usize = 2;
        const IDX_PARENT: usize = 0;
        const IDX_CHILD: usize = 1;

        impl StateMachine {
            #[no_coverage]
            fn new() -> Executor<Self, Message> {
                let sm = RefCell::new(StateMachine { state: 0 });
                let sme = Executor::new(sm, MAX_STATES)
                    .state(StateInfo::new("parent", Self::parent))
                    .state(StateInfo::new("child", Self::child).parent_idx(IDX_PARENT))
                    .build(IDX_CHILD)
                    .expect("Unexpected error initializing");

                sme
            }

            #[no_coverage]
            fn parent(&mut self, _e: &Executor<Self, Message>, msg: &Message) -> StateResult {
                match msg {
                    Message::Add { val } => self.state += val,
                    Message::Sub { val } => self.state -= val,
                }
                (Handled::Yes, None)
            }

            #[no_coverage]
            fn child(&mut self, _e: &Executor<Self, Message>, _msg: &Message) -> StateResult {
                (Handled::No, None)
            }
        }

        // Create a sme and validate it's in the expected state
        let mut sme = StateMachine::new();
        assert_eq!(std::mem::size_of_val(sme.get_sm()), 16);
        assert_eq!(sme.get_state_enter_cnt(IDX_PARENT), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_PARENT), 0);
        assert_eq!(sme.get_state_exit_cnt(IDX_PARENT), 0);
        assert_eq!(sme.get_state_enter_cnt(IDX_CHILD), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_CHILD), 0);
        assert_eq!(sme.get_state_exit_cnt(IDX_CHILD), 0);
        assert_eq!(sme.get_sm().borrow().state, 0);

        // For code coverage
        println!("{:?}", Message::Add { val: -1 });
        println!("{:?}", sme.get_sm());

        sme.dispatch(&Message::Add { val: 2 });
        assert_eq!(sme.get_state_enter_cnt(IDX_PARENT), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_PARENT), 1);
        assert_eq!(sme.get_state_exit_cnt(IDX_PARENT), 0);
        assert_eq!(sme.get_state_enter_cnt(IDX_CHILD), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_CHILD), 1);
        assert_eq!(sme.get_state_exit_cnt(IDX_CHILD), 0);
        assert_eq!(sme.get_sm().borrow().state, 2);

        sme.dispatch(&Message::Sub { val: 1 });
        assert_eq!(sme.get_state_enter_cnt(IDX_PARENT), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_PARENT), 2);
        assert_eq!(sme.get_state_exit_cnt(IDX_PARENT), 0);
        assert_eq!(sme.get_state_enter_cnt(IDX_CHILD), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_CHILD), 2);
        assert_eq!(sme.get_state_exit_cnt(IDX_CHILD), 0);
        assert_eq!(sme.get_sm().borrow().state, 1);
    }

    #[test]
    #[no_coverage]
    fn test_leaf_transitions_in_a_tree() {
        // StateMachine simply transitions back and forth
        // between initial and other.
        //
        //                base=0
        //        --------^  ^-------
        //       /                   \
        //      /                     \
        //    other=2   <======>   initial=1

        #[derive(Debug)]
        struct StateMachine;

        // Create a Protocol with no messages
        #[derive(Debug)]
        struct NoMessages;

        const MAX_STATES: usize = 3;
        const IDX_BASE: usize = 0;
        const IDX_INITIAL: usize = 1;
        const IDX_OTHER: usize = 2;

        impl StateMachine {
            #[no_coverage]
            fn new() -> Executor<Self, NoMessages> {
                let sm = RefCell::new(StateMachine);
                let sme = Executor::new(sm, MAX_STATES)
                    .state(StateInfo::new("base", Self::base).enter_fn(Self::base_enter))
                    .state(
                        StateInfo::new("initial", Self::initial)
                            .enter_fn(Self::initial_enter)
                            .exit_fn(Self::initial_exit)
                            .parent_idx(IDX_BASE),
                    )
                    .state(
                        StateInfo::new("other", Self::other)
                            .enter_fn(Self::other_enter)
                            .exit_fn(Self::other_exit)
                            .parent_idx(IDX_BASE),
                    )
                    .build(IDX_INITIAL)
                    .expect("Unexpected error initializing");

                sme
            }

            fn base_enter(&mut self, _msg: &NoMessages) {}

            // This state has idx 0
            #[no_coverage]
            fn base(&mut self, _e: &Executor<Self, NoMessages>, _msg: &NoMessages) -> StateResult {
                (Handled::Yes, None)
            }

            #[no_coverage]
            fn initial_enter(&mut self, _msg: &NoMessages) {}

            // This state has idx 0
            #[no_coverage]
            fn initial(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                (Handled::Yes, Some(IDX_OTHER))
            }

            #[no_coverage]
            fn initial_exit(&mut self, _msg: &NoMessages) {}

            #[no_coverage]
            fn other_enter(&mut self, _msg: &NoMessages) {}

            // This state has idx 0
            #[no_coverage]
            fn other(&mut self, _e: &Executor<Self, NoMessages>, _msg: &NoMessages) -> StateResult {
                (Handled::Yes, Some(IDX_INITIAL))
            }

            #[no_coverage]
            fn other_exit(&mut self, _msg: &NoMessages) {}
        }

        // For code coverage
        println!("{:?}", NoMessages);
        println!("{:?}", StateMachine);

        // Create a sme and validate it's in the expected state
        let mut sme = StateMachine::new();
        assert_eq!(std::mem::size_of_val(sme.get_sm()), 8);
        assert_eq!(sme.get_state_enter_cnt(IDX_BASE), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_BASE), 0);
        assert_eq!(sme.get_state_exit_cnt(IDX_BASE), 0);
        assert_eq!(sme.get_state_enter_cnt(IDX_INITIAL), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_INITIAL), 0);
        assert_eq!(sme.get_state_exit_cnt(IDX_INITIAL), 0);
        assert_eq!(sme.get_state_enter_cnt(IDX_OTHER), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_OTHER), 0);
        assert_eq!(sme.get_state_exit_cnt(IDX_OTHER), 0);

        sme.dispatch(&NoMessages);
        assert_eq!(sme.get_state_enter_cnt(IDX_BASE), 1);
        assert_eq!(sme.get_state_process_cnt(IDX_BASE), 0);
        assert_eq!(sme.get_state_exit_cnt(IDX_BASE), 0);
        assert_eq!(sme.get_state_enter_cnt(IDX_INITIAL), 1);
        assert_eq!(sme.get_state_process_cnt(IDX_INITIAL), 1);
        assert_eq!(sme.get_state_exit_cnt(IDX_INITIAL), 1);
        assert_eq!(sme.get_state_enter_cnt(IDX_OTHER), 0);
        assert_eq!(sme.get_state_process_cnt(IDX_OTHER), 0);
        assert_eq!(sme.get_state_exit_cnt(IDX_OTHER), 0);

        sme.dispatch(&NoMessages);
        assert_eq!(sme.get_state_enter_cnt(IDX_BASE), 1);
        assert_eq!(sme.get_state_process_cnt(IDX_BASE), 0);
        assert_eq!(sme.get_state_exit_cnt(IDX_BASE), 0);
        assert_eq!(sme.get_state_enter_cnt(IDX_INITIAL), 1);
        assert_eq!(sme.get_state_process_cnt(IDX_INITIAL), 1);
        assert_eq!(sme.get_state_exit_cnt(IDX_INITIAL), 1);
        assert_eq!(sme.get_state_enter_cnt(IDX_OTHER), 1);
        assert_eq!(sme.get_state_process_cnt(IDX_OTHER), 1);
        assert_eq!(sme.get_state_exit_cnt(IDX_OTHER), 1);

        sme.dispatch(&NoMessages);
        assert_eq!(sme.get_state_enter_cnt(IDX_BASE), 1);
        assert_eq!(sme.get_state_process_cnt(IDX_BASE), 0);
        assert_eq!(sme.get_state_exit_cnt(IDX_BASE), 0);
        assert_eq!(sme.get_state_enter_cnt(IDX_INITIAL), 2);
        assert_eq!(sme.get_state_process_cnt(IDX_INITIAL), 2);
        assert_eq!(sme.get_state_exit_cnt(IDX_INITIAL), 2);
        assert_eq!(sme.get_state_enter_cnt(IDX_OTHER), 1);
        assert_eq!(sme.get_state_process_cnt(IDX_OTHER), 1);
        assert_eq!(sme.get_state_exit_cnt(IDX_OTHER), 1);

        sme.dispatch(&NoMessages);
        assert_eq!(sme.get_state_enter_cnt(IDX_BASE), 1);
        assert_eq!(sme.get_state_process_cnt(IDX_BASE), 0);
        assert_eq!(sme.get_state_exit_cnt(IDX_BASE), 0);
        assert_eq!(sme.get_state_enter_cnt(IDX_INITIAL), 2);
        assert_eq!(sme.get_state_process_cnt(IDX_INITIAL), 2);
        assert_eq!(sme.get_state_exit_cnt(IDX_INITIAL), 2);
        assert_eq!(sme.get_state_enter_cnt(IDX_OTHER), 2);
        assert_eq!(sme.get_state_process_cnt(IDX_OTHER), 2);
        assert_eq!(sme.get_state_exit_cnt(IDX_OTHER), 2);

        sme.dispatch(&NoMessages);
        assert_eq!(sme.get_state_enter_cnt(IDX_BASE), 1);
        assert_eq!(sme.get_state_process_cnt(IDX_BASE), 0);
        assert_eq!(sme.get_state_exit_cnt(IDX_BASE), 0);
        assert_eq!(sme.get_state_enter_cnt(IDX_INITIAL), 3);
        assert_eq!(sme.get_state_process_cnt(IDX_INITIAL), 3);
        assert_eq!(sme.get_state_exit_cnt(IDX_INITIAL), 3);
        assert_eq!(sme.get_state_enter_cnt(IDX_OTHER), 2);
        assert_eq!(sme.get_state_process_cnt(IDX_OTHER), 2);
        assert_eq!(sme.get_state_exit_cnt(IDX_OTHER), 2);
    }

    #[test]
    #[no_coverage]
    //#[cfg(not(tarpaulin_include))]
    fn test_leaf_transitions_between_trees() {
        // StateMachine simply transitions back and forth
        // between initial and other.
        //
        //  other_base=2          initial_base=0
        //       ^                     ^
        //       |                     |
        //     other=3              initial=1

        #[derive(Debug)]
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
            #[no_coverage]
            fn new() -> Executor<Self, NoMessages> {
                let sm = RefCell::new(StateMachine);
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
                    .build(IDX_INITIAL)
                    .expect("Unexpected error initializing");

                sme
            }

            #[no_coverage]
            fn initial_base_enter(&mut self, _msg: &NoMessages) {}

            // This state has hdl 0
            #[no_coverage]
            fn initial_base(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                (Handled::Yes, None)
            }

            #[no_coverage]
            fn initial_base_exit(&mut self, _msg: &NoMessages) {}

            #[no_coverage]
            fn initial_enter(&mut self, _msg: &NoMessages) {}

            // This state has hdl 0
            #[no_coverage]
            fn initial(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                (Handled::Yes, Some(IDX_OTHER))
            }

            #[no_coverage]
            fn initial_exit(&mut self, _msg: &NoMessages) {}

            #[no_coverage]
            fn other_base_enter(&mut self, _msg: &NoMessages) {}

            // This state has hdl 0
            #[no_coverage]
            fn other_base(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                (Handled::Yes, None)
            }

            #[no_coverage]
            fn other_base_exit(&mut self, _msg: &NoMessages) {}

            #[no_coverage]
            fn other_enter(&mut self, _msg: &NoMessages) {}

            // This state has hdl 0
            #[no_coverage]
            fn other(&mut self, _e: &Executor<Self, NoMessages>, _msg: &NoMessages) -> StateResult {
                (Handled::Yes, Some(IDX_INITIAL))
            }

            #[no_coverage]
            fn other_exit(&mut self, _msg: &NoMessages) {}
        }

        // For code coverage
        println!("{:?}", NoMessages);
        println!("{:?}", StateMachine);

        // Create a sme and validate it's in the expected state
        let mut sme = StateMachine::new();
        assert_eq!(std::mem::size_of_val(sme.get_sm()), 8);
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

    #[test]
    #[no_coverage]
    fn test_1s_cycle() {
        // StateMachine with one state and has itself as parent,
        // this should fail to initialize!
        //
        //     ------
        //     |    |
        //     v    |
        //  state1 --

        #[derive(Debug)]
        pub struct StateMachine;

        // Create a Protocol
        #[derive(Debug)]
        pub struct NoMessages;

        const MAX_STATES: usize = 1;
        const IDX_STATE1: usize = 0;

        impl StateMachine {
            #[no_coverage]
            fn new() {
                let sm = RefCell::new(StateMachine);
                match Executor::new(sm, MAX_STATES)
                    .state(StateInfo::new("state1", Self::state1).parent_idx(IDX_STATE1))
                    .build(IDX_STATE1)
                {
                    Ok(_) => panic!("Expected a cycle it wasn't detected"),
                    Err(e) => assert_eq!(e.to_string(), "Cycle detected"),
                }
            }

            #[no_coverage]
            fn state1(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                (Handled::Yes, None)
            }
        }

        // For code coverage
        println!("{:?}", NoMessages);
        println!("{:?}", StateMachine);

        StateMachine::new();
    }

    #[test]
    #[no_coverage]
    fn test_2s_one_self_cycle() {
        // StateMachine with one state and has itself as parent
        // plus another state with no parent, this should fail to initialize!
        //
        //     ------
        //     |    |
        //     v    |
        //  state1 --     state2

        #[derive(Debug)]
        pub struct StateMachine;

        // Create a Protocol
        #[derive(Debug)]
        pub struct NoMessages;

        const MAX_STATES: usize = 1;
        const IDX_STATE1: usize = 0;
        const _IDX_STATE2: usize = 1;

        impl StateMachine {
            #[no_coverage]
            fn new() {
                let sm = RefCell::new(StateMachine);
                match Executor::new(sm, MAX_STATES)
                    .state(StateInfo::new("state1", Self::state1).parent_idx(IDX_STATE1))
                    .state(StateInfo::new("state2", Self::state2))
                    .build(IDX_STATE1)
                {
                    Ok(_) => panic!("Expected a cycle it wasn't detected"),
                    Err(e) => assert_eq!(e.to_string(), "Cycle detected"),
                }
            }

            #[no_coverage]
            fn state1(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                (Handled::Yes, None)
            }

            #[no_coverage]
            fn state2(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                (Handled::Yes, None)
            }
        }

        // For code coverage
        println!("{:?}", NoMessages);
        println!("{:?}", StateMachine);

        StateMachine::new();
    }

    #[test]
    #[no_coverage]
    fn test_2s_cycle() {
        // StateMachine with two states each has the other as parent,
        // this should fail to build!
        //
        //  state2
        //   |  ^
        //   v  |
        //  state1

        #[derive(Debug)]
        pub struct StateMachine;

        // Create a Protocol
        #[derive(Debug)]
        pub struct NoMessages;

        const MAX_STATES: usize = 2;
        const IDX_STATE1: usize = 0;
        const IDX_STATE2: usize = 1;
        const _IDX_STATE3: usize = 2;

        impl StateMachine {
            #[no_coverage]
            fn new() {
                let sm = RefCell::new(StateMachine);
                match Executor::new(sm, MAX_STATES)
                    .state(StateInfo::new("state1", Self::state1).parent_idx(IDX_STATE2))
                    .state(StateInfo::new("state2", Self::state2).parent_idx(IDX_STATE1))
                    .build(IDX_STATE1)
                {
                    Ok(_) => panic!("Expected a cycle it wasn't detected"),
                    Err(e) => assert_eq!(e.to_string(), "Cycle detected"),
                }
            }

            #[no_coverage]
            fn state1(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                (Handled::Yes, None)
            }

            #[no_coverage]
            fn state2(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                (Handled::Yes, None)
            }
        }

        // For code coverage
        println!("{:?}", NoMessages);
        println!("{:?}", StateMachine);

        StateMachine::new();
    }

    #[test]
    #[no_coverage]
    fn test_3s_one_cycle() {
        // StateMachine with three states two have other as parent third is standalone,
        // this should fail to build!
        //
        //  state2   state3
        //   |  ^
        //   v  |
        //  state1

        #[derive(Debug)]
        pub struct StateMachine;

        // Create a Protocol
        #[derive(Debug)]
        pub struct NoMessages;

        const MAX_STATES: usize = 2;
        const IDX_STATE1: usize = 0;
        const IDX_STATE2: usize = 1;

        impl StateMachine {
            #[no_coverage]
            fn new() {
                let sm = RefCell::new(StateMachine);
                match Executor::new(sm, MAX_STATES)
                    .state(StateInfo::new("state1", Self::state1).parent_idx(IDX_STATE2))
                    .state(StateInfo::new("state2", Self::state2).parent_idx(IDX_STATE1))
                    .state(StateInfo::new("state3", Self::state3))
                    .build(IDX_STATE1)
                {
                    Ok(_) => panic!("Expected a cycle it wasn't detected"),
                    Err(e) => assert_eq!(e.to_string(), "Cycle detected"),
                }
            }

            #[no_coverage]
            fn state1(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                (Handled::Yes, None)
            }

            #[no_coverage]
            fn state2(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                (Handled::Yes, None)
            }

            #[no_coverage]
            fn state3(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                (Handled::Yes, None)
            }
        }

        // For code coverage
        println!("{:?}", NoMessages);
        println!("{:?}", StateMachine);

        StateMachine::new();
    }

    #[test]
    #[no_coverage]
    fn test_5s_long_cycle() {
        // StateMachine with 5 states twi leafs and a long cycle from state1 to state3
        // this should fail to initialize!
        //
        //  --- state1
        //  |      ^
        //  |      |
        //  |   state2
        //  |      ^
        //  |      |
        //  --> state3 <-------
        //         ^          |
        //         |          |
        //      state4     state5
        //

        #[derive(Debug)]
        pub struct StateMachine;

        // Create a Protocol
        #[derive(Debug)]
        pub struct NoMessages;

        const MAX_STATES: usize = 5;
        const IDX_STATE1: usize = 0;
        const IDX_STATE2: usize = 1;
        const IDX_STATE3: usize = 2;
        const _IDX_STATE4: usize = 3;
        const _IDX_STATE5: usize = 4;

        impl StateMachine {
            #[no_coverage]
            fn new() {
                let sm = RefCell::new(StateMachine);
                match Executor::new(sm, MAX_STATES)
                    .state(StateInfo::new("state1", Self::state1).parent_idx(IDX_STATE3))
                    .state(StateInfo::new("state2", Self::state2).parent_idx(IDX_STATE1))
                    .state(StateInfo::new("state3", Self::state3).parent_idx(IDX_STATE2))
                    .state(StateInfo::new("state4", Self::state4).parent_idx(IDX_STATE3))
                    .state(StateInfo::new("state5", Self::state5).parent_idx(IDX_STATE3))
                    .build(IDX_STATE1)
                {
                    Ok(_) => panic!("Expected a cycle it wasn't detected"),
                    Err(e) => assert_eq!(e.to_string(), "Cycle detected"),
                }
            }

            #[no_coverage]
            fn state1(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                (Handled::Yes, None)
            }

            #[no_coverage]
            fn state2(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                (Handled::Yes, None)
            }

            #[no_coverage]
            fn state3(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                (Handled::Yes, None)
            }

            #[no_coverage]
            fn state4(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                (Handled::Yes, None)
            }

            #[no_coverage]
            fn state5(
                &mut self,
                _e: &Executor<Self, NoMessages>,
                _msg: &NoMessages,
            ) -> StateResult {
                (Handled::Yes, None)
            }
        }

        // For code coverage
        println!("{:?}", NoMessages);
        println!("{:?}", StateMachine);

        StateMachine::new();
    }
}
