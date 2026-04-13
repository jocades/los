use core::cell::{Cell, UnsafeCell};

#[repr(transparent)]
pub struct Volatile<T: Copy>(T);

impl<T: Copy> Volatile<T> {
    #[allow(dead_code)]
    pub fn new(value: T) -> Self {
        Self(value)
    }

    pub fn read(&self) -> T {
        unsafe { core::ptr::read_volatile(&self.0) }
    }

    pub fn write(&mut self, value: T) {
        unsafe { core::ptr::write_volatile(&mut self.0, value) };
    }
}

impl<T: Copy + Default> Default for Volatile<T> {
    fn default() -> Self {
        Self(T::default())
    }
}

pub struct OnceCell<T> {
    // Invariant: written to at most once.
    inner: UnsafeCell<Option<T>>,
}

// No threads nor CPU interrupts at this stage so lying to the compiler is fine.
unsafe impl<T: Send> Sync for OnceCell<T> {}

#[allow(dead_code)]
impl<T> OnceCell<T> {
    pub const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(None),
        }
    }

    pub fn get(&self) -> Option<&T> {
        unsafe { &*self.inner.get() }.as_ref()
    }

    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.inner.get_mut().as_mut()
    }

    pub fn set(&self, value: T) -> Result<(), T> {
        if self.get().is_some() {
            return Err(value);
        }
        unsafe { *self.inner.get() = Some(value) };
        Ok(())
    }

    /// The reentrant case is allowed and is UB. An `intialising` flag can be used in the future.
    /// ```ignore
    /// let cell = OnceCell::new();
    /// let x = cell.get_or_init(|| {
    ///     cell.get_or_init(|| 2);
    ///     1
    /// });
    /// assert_eq!(x, 1);
    /// ```
    pub fn get_or_init(&self, f: impl FnOnce() -> T) -> &T {
        if let Some(value) = self.get() {
            return value;
        }
        unsafe {
            *self.inner.get() = Some(f());
            self.get().unwrap_unchecked()
        }
    }

    /// The reentrant case is statically impossible since we would be borrowing `&mut self` more
    /// than once at a time and the compiler makes sure the invariant holds.
    pub fn get_mut_or_init(&mut self, f: impl FnOnce() -> T) -> &mut T {
        self.inner.get_mut().get_or_insert_with(f)
    }
}

pub struct Lazy<T, F = fn() -> T> {
    cell: OnceCell<T>,
    init: Cell<Option<F>>,
}

// No threads nor CPU interrupts at this stage so lying to the compiler is fine.
unsafe impl<T: Send> Sync for Lazy<T> {}

impl<T, F: FnOnce() -> T> Lazy<T, F> {
    pub const fn new(init: F) -> Self {
        Self {
            cell: OnceCell::new(),
            init: Cell::new(Some(init)),
        }
    }

    pub fn force(this: &Lazy<T, F>) -> &T {
        this.cell.get_or_init(|| match this.init.take() {
            Some(f) => f(),
            None => unreachable!(),
        })
    }

    pub fn force_mut(this: &mut Lazy<T, F>) -> &mut T {
        this.cell.get_mut_or_init(|| match this.init.take() {
            Some(f) => f(),
            None => unreachable!(),
        })
    }
}

impl<T, F: FnOnce() -> T> core::ops::Deref for Lazy<T, F> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        Lazy::force(self)
    }
}

impl<T, F: FnOnce() -> T> core::ops::DerefMut for Lazy<T, F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Lazy::force_mut(self)
    }
}

#[allow(dead_code)]
pub mod spin {
    use core::cell::UnsafeCell;
    use core::ops::{Deref, DerefMut};
    use core::sync::atomic::{AtomicBool, Ordering};

    const LOCKED: bool = true;
    const UNLOCKED: bool = false;

    pub struct Mutex<T> {
        lock: AtomicBool,
        data: UnsafeCell<T>,
    }

    unsafe impl<T: Send> Sync for Mutex<T> {}

    impl<T> Mutex<T> {
        pub fn new(data: T) -> Self {
            Self {
                lock: AtomicBool::new(UNLOCKED),
                data: UnsafeCell::new(data),
            }
        }

        pub fn lock(&self) -> MutexGuard<'_, T> {
            while self
                .lock
                .compare_exchange_weak(UNLOCKED, LOCKED, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                // while self.lock.load(Ordering::Relaxed) == LOCKED {
                core::hint::spin_loop();
                // }
            }
            MutexGuard { mu: self }
        }
    }

    pub struct MutexGuard<'a, T> {
        mu: &'a Mutex<T>,
    }

    impl<T> Deref for MutexGuard<'_, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            unsafe { &*self.mu.data.get() }
        }
    }

    impl<T> DerefMut for MutexGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            unsafe { &mut *self.mu.data.get() }
        }
    }

    impl<T> Drop for MutexGuard<'_, T> {
        fn drop(&mut self) {
            self.mu.lock.store(UNLOCKED, Ordering::Release);
        }
    }
}
