use std::boxed::Box;

/// A function of `&T -> bool`, with an "always true" default.
///
/// This exists to account for Plugin and Resource "approval" processes to
/// choose between conflicting configurations of a type.
///
/// Most plugins shouldn't be picky, all they require is the _presence_ of a
/// resource or other plugin. We can imagine how a game developer might have
/// tighter, runtime-known constraints on a 3rd party crate than a plugin
/// author or engine developer.
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
