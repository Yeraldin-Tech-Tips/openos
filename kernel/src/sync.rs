use core::{
    cell::UnsafeCell,
    hint::spin_loop,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy)]
struct IrqState {
    were_enabled: bool,
}

impl IrqState {
    fn save_and_disable() -> Self {
        let rflags: u64;
        unsafe {
            core::arch::asm!(
                "pushfq",
                "pop {}",
                "cli",
                out(reg) rflags,
                options(nomem)
            );
        }
        Self {
            were_enabled: (rflags & (1 << 9)) != 0,
        }
    }

    fn restore(self) {
        if self.were_enabled {
            unsafe {
                core::arch::asm!("sti", options(nostack));
            }
        }
    }
}

pub struct IrqSafeLock<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for IrqSafeLock<T> {}

impl<T> IrqSafeLock<T> {
    pub const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    pub fn lock(&self) -> IrqSafeGuard<'_, T> {
        let irq_state = IrqState::save_and_disable();
        while self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            spin_loop();
        }
        IrqSafeGuard {
            lock: self,
            irq_state,
        }
    }
}

pub struct IrqSafeGuard<'a, T> {
    lock: &'a IrqSafeLock<T>,
    irq_state: IrqState,
}

impl<T> core::ops::Deref for IrqSafeGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> core::ops::DerefMut for IrqSafeGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for IrqSafeGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
        self.irq_state.restore();
    }
}
