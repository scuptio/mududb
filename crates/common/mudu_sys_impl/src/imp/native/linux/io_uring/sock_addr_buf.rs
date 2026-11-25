//! Socket address buffer for io_uring operations.

/// Owned copy of a `sockaddr_storage` with its length, for passing
/// socket addresses to io_uring operations.
#[derive(Clone, Copy)]
pub struct SockAddrBuf {
    raw: rliburing::sockaddr_storage,
    len: u32,
}

impl SockAddrBuf {
    /// Creates a zeroed buffer sized for a full `sockaddr_storage`.
    pub fn new_empty() -> Self {
        Self {
            raw: unsafe { std::mem::zeroed() },
            len: std::mem::size_of::<rliburing::sockaddr_storage>() as u32,
        }
    }

    /// Returns the length of the stored address in bytes.
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Returns `true` if no address is stored.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Constructs from a raw `sockaddr_storage` and length.
    pub fn from_raw(raw: rliburing::sockaddr_storage, len: u32) -> Self {
        Self { raw, len }
    }

    /// The raw `sockaddr_storage`.
    pub fn raw(&self) -> &rliburing::sockaddr_storage {
        &self.raw
    }

    /// Raw pointer to the address as `sockaddr`.
    pub fn sockaddr_ptr(&self) -> *const rliburing::sockaddr {
        (&self.raw as *const rliburing::sockaddr_storage).cast()
    }

    /// Mutable raw pointer to the address as `sockaddr`.
    pub fn sockaddr_mut_ptr(&mut self) -> *mut rliburing::sockaddr {
        (&mut self.raw as *mut rliburing::sockaddr_storage).cast()
    }

    /// The address length in bytes.
    pub fn socklen(&self) -> rliburing::socklen_t {
        self.len
    }

    /// Mutable pointer to the address length.
    pub fn socklen_mut_ptr(&mut self) -> *mut rliburing::socklen_t {
        &mut self.len
    }
}

/// Backward-compatible alias.
pub type SocketAddrBuf = SockAddrBuf;
