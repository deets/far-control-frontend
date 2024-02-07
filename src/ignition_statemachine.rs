use std::time::Duration;

use rust_fsm::*;

const TIMEOUT: Duration = Duration::from_secs(10);

state_machine! {
    derive(Debug)
    pub Ignition(Start)

    Start(Timeout) => Reset,

    Reset => {
        Timeout => Reset,
        Ack => Idle,
    },

    Idle => {
        LowerHalf => LowerPrimed,
        Timeout => Reset,
    },

    LowerPrimed => {
        UpperHalf => FullyPrimed,
        Timeout => Reset,
    },

    FullyPrimed => {
        Timeout => Reset,
        Ignite => Ignition
    },

    Ignition => {
        Timeout => Idle,
    },
}

pub struct StateKeeper {
    sm: StateMachine<Ignition>,
    elapsed_since_last_state_change: Duration,
    timeout: Duration,
}

impl Default for StateKeeper {
    fn default() -> Self {
        Self {
            sm: StateMachine::new(),
            elapsed_since_last_state_change: Duration::from_micros(0),
            timeout: TIMEOUT,
        }
    }
}

impl StateKeeper {
    pub fn state(&self) -> &IgnitionState {
        self.sm.state()
    }

    pub fn progress_time(
        &mut self,
        elapsed: Duration,
        action: impl FnOnce(&IgnitionState, &IgnitionState, &IgnitionInput),
    ) {
        self.elapsed_since_last_state_change = self.elapsed_since_last_state_change + elapsed;
        if self.elapsed_since_last_state_change > self.timeout {
            self.feed(&IgnitionInput::Timeout {}, action);
        }
    }

    pub fn feed(
        &mut self,
        event: &IgnitionInput,
        action: impl FnOnce(&IgnitionState, &IgnitionState, &IgnitionInput),
    ) {
        let old_state = self.cloned_state();
        if self.sm.consume(&event).is_ok() {
            self.elapsed_since_last_state_change = Duration::from_millis(0);
            let current_state = self.sm.state();
            action(&old_state, current_state, event);
        }
    }

    // This is a bit insane, but the FSM macro doesn't allow for
    // state cloning/copying, as the enum is generated. I'm a bit
    // at a loss as what else to do.
    pub fn cloned_state(&self) -> IgnitionState {
        match self.state() {
            IgnitionState::FullyPrimed => IgnitionState::FullyPrimed,
            IgnitionState::Idle => IgnitionState::Idle,
            IgnitionState::Ignition => IgnitionState::Ignition,
            IgnitionState::LowerPrimed => IgnitionState::LowerPrimed,
            IgnitionState::Reset => IgnitionState::Reset,
            IgnitionState::Start => IgnitionState::Start,
        }
    }
}

#[cfg(test)]
mod test {
    use std::assert_matches::assert_matches;

    use super::*;

    #[test]
    fn test_start_progresses_to_reset_via_timeout() {
        let mut sk = StateKeeper::default();
        assert_matches!(sk.state(), &IgnitionState::Start);
        sk.progress_time(Duration::from_millis(500), |_, _, _| {});
        assert_matches!(sk.state(), &IgnitionState::Start);
        sk.progress_time(TIMEOUT, |_, _, _| {});
        assert_matches!(sk.state(), &IgnitionState::Reset);
    }

    #[test]
    fn test_non_existing_transition_behavior() {
        let mut sk = StateKeeper::default();
        sk.feed(&IgnitionInput::LowerHalf, |_, _, _| {
            assert!(false);
        });
        assert_matches!(sk.state(), &IgnitionState::Start);
    }

    #[test]
    fn test_callback_invoked_on_transition() {
        let mut sk = StateKeeper::default();
        let mut called = false;
        sk.feed(&IgnitionInput::Timeout, |_, _, _| {
            called = true;
        });
        assert!(called);
    }
}
