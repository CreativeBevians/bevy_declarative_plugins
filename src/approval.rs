use std::boxed::Box;

/// A function that, given a type (and a context), returns a boolean.
///
/// Defined and used here in the context of solving the dependency graph by
/// "asking" each dependency if a given resource or plugin config is appropriate
/// for its needs.
///
/// Most plugins shouldn't be picky, all they require is the _presence_ of a
/// resource or other plugin. But some might have tighter, runtime-known constraints.
///
/// Approval functions are expected to, mostly, return true and neither contain
/// nor take take advantage of mutable state. Memoization rights reserved.
#[derive(Default)]
pub(crate) struct Approval<T: ?Sized> {
    approval_fn: Option<Box<dyn Fn(&T) -> bool>>,
}

impl<T> Approval<T> {
    /// Creates a new approval function which does not care about context.
    pub(crate) fn new(approval: impl Fn(&T) -> bool + 'static) -> Self {
        Self {
            approval_fn: Some(Box::new(move |input| approval(input))),
        }
    }

    /// "Asks" if the input is good enough.
    pub(crate) fn approves(&self, input: &T) -> bool {
        self.approval_fn.as_ref().map(|f| f(input)).unwrap_or(true)
    }

    pub(crate) fn always_approve() -> Self {
        Self { approval_fn: None }
    }

    pub(crate) fn is_trivially_true(&self) -> bool {
        self.approval_fn.is_none()
    }
}

impl<T, F: Fn(&T) -> bool + 'static> From<F> for Approval<T> {
    fn from(value: F) -> Self {
        Self::new(value)
    }
}
