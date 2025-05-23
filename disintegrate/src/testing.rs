//! Utility for testing a Decision implementation
//!
//! The test harness allows you to set up a history of events, perform the given decision,
//! and make assertions about the resulting changes.
use std::fmt::Debug;

use crate::{Decision, Event, IntoState, IntoStatePart, MultiState, PersistedEvent};

/// Test harness for testing decisions.
pub struct TestHarness;

impl TestHarness {
    /// Sets up a history of events.
    ///
    /// # Arguments
    ///
    /// * `history` - A history of events to derive the current state.
    ///
    /// # Returns
    ///
    /// A `TestHarnessStep` representing the "given" step.
    pub fn given<E: Event + Clone>(history: impl Into<Vec<E>>) -> TestHarnessStep<E, Given> {
        TestHarnessStep {
            history: history.into(),
            _step: Given,
        }
    }
}

/// Represents the given step of the test harness.
pub struct Given;

/// Represents when step of the test harness.
pub struct When<R, ERR> {
    result: Result<Vec<R>, ERR>,
}

pub struct TestHarnessStep<E, ST> {
    history: Vec<E>,
    _step: ST,
}

impl<E: Event + Clone> TestHarnessStep<E, Given> {
    /// Executes a decision on the state derived from the given history.
    ///
    /// # Arguments
    ///
    /// * `decision` - The decision to test.
    ///
    /// # Returns
    ///
    /// A `TestHarnessStep` representing the "when" step.
    pub fn when<D, SP, S, ERR>(self, decision: D) -> TestHarnessStep<E, When<E, ERR>>
    where
        D: Decision<Event = E, Error = ERR, StateQuery = S>,
        S: IntoStatePart<i64, S, Target = SP>,
        SP: IntoState<S> + MultiState<i64, E>,
    {
        let mut state = decision.state_query().into_state_part();
        for event in self
            .history
            .iter()
            .enumerate()
            .map(|(id, event)| PersistedEvent::new((id + 1) as i64, event.clone()))
        {
            state.mutate_all(event);
        }
        let result = decision.process(&state.into_state());
        TestHarnessStep {
            history: self.history,
            _step: When { result },
        }
    }
}

impl<R, E, ERR> TestHarnessStep<E, When<R, ERR>>
where
    E: Event + Clone + PartialEq,
    R: Debug + PartialEq,
    ERR: Debug + PartialEq,
{
    /// Makes assertions about the changes.
    ///
    /// # Arguments
    ///
    /// * `expected` - The expected changes.
    ///
    /// # Panics
    ///
    /// Panics if the action result is not `Ok` or if the changes do not match the expected changes.
    ///
    /// # Examples
    #[track_caller]
    pub fn then(self, expected: impl Into<Vec<R>>) {
        assert_eq!(Ok(expected.into()), self._step.result);
    }

    /// Allows for custom assertions on the resulting events from a decision execution.
    ///
    /// The `then_assert` method enables more complex verification logic beyond simple equality checks.
    /// This is particularly useful when you need to perform detailed validation of event properties or
    /// when the exact sequence or content of events requires custom validation logic.
    ///
    /// # Parameters
    ///
    /// * `assertion` - A closure that receives a reference to the vector of resulting events and performs custom assertions on them.
    ///
    /// # Example
    ///
    /// ```no_run
    ///
    ///     #[test]
    ///     fn test_with_custom_assertions() {
    ///         disintegrate::TestHarness::given([
    ///             DomainEvent::AccountOpened { account_id: 1 },
    ///             DomainEvent::AmountDeposited {
    ///                 account_id: 1,
    ///                 amount: 10,
    ///             },
    ///         ])
    ///         .when(WithdrawAmount::new(1, 10))
    ///         .then_assert(|events| {
    ///             // Complex assertions can be implemented here
    ///             assert_eq!(events.len(), 1, "Expected exactly one event");
    ///             if let DomainEvent::AmountWithdrawn { account_id, amount } = &events[0] {
    ///                 assert_eq!(*account_id, 1);
    ///                 assert_eq!(*amount, 10);
    ///                 // Additional validation like checking timestamps, etc.
    ///             } else {
    ///                 panic!("Expected AmountWithdrawn event failed");
    ///             }
    ///         });
    ///     }
    /// ```
    ///
    /// # Notes
    ///
    /// * This method is tracked by the Rust caller location system, so error messages will point to the correct line in your test.
    /// * Use `then()` for straightforward equality assertions
    /// * For asserting errors rather than events, use `then_err()` instead.
    #[track_caller]
    pub fn then_assert(self, assertion: impl FnOnce(&Vec<R>)) {
        assertion(&self._step.result.unwrap());
    }

    /// Makes assertions about the expected error result.
    ///
    /// # Arguments
    ///
    /// * `expected` - The expected error.
    ///
    /// # Panics
    ///
    /// Panics if the action result is not `Err` or if the error does not match the expected error.
    #[track_caller]
    pub fn then_err(self, expected: ERR) {
        let err = self._step.result.unwrap_err();
        assert_eq!(err, expected);
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use super::*;
    use crate::utils::tests::*;

    #[test]
    fn it_should_set_up_initial_state_and_apply_the_history() {
        let mut mock_add_item = MockDecision::new();
        mock_add_item
            .expect_state_query()
            .once()
            .return_once(|| cart("c1", []));
        mock_add_item
            .expect_process()
            .once()
            .return_once(|_| Ok(vec![item_added_event("p2", "c1")]));

        TestHarness::given(vec![item_added_event("p1", "c1")])
            .when(mock_add_item)
            .then([item_added_event("p2", "c1")]);
    }

    #[test]
    #[should_panic]
    fn it_should_panic_when_action_failed_and_events_were_expected() {
        let mut mock_add_item = MockDecision::new();
        mock_add_item
            .expect_process()
            .once()
            .return_once(|_| Err(CartError("Some error".to_string())));
        TestHarness::given([])
            .when(mock_add_item)
            .then([item_added_event("p2", "c1")]);
    }

    #[test]
    fn it_should_assert_expected_error_with_then_err() {
        let mut mock_add_item = MockDecision::new();
        mock_add_item
            .expect_state_query()
            .once()
            .return_once(|| cart("c1", []));
        mock_add_item
            .expect_process()
            .once()
            .return_once(|_| Err(CartError("Some error".to_string())));
        TestHarness::given([])
            .when(mock_add_item)
            .then_err(CartError("Some error".to_string()));
    }

    #[test]
    #[should_panic]
    fn it_should_panic_when_an_error_is_expected() {
        let mut mock_add_item = MockDecision::new();
        mock_add_item
            .expect_process()
            .once()
            .return_once(|_| Ok(vec![item_added_event("p2", "c1")]));

        TestHarness::given(vec![item_added_event("p1", "c1")])
            .when(mock_add_item)
            .then_err(CartError("Some error".to_string()));
    }
}
