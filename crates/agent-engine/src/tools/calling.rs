use std::cell::RefCell;

use super::Workspace;

thread_local! {
    static CALL: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub fn current() -> Option<String> {
    CALL.with(|call| call.borrow().clone())
}

pub fn within<T>(id: &str, run: impl FnOnce() -> T) -> T {
    struct Restore(Option<String>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            CALL.with(|call| *call.borrow_mut() = previous);
        }
    }
    let _restore = Restore(CALL.with(|call| call.borrow_mut().replace(id.to_owned())));
    run()
}

pub fn traceparent(workspace: &Workspace) -> Option<String> {
    let trace = workspace.trace.as_deref()?;
    domain::trace::otlp::traceparent(trace, &current()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRACE: &str = "0af7651916cd43dd8448eb211c80319c";

    #[test]
    fn a_tool_call_carries_the_run_trace_and_its_own_span_while_it_runs() {
        let mut workspace = Workspace::new("/tmp");
        assert_eq!(within("c1", || traceparent(&workspace)), None, "no trace, nothing to send");
        workspace.trace = Some(TRACE.into());
        assert_eq!(traceparent(&workspace), None, "outside a call there is no span to point at");
        let (outer, nested) = within("c1", || {
            let nested = within("c2", || traceparent(&workspace));
            (traceparent(&workspace), nested)
        });
        let span = domain::trace::otlp::call_span_id(TRACE, "c1");
        assert_eq!(outer, Some(format!("00-{TRACE}-{span}-01")));
        assert_ne!(outer, nested);
        assert_eq!(current(), None);
    }
}
