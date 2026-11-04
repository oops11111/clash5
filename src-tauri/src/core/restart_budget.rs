use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub(crate) const RESTART_WINDOW: Duration = Duration::from_secs(5 * 60);
pub(crate) const MAX_RESTARTS: usize = 3;

#[derive(Clone, Copy, Debug)]
pub(crate) struct RecoveryRequest {
    pub(crate) ticket: u64,
    pub(crate) readiness: u64,
}

impl RecoveryRequest {
    pub(crate) const fn is_current(self, ticket: u64, readiness: u64, stopped: bool, exiting: bool) -> bool {
        !exiting && stopped && ticket == self.ticket && readiness == self.readiness
    }
}

#[derive(Debug, Default)]
pub(crate) struct RestartBudget {
    attempts: VecDeque<Instant>,
}

impl RestartBudget {
    pub(crate) fn try_start(&mut self, now: Instant) -> bool {
        while self
            .attempts
            .front()
            .is_some_and(|attempt| now.saturating_duration_since(*attempt) >= RESTART_WINDOW)
        {
            self.attempts.pop_front();
        }
        if self.attempts.len() >= MAX_RESTARTS {
            return false;
        }
        self.attempts.push_back(now);
        true
    }

    pub(crate) fn attempt_count(&self) -> usize {
        self.attempts.len()
    }
}
