use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::rc::{Rc, Weak};

#[derive(Clone)]
pub struct RcCell<T>(Rc<UnsafeCell<T>>);
#[derive(Clone)]
pub struct WeakRcCell<T>(Weak<UnsafeCell<T>>);
impl<T> RcCell<T> {
  pub fn new(value: T) -> Self { Self(Rc::new(UnsafeCell::new(value))) }

  pub fn downgrade(&self) -> WeakRcCell<T> { WeakRcCell(Rc::downgrade(&self.0)) }
}
impl<T> WeakRcCell<T> {
  pub fn upgrade(&self) -> Option<RcCell<T>> { self.0.upgrade().map(RcCell) }
}
impl<T: PartialEq> PartialEq for WeakRcCell<T> {
  fn eq(&self, other: &Self) -> bool {
    match (self.upgrade(), other.upgrade()) {
    (Some(this), Some(other)) => this == other,
    (None, None) => true,
    _ => false,
    }
  }
}

impl<T> Deref for RcCell<T> {
  type Target = T;

  fn deref(&self) -> &Self::Target { unsafe { &*self.0.get() } }
}
impl<T> DerefMut for RcCell<T> {
  fn deref_mut(&mut self) -> &mut Self::Target { unsafe { &mut *self.0.get() } }
}
impl<T: PartialEq> PartialEq for RcCell<T> {
  fn eq(&self, other: &Self) -> bool { self.deref().eq(other.deref()) }
}
impl<T: Eq> Eq for RcCell<T> {}
impl<T> From<T> for RcCell<T> {
  fn from(value: T) -> Self { Self::new(value) }
}

#[test]
pub fn rc_cell() {
  let rc = RcCell::new(5);
  let mut rc2 = rc.clone();
  *rc2 += 1;
  assert_eq!(*rc, 6);
}
