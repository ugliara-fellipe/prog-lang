use bumpalo::Bump;
use std::cell::RefCell;

pub struct RoutineArena {
    arena: RefCell<Bump>,
}

impl RoutineArena {
    pub fn new() -> Self {
        Self {
            arena: RefCell::new(Bump::new()),
        }
    }

    pub fn alloc<T>(&self, val: T) -> &mut T {
        // Allocate space in local arena memory in O(1)
        let arena_ref = unsafe { &mut *self.arena.as_ptr() };
        arena_ref.alloc(val)
    }

    pub fn reset(&self) {
        self.arena.borrow_mut().reset();
    }
}
