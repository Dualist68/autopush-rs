use std::cell::{RefCell, RefMut};
use std::rc::Rc;

use futures::{
    task::{Context, Poll},
    Sink, Stream,
};

/// Helper object to turn `Rc<RefCell<T>>` into a `Stream` and `Sink`
///
/// This is basically just a helper to allow multiple "owning" references to a
/// `T` which is both a `Stream` and a `Sink`. Similar to `Stream::split` in the
/// futures crate, but doesn't actually split it (and allows internal access).
pub struct RcObject<T>(Rc<RefCell<T>>);

impl<T> RcObject<T> {
    pub fn new(t: T) -> RcObject<T> {
        RcObject(Rc::new(RefCell::new(t)))
    }

    pub fn borrow_mut(&self) -> RefMut<'_, T> {
        self.0.borrow_mut()
    }
}

impl<T: Stream> Stream for RcObject<T> {
    type Item = T::Item;

    fn poll_next(&mut self, _cx: &mut Context) -> Poll<Option<T::Item>> {
        self.0.borrow_mut().poll()
    }
}

impl<T: Stream> Sink<T> for RcObject<T> {
    fn start_send(&mut self, msg: T::Item) -> Result<(), Self::Error> {
        self.0.borrow_mut().start_send(msg)
    }

    fn poll_flush(&mut self) -> Poll<()> {
        self.0.borrow_mut().poll_complete()
    }

    fn poll_close(&mut self) -> Poll<()> {
        self.0.borrow_mut().close()
    }
}

impl<T> Clone for RcObject<T> {
    fn clone(&self) -> RcObject<T> {
        RcObject(self.0.clone())
    }
}
