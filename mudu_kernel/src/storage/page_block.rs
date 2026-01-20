use std::ops::Range;
use mudu::common::endian;
use crate::storage::constant;
use crate::storage::constant::PAGE_TAIL_SIZE;

pub struct PageBlock {
    data: Vec<u8>
}

pub struct PageUpdate {
    page_id: u64,
    offset: u64,
    update_data: Vec<u8>
}

impl PageUpdate {
    pub fn new(page_id:u64, offset:u64, update_data: Vec<u8>) -> PageUpdate {
        Self {page_id, offset, update_data}
    }
    
    pub fn page_id(&self) -> u64 {
        self.page_id
    }
    
    pub fn offset(&self) -> u64 {
        self.offset
    }
    
    pub fn update_data(&self) -> &[u8] {
        &self.update_data
    }
}

impl PageBlock {
    pub fn new_empty(page_size:u64) -> PageBlock {
        Self { data:vec![0;page_size as usize] }
    }

    pub fn new(data: Vec<u8>) -> PageBlock {
        Self { data }
    }

    pub fn get_page_id(&self) -> u64 {
        endian::read_u64(&self.data[constant::page_offset_range_page_id()])
    }

    pub fn get_lsn(&self) -> u64 {
        endian::read_u64(&self.data[constant::page_offset_range_lsn()])
    }

    pub fn set_page_id(&mut self, page_id:u64) {
        endian::write_u64(&mut self.data[constant::page_offset_range_page_id()], page_id);
    }

    pub fn set_lsn(&mut self, lsn:u64) {
        endian::write_u64(&mut self.data[constant::page_offset_range_lsn()], lsn);
    }

    pub fn update_checksum(&self) {
        
    }
    pub fn update_set_lsn(&mut self, lsn:u64) -> PageUpdate {
        let range = constant::page_offset_range_lsn();
        let page_id = self.get_page_id();
        let offset = range.start as _;
        let update_data = &mut self.data[range];
        endian::write_u64(update_data, lsn);
        PageUpdate {
            page_id,
            offset,
            update_data: update_data.to_vec()
        }
    }

    pub fn set_checksum(&mut self, checksum:u64) {
        let range = self.range_checksum();
        endian::write_u64(&mut self.data[range], checksum);
    }

    pub fn get_checksum(&self) -> Checksum {
        let range = self.range_checksum();
        endian::read_u64(&self.data[range])
    }

    pub fn range_checksum(&self) -> Range<usize> {
        self.data.len() - size_of::<Checksum>()..self.data.len()
    }

    pub fn block(&self) -> &[u8] {
        if self.data.is_empty() {
            println!("Empty page block");
        }
        &self.data
    }

    pub fn block_mut(&mut self) -> &mut [u8] {
        if self.data.is_empty() {
            panic!("Empty page block");
        }
        &mut self.data
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn set_block(&mut self, block:&mut PageBlock) {
        self.swap(block);
    }

    pub fn use_block(&mut self) -> Vec<u8> {
        let mut vec = Vec::new();
        std::mem::swap(&mut self.data, &mut vec);
        vec
    }

    pub fn swap(&mut self, new_block:&mut PageBlock) {
        std::mem::swap(&mut self.data, &mut new_block.data);
    }

    pub fn copy_block(&mut self, block:&mut PageBlock) {
        block.data.resize(self.data.len(), 0);
        block.data.copy_from_slice(&self.data);
    }

    pub fn update_block(&mut self, block:&mut PageBlock) {

    }
    
    pub fn reset_checksum(&mut self) {
        let checksum = self.calculate_checksum();
        self.set_checksum(checksum);
    }
    
    fn calculate_checksum(&self) -> Checksum {
        self.data[0.. self.range_checksum().start as _]
            .iter()
            .fold(0u64,
                  |acc, &byte| acc.wrapping_add(byte as u64)
            )
    }
}

pub type Checksum = u64;