use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use mcslock::raw::Mutex;
use mcslock::relax::Spin;
use tokio::sync::RwLock;
use mudu::common::endian;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use crate::storage::constant::LayoutExtentHeader;

struct ExtentMeta {
    // file id and table space id would not to be persistent
    file_id:u64,
    table_space_id:u64,
    // extent header info
    extent_id: u64,
    start_page: u64,
    page_count: u64,
}

struct ExtentData {
    meta: ExtentMeta,
    payload:Mutex<ExtentPayload, Spin>,
}
#[derive(Clone)]
pub struct Extent {
    inner:Arc<ExtentData>
}

impl Extent {
    pub fn extent_id(&self) -> u64 {
        self.inner.meta.extent_id
    }

    pub fn new(
        file_id:u64,
        table_space_id:u64,
        extent_id: u64,
        start_page: u64,
        page_count: u64,
    ) -> Extent {
        Self {
            inner:Arc::new(
                ExtentData {
                    meta : ExtentMeta::new(
                        file_id, table_space_id,
                        extent_id, start_page, page_count),
                    payload: Mutex::new(ExtentPayload::new(page_count)),
                }
            )
        }
    }


    // from binary exclude page header and page tailer
    pub fn from(
        file_id:u64,
        table_space_id:u64,
        slice:&[u8]) -> RS<Extent> {
        let meta = ExtentMeta::from(
            file_id, table_space_id,
            slice)?;
        let payload = ExtentPayload::from(slice, &meta)?;
        Ok(Self {
            inner:Arc::new(ExtentData{
                meta,
                payload: Mutex::new(payload)
            })
        })
    }

    pub fn extent_allocate_page(&self) -> Option<u64> {
        self.inner.payload.lock_then(|payload|{
            payload.allocate_page()
        })
    }

    pub fn file_id(&self) -> u64 {
        self.inner.meta.file_id
    }
}

pub struct ExtentPayload {
    bitmap: QuadBitmap,
    next_write_page:u64,
    full_page_count:u64,
}

impl ExtentMeta {
    fn from(
        file_id:u64,
        table_space_id:u64,
        vec:&[u8]) -> RS<ExtentMeta> {
        if vec.len() < LayoutExtentHeader::size() {
            return Err(m_error!(EC::FatalError, "cannot fit into extent meta data"))
        }
        let extent_id = endian::read_u64(&vec[LayoutExtentHeader::range_extent_id()]);
        let start_page = endian::read_u64(&vec[LayoutExtentHeader::range_start_page()]);
        let page_count = endian::read_u64(&vec[LayoutExtentHeader::range_page_count()]);
        Ok(Self {
            file_id,
            table_space_id,
            extent_id,
            start_page,
            page_count,
        })
    }

    fn new(
        file_id:u64,
        tablespace_id:u64,
        extent_id: u64,
        start_page: u64,
        page_count: u64,
    ) -> Self {
        Self {
            file_id: 0,
            table_space_id: 0,
            extent_id,
            start_page,
            page_count,
        }
    }

    fn data_pages(&self) -> u64 {
        self.page_count - 1
    }

    fn to_vec(&self) -> Vec<u8> {
        let n = LayoutExtentHeader::size();
        let mut vec = Vec::with_capacity(n);
        vec.resize(n, 0);
        endian::write_u64(&mut vec[LayoutExtentHeader::range_extent_id()], self.extent_id);
        endian::write_u64(&mut vec[LayoutExtentHeader::range_start_page()], self.start_page);
        endian::write_u64(&mut vec[LayoutExtentHeader::range_page_count()], self.page_count);
        vec
    }
}

impl ExtentPayload {
    fn from(vec:&[u8], meta:&ExtentMeta) -> RS<Self> {
        let bitmap_off = LayoutExtentHeader::offset_bitmap();
        let bytes = Self::bytes_of_bitmap(meta.data_pages());
        let bitmap = QuadBitmap::from(&vec[bitmap_off.. bitmap_off + bytes]);
        let (next_write_page, _) = bitmap.find_first_state012().unwrap_or((0, QuadState::State0));
        let full_page_count = meta.data_pages() - bitmap.count_state012() as u64;
        let extent = Self {
            bitmap,
            next_write_page:next_write_page as _,
            full_page_count,
        };
        Ok(extent)
    }

    fn bytes_of_bitmap(page_count:u64) -> usize {
        (page_count as usize)/4
    }

    fn new(pages:u64) -> Self {
        let bitmap = QuadBitmap::new(pages as usize);
        Self {
            bitmap,
            next_write_page: 0,
            full_page_count: 0,
        }
    }

    /// Allocate a page from this extent
    pub fn allocate_page(&mut self) -> Option<u64> {
        if let Some(page_offset) = self.find_free() {
            self.bitmap.set(page_offset, QuadState::State1);
            Some(page_offset as u64)
        } else {
            None
        }
    }

    fn find_free(&mut self) -> Option<usize> {
        self.bitmap.find_first_state0()
    }

    /// Free a page in this extent
    fn free_page(&mut self, page_number: u64, meta:&ExtentMeta) -> bool {
        if page_number >= meta.start_page && page_number < meta.start_page + meta.data_pages() {
            let offset = (page_number - meta.start_page) as usize;
            self.bitmap.set(offset, QuadState::State0);
            true
        } else {
            false
        }
    }

    /// Check if extent has free pages
    pub fn has_free_pages(&self) -> bool {
        self.bitmap.find_first_state0().is_some()
    }

    /// Get number of free pages in this extent
    fn free_page_count(&self) -> usize {
        self.bitmap.count_state0()
    }

    fn to_vec(&self, meta:&ExtentMeta) -> Vec<u8> {
        let size = self.bitmap.data.len();
        let mut vec = Vec::with_capacity(size);
        vec.resize(size, 0);
        vec[LayoutExtentHeader::offset_bitmap()..].copy_from_slice(&self.bitmap.data);
        vec
    }

    fn size(meta:&ExtentMeta) -> usize {
        LayoutExtentHeader::size() + (meta.data_pages() as usize) / 4
    }
}

/// Four-state enumeration, each state represented by 2 bits
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuadState {
    State0 = 0b00,  // 00
    State1 = 0b01,  // 01
    State2 = 0b10,  // 10
    State3 = 0b11,  // 11
}

/// Bitmap that stores QuadState values efficiently (2 bits per state)
pub struct QuadBitmap {
    // Storage vector where each byte holds 4 QuadState values
    data: Vec<u8>,
    // Number of states stored in the bitmap
    len: usize,
}

impl QuadBitmap {
    /// Creates a new bitmap with the specified capacity
    /// Each byte can store 4 states, so we calculate the required byte capacity
    pub fn new(capacity: usize) -> Self {
        // Calculate bytes needed: (capacity + 3) / 4 rounds up to nearest byte
        let byte_capacity = (capacity + 3) / 4;
        Self {
            data: vec![0; byte_capacity],
            len: capacity,
        }
    }

    pub fn from(vec:&[u8]) -> Self {
        let len = vec.len() * 4;
        Self {
            data:vec.to_vec(),
            len
        }
    }

    /// Returns the number of states in the bitmap
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the bitmap contains no states
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Gets the state at the specified index
    /// Returns None if index is out of bounds
    pub fn get(&self, index: usize) -> Option<QuadState> {
        if index >= self.len {
            return None;
        }

        let byte_index = index / 4;
        let bit_offset = (index % 4) * 2;
        let mask = 0b11 << bit_offset;
        let value = (self.data[byte_index] & mask) >> bit_offset;

        match value {
            0b00 => Some(QuadState::State0),
            0b01 => Some(QuadState::State1),
            0b10 => Some(QuadState::State2),
            0b11 => Some(QuadState::State3),
            _ => unreachable!(), // Should never happen since we only use 2 bits
        }
    }

    /// Sets the state at the specified index
    /// Returns Err if index is out of bounds
    pub fn set(&mut self, index: usize, state: QuadState) -> Result<(), &'static str> {
        if index >= self.len {
            return Err("Index out of bounds");
        }

        let byte_index = index / 4;
        let bit_offset = (index % 4) * 2;
        let mask = 0b11 << bit_offset;

        // Clear the existing bits and set new value
        self.data[byte_index] &= !mask;
        self.data[byte_index] |= (state as u8) << bit_offset;

        Ok(())
    }

    /// Returns an iterator over all states in the bitmap
    pub fn iter(&self) -> QuadStateBitmapIter<'_> {
        QuadStateBitmapIter {
            bitmap: self,
            index: 0,
        }
    }

    /// Finds the first occurrence of State0 in the bitmap
    /// Returns Some(index) if found, None otherwise
    pub fn find_first_state0(&self) -> Option<usize> {
        self.find_first_state0_from(0)
    }

    /// Finds the first occurrence of State0 starting from the specified position
    /// Returns Some(index) if found, None otherwise
    pub fn find_first_state0_from(&self, start: usize) -> Option<usize> {
        if start >= self.len {
            return None;
        }

        // Calculate starting byte and bit offset within that byte
        let start_byte = start / 4;
        let start_bit_offset = (start % 4) * 2;

        // Iterate through each byte in storage
        for byte_index in start_byte..self.data.len() {
            let byte = self.data[byte_index];

            // Check each of the 4 states stored in this byte
            for bit_offset in 0..4 {
                let global_index = byte_index * 4 + bit_offset;

                // Skip states before our starting position
                if byte_index == start_byte && bit_offset < start_bit_offset / 2 {
                    continue;
                }

                // Check if we've exceeded the bitmap length
                if global_index >= self.len {
                    return None;
                }

                // Extract the 2-bit state value
                let mask = 0b11 << (bit_offset * 2);
                let value = (byte & mask) >> (bit_offset * 2);

                // Check if this is State0
                if value == QuadState::State0 as u8 {
                    return Some(global_index);
                }
            }
        }

        None
    }

    /// Finds the first occurrence of State0, State1, or State2 in the bitmap
    /// Returns Some((index, state)) if found, None if only State3 exists
    pub fn find_first_state012(&self) -> Option<(usize, QuadState)> {
        self.find_first_state012_from(0)
    }

    /// Finds the first occurrence of State0, State1, or State2 starting from specified position
    /// Returns Some((index, state)) if found, None if only State3 exists from start position
    pub fn find_first_state012_from(&self, start: usize) -> Option<(usize, QuadState)> {
        if start >= self.len {
            return None;
        }

        let start_byte = start / 4;
        let start_bit_offset = (start % 4) * 2;

        for byte_index in start_byte..self.data.len() {
            let byte = self.data[byte_index];

            for bit_offset in 0..4 {
                let global_index = byte_index * 4 + bit_offset;

                // Skip states before our starting position
                if byte_index == start_byte && bit_offset < start_bit_offset / 2 {
                    continue;
                }

                if global_index >= self.len {
                    return None;
                }

                let mask = 0b11 << (bit_offset * 2);
                let value = (byte & mask) >> (bit_offset * 2);

                // Check if this is State0, State1, or State2 (any value except 0b11)
                if value != QuadState::State3 as u8 {
                    let state = match value {
                        0b00 => QuadState::State0,
                        0b01 => QuadState::State1,
                        0b10 => QuadState::State2,
                        _ => unreachable!(), // We already filtered out State3
                    };
                    return Some((global_index, state));
                }
            }
        }

        None
    }

    /// Optimized version for finding State0, State1, or State2
    pub fn find_first_state012_fast(&self) -> Option<(usize, QuadState)> {
        self.find_first_state012_from_fast(0)
    }

    /// Optimized version using bit operations for faster search
    pub fn find_first_state012_from_fast(&self, start: usize) -> Option<(usize, QuadState)> {
        if start >= self.len {
            return None;
        }

        let start_byte = start / 4;
        let start_offset = start % 4;

        for byte_index in start_byte..self.data.len() {
            let mut byte = self.data[byte_index];

            // Mask out bits before start position for the starting byte
            if byte_index == start_byte && start_offset > 0 {
                let mask = (1 << (start_offset * 2)) - 1;
                byte &= !mask;
            }

            // Fast path: if byte is not 0xFF, it contains at least one non-State3 value
            if byte != 0xFF {
                for bit_offset in 0..4 {
                    let global_index = byte_index * 4 + bit_offset;

                    // Skip states before start position
                    if byte_index == start_byte && bit_offset < start_offset {
                        continue;
                    }

                    if global_index >= self.len {
                        return None;
                    }

                    let mask = 0b11 << (bit_offset * 2);
                    let value = (byte & mask) >> (bit_offset * 2);

                    if value != QuadState::State3 as u8 {
                        let state = match value {
                            0b00 => QuadState::State0,
                            0b01 => QuadState::State1,
                            0b10 => QuadState::State2,
                            _ => unreachable!(),
                        };
                        return Some((global_index, state));
                    }
                }
            }
        }

        None
    }

    /// Finds all positions that contain State0, State1, or State2
    /// Returns a vector of (index, state) tuples
    pub fn find_all_state012(&self) -> Vec<(usize, QuadState)> {
        let mut results = Vec::new();
        let mut current_pos = 0;

        while let Some((pos, state)) = self.find_first_state012_from(current_pos) {
            results.push((pos, state));
            current_pos = pos + 1;
        }

        results
    }

    /// Finds the first occurrence of a specific state
    /// Returns Some(index) if found, None otherwise
    pub fn find_first_state(&self, target_state: QuadState) -> Option<usize> {
        self.find_first_state_from(target_state, 0)
    }

    /// Finds the first occurrence of a specific state starting from specified position
    pub fn find_first_state_from(&self, target_state: QuadState, start: usize) -> Option<usize> {
        if start >= self.len {
            return None;
        }

        let start_byte = start / 4;
        let start_bit_offset = (start % 4) * 2;

        for byte_index in start_byte..self.data.len() {
            let byte = self.data[byte_index];

            for bit_offset in 0..4 {
                let global_index = byte_index * 4 + bit_offset;

                if byte_index == start_byte && bit_offset < start_bit_offset / 2 {
                    continue;
                }

                if global_index >= self.len {
                    return None;
                }

                let mask = 0b11 << (bit_offset * 2);
                let value = (byte & mask) >> (bit_offset * 2);

                if value == target_state as u8 {
                    return Some(global_index);
                }
            }
        }

        None
    }

    /// Returns count of indices which is State0
    pub fn count_state0(&self) -> usize {
        let mut n = 0;
        let mut current_pos = 0;
        while let Some(pos) = self.find_first_state0_from(current_pos) {
            n += 1;
            current_pos = pos + 1;
        }
        n
    }

    pub fn count_state012(&self) -> usize {
        let mut n = 0;
        let mut current_pos = 0;
        while let Some((pos, _)) = self.find_first_state012_from(current_pos) {
            n += 1;
            current_pos = pos + 1;
        }
        n
    }

    /// Finds all positions that contain State0
    /// Returns a vector of indices where State0 is found
    pub fn find_all_state0(&self) -> Vec<usize> {
        let mut positions = Vec::new();
        let mut current_pos = 0;

        while let Some(pos) = self.find_first_state0_from(current_pos) {
            positions.push(pos);
            current_pos = pos + 1;
        }

        positions
    }

    /// Sets a range of states starting from the specified position
    /// Returns Err if the range would exceed bitmap bounds
    pub fn set_range(&mut self, start: usize, states: &[QuadState]) -> Result<(), &'static str> {
        if start + states.len() > self.len {
            return Err("Range out of bounds");
        }

        for (i, &state) in states.iter().enumerate() {
            self.set(start + i, state)?;
        }
        Ok(())
    }

    /// Creates a bitmap from a slice of QuadState values
    pub fn from_slice(states: &[QuadState]) -> Self {
        let mut bitmap = Self::new(states.len());
        for (i, &state) in states.iter().enumerate() {
            bitmap.set(i, state).unwrap();
        }
        bitmap
    }
}

/// Iterator for QuadStateBitmap
pub struct QuadStateBitmapIter<'a> {
    bitmap: &'a QuadBitmap,
    index: usize,
}

impl<'a> Iterator for QuadStateBitmapIter<'a> {
    type Item = QuadState;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.bitmap.len {
            None
        } else {
            let state = self.bitmap.get(self.index).unwrap();
            self.index += 1;
            Some(state)
        }
    }
}

impl std::fmt::Debug for QuadBitmap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "QuadStateBitmap[")?;
        for (i, state) in self.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{:?}", state)?;
        }
        write!(f, "]")
    }
}


#[cfg(test)]
mod tests {
    use super::*;


    // Example usage and demonstration
    #[test]
    fn test() {
        // Test case: Mixed states
        let mut bitmap = QuadBitmap::new(10);

        // Set various states
        bitmap.set(0, QuadState::State3).unwrap();  // Skip this one
        bitmap.set(1, QuadState::State3).unwrap();  // Skip this one
        bitmap.set(2, QuadState::State1).unwrap();  // First State0/1/2
        bitmap.set(3, QuadState::State3).unwrap();  // Skip
        bitmap.set(4, QuadState::State0).unwrap();  // Second State0/1/2
        bitmap.set(5, QuadState::State2).unwrap();  // Third State0/1/2
        bitmap.set(6, QuadState::State3).unwrap();  // Skip
        bitmap.set(7, QuadState::State3).unwrap();  // Skip

        println!("Bitmap: {:?}", bitmap);

        // Find first State0, State1, or State2
        if let Some((pos, state)) = bitmap.find_first_state012() {
            println!("First State0/1/2 at position {}: {:?}", pos, state);  // Expected: position 2, State1
        } else {
            println!("No State0, State1, or State2 found");
        }

        // Find from specific position
        if let Some((pos, state)) = bitmap.find_first_state012_from(3) {
            println!("First State0/1/2 from position 3: {} - {:?}", pos, state);  // Expected: position 4, State0
        }

        // Find all State0/1/2 positions
        let all_state012 = bitmap.find_all_state012();
        println!("All State0/1/2 positions: {:?}", all_state012);  // Expected: [(2, State1), (4, State0), (5, State2)]

        // Test case: Only State3
        let mut bitmap2 = QuadBitmap::new(4);
        bitmap2.set(0, QuadState::State3).unwrap();
        bitmap2.set(1, QuadState::State3).unwrap();
        bitmap2.set(2, QuadState::State3).unwrap();
        bitmap2.set(3, QuadState::State3).unwrap();

        match bitmap2.find_first_state012() {
            Some((pos, state)) => println!("Found {:?} at {}", state, pos),
            None => println!("No State0, State1, or State2 found in bitmap2"),  // Expected output
        }

        // Test optimized version
        if let Some((pos, state)) = bitmap.find_first_state012_fast() {
            println!("Fast search found: {} - {:?}", pos, state);
        }
    }

    #[test]
    fn test_find_first_state012() {
        let mut bitmap = QuadBitmap::new(6);
        bitmap.set(0, QuadState::State3).unwrap();
        bitmap.set(1, QuadState::State3).unwrap();
        bitmap.set(2, QuadState::State1).unwrap();  // First match
        bitmap.set(3, QuadState::State0).unwrap();
        bitmap.set(4, QuadState::State3).unwrap();
        bitmap.set(5, QuadState::State2).unwrap();

        let result = bitmap.find_first_state012();
        assert_eq!(result, Some((2, QuadState::State1)));
    }

    #[test]
    fn test_find_first_state012_from() {
        let mut bitmap = QuadBitmap::new(8);
        bitmap.set(1, QuadState::State0).unwrap();
        bitmap.set(3, QuadState::State1).unwrap();
        bitmap.set(5, QuadState::State2).unwrap();

        assert_eq!(bitmap.find_first_state012_from(5), Some((5, QuadState::State2)));
    }

    #[test]
    fn test_find_first_state012_only_state3() {
        let mut bitmap = QuadBitmap::new(3);
        bitmap.set(0, QuadState::State3).unwrap();
        bitmap.set(1, QuadState::State3).unwrap();
        bitmap.set(2, QuadState::State3).unwrap();

        assert_eq!(bitmap.find_first_state012(), None);
        assert_eq!(bitmap.find_first_state012_from(1), None);
    }

    #[test]
    fn test_find_all_state012() {
        let bitmap = QuadBitmap::from_slice(&[
            QuadState::State3,
            QuadState::State0,
            QuadState::State3,
            QuadState::State1,
            QuadState::State2,
            QuadState::State3,
        ]);

        let expected = vec![
            (1, QuadState::State0),
            (3, QuadState::State1),
            (4, QuadState::State2),
        ];
        assert_eq!(bitmap.find_all_state012(), expected);
    }

    #[test]
    fn test_find_first_state() {
        let mut bitmap = QuadBitmap::new(5);
        bitmap.set(0, QuadState::State0).unwrap();
        bitmap.set(1, QuadState::State1).unwrap();
        bitmap.set(2, QuadState::State0).unwrap();
        bitmap.set(3, QuadState::State2).unwrap();
        bitmap.set(4, QuadState::State0).unwrap();

        assert_eq!(bitmap.find_first_state(QuadState::State0), Some(0));
        assert_eq!(bitmap.find_first_state(QuadState::State1), Some(1));
        assert_eq!(bitmap.find_first_state(QuadState::State2), Some(3));
        assert_eq!(bitmap.find_first_state(QuadState::State3), None);
        assert_eq!(bitmap.find_first_state_from(QuadState::State0, 1), Some(2));
    }

    #[test]
    fn test_fast_vs_normal_search() {
        let mut bitmap = QuadBitmap::new(10);
        bitmap.set(7, QuadState::State1).unwrap();

        // Both methods should return the same result
        assert_eq!(bitmap.find_first_state012(), bitmap.find_first_state012_fast());
        assert_eq!(bitmap.find_first_state012_from(3), bitmap.find_first_state012_from_fast(3));
    }
}