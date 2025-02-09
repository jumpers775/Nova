use gtk::glib;
use std::future::Future;
use std::rc::Rc;
use std::sync::Arc;

/// Manages background operations on a separate MainContext
pub struct BackgroundContext {
    context: glib::MainContext,
}

impl BackgroundContext {
    pub fn new() -> Self {
        Self {
            context: glib::MainContext::new(),
        }
    }

    /// Spawn a future on the background context
    pub fn spawn<F>(&self, future: F) -> glib::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.context.spawn(future)
    }

    /// Get a reference to the underlying MainContext
    pub fn context(&self) -> &glib::MainContext {
        &self.context
    }
}

/// Global background context for the application
pub fn global() -> &'static BackgroundContext {
    static INSTANCE: std::sync::OnceLock<BackgroundContext> = std::sync::OnceLock::new();
    INSTANCE.get_or_init(BackgroundContext::new)
}
