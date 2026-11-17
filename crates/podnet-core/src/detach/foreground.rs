//! The ready document in the foreground (#631): a command with `--json`
//! that runs in the foreground gets its ready document from [`super::ready`]
//! through a hook, as a detached child sends it to its parent.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

type Hook = Box<dyn Fn(&serde_json::Value) + Send + Sync>;

static HOOK: OnceLock<Hook> = OnceLock::new();
static SENT: AtomicBool = AtomicBool::new(false);

/// Give the first ready document of this process to `hook`, when the
/// process is not a detached child. A second hook is ignored.
pub fn on_ready(hook: impl Fn(&serde_json::Value) + Send + Sync + 'static) {
    let _ = HOOK.set(Box::new(hook));
}

/// Called by [`super::ready`]: run the hook once, outside a detached child.
pub(super) fn send(doc: &serde_json::Value) {
    if super::is_child() {
        return;
    }
    if let Some(hook) = HOOK.get() {
        if !SENT.swap(true, Ordering::SeqCst) {
            hook(doc);
        }
    }
}
