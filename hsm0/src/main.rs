use custom_logger::env_logger_init;

fn hsm0() {
    use hsm0::{NoMessages, StateMachine};

    println!("hsm0");

    // Create a sm and validate it's in the expected state
    let mut sm = StateMachine::new();
    assert_eq!(sm.base_enter_cnt, 0);
    assert_eq!(sm.base_cnt, 0);
    assert_eq!(sm.base_exit_cnt, 0);
    assert_eq!(sm.intermediate_enter_cnt, 0);
    assert_eq!(sm.intermediate_cnt, 0);
    assert_eq!(sm.intermediate_exit_cnt, 0);
    assert_eq!(sm.bottom_enter_cnt, 0);
    assert_eq!(sm.bottom_cnt, 0);
    assert_eq!(sm.bottom_exit_cnt, 0);

    // base process msg returns TranitionTo intermediate, its child
    sm.dispatch_msg(&NoMessages);
    assert_eq!(sm.base_enter_cnt, 1);
    assert_eq!(sm.base_cnt, 1);
    assert_eq!(sm.base_exit_cnt, 1); // BUG, we're going to a child shouldn't not leaving base
    assert_eq!(sm.intermediate_enter_cnt, 0);
    assert_eq!(sm.intermediate_cnt, 0);
    assert_eq!(sm.intermediate_exit_cnt, 0);
    assert_eq!(sm.bottom_enter_cnt, 0);
    assert_eq!(sm.bottom_cnt, 0);
    assert_eq!(sm.bottom_exit_cnt, 0);

    sm.dispatch_msg(&NoMessages);
    assert_eq!(sm.base_enter_cnt, 1);
    assert_eq!(sm.base_cnt, 1);
    assert_eq!(sm.base_exit_cnt, 1); // OK, didn't change but should be 0
    assert_eq!(sm.intermediate_enter_cnt, 1);
    assert_eq!(sm.intermediate_cnt, 1);
    assert_eq!(sm.intermediate_exit_cnt, 0);
    assert_eq!(sm.bottom_enter_cnt, 0);
    assert_eq!(sm.bottom_cnt, 0);
    assert_eq!(sm.bottom_exit_cnt, 0);

    // Dispatch the message and validate it transitioned
    sm.dispatch_msg(&NoMessages);
    assert_eq!(sm.base_enter_cnt, 1);
    assert_eq!(sm.base_cnt, 1);
    assert_eq!(sm.base_exit_cnt, 1);
    assert_eq!(sm.intermediate_enter_cnt, 1);
    assert_eq!(sm.intermediate_cnt, 2);
    assert_eq!(sm.intermediate_exit_cnt, 0);
    assert_eq!(sm.bottom_enter_cnt, 0);
    assert_eq!(sm.bottom_cnt, 0);
    assert_eq!(sm.bottom_exit_cnt, 0);
}

fn main() {
    env_logger_init("info");
    log::debug!("main:+");

    hsm0();

    log::debug!("main:-");
}
