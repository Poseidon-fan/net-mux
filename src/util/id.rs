//! Stream identifier types and the per-session allocator.

use std::sync::atomic::{AtomicU32, Ordering};

/// Unique identifier of a logical stream within a session.
pub type StreamId = u32;

/// Endpoint role of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    /// Initiator side: allocates odd stream ids starting at 1.
    Client,
    /// Acceptor side: allocates even stream ids starting at 2.
    Server,
}

impl Role {
    pub(crate) fn first_id(self) -> StreamId {
        match self {
            Role::Client => 1,
            Role::Server => 2,
        }
    }

    /// Whether the given id was allocated by *this* role.
    pub(crate) fn owns(self, id: StreamId) -> bool {
        match self {
            Role::Client => !id.is_multiple_of(2),
            Role::Server => id != 0 && id.is_multiple_of(2),
        }
    }
}

/// Lock-free, parity-respecting stream id allocator.
///
/// Client-allocated ids are odd (1, 3, 5, …) and server-allocated ids are
/// even (2, 4, 6, …); id `0` is reserved for session-scoped frames such as
/// `Ping` and `GoAway`.
#[derive(Debug)]
pub(crate) struct StreamIdAllocator {
    next: AtomicU32,
}

impl StreamIdAllocator {
    pub(crate) fn new(role: Role) -> Self {
        Self {
            next: AtomicU32::new(role.first_id()),
        }
    }

    /// Allocate the next stream id, advancing by 2. Returns `None` once the
    /// 32-bit id space is exhausted.
    pub(crate) fn allocate(&self) -> Option<StreamId> {
        loop {
            let cur = self.next.load(Ordering::Relaxed);
            let next = cur.checked_add(2)?;
            if self
                .next
                .compare_exchange_weak(cur, next, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return Some(cur);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_with_correct_parity() {
        let client = StreamIdAllocator::new(Role::Client);
        assert_eq!(client.allocate(), Some(1));
        assert_eq!(client.allocate(), Some(3));
        assert_eq!(client.allocate(), Some(5));

        let server = StreamIdAllocator::new(Role::Server);
        assert_eq!(server.allocate(), Some(2));
        assert_eq!(server.allocate(), Some(4));
    }

    #[test]
    fn role_owns_check() {
        assert!(Role::Client.owns(1));
        assert!(!Role::Client.owns(2));
        assert!(Role::Server.owns(2));
        assert!(!Role::Server.owns(0));
        assert!(!Role::Server.owns(1));
    }
}
