use std::ops::Range;

pub const PAGE_TAIL_SIZE:usize = 64usize;
pub const PAGE_HEADER_SIZE:usize = 64usize;

pub const DATA_HEADER_SIZE:usize = 64usize;
pub const EXTEND_HEADER_SIZE:usize = 64usize;

pub fn offset_page_id() -> usize {
    0
}

pub fn size_page_id() -> usize {
    size_of::<u64>()
}

pub fn offset_lsn() -> usize {
    size_of::<u64>()
}

pub fn size_lsn() -> usize {
    size_of::<u64>()
}

pub fn page_offset_range_page_id() -> Range<usize> {
    offset_page_id()..offset_page_id() + size_page_id()
}

pub fn page_offset_range_lsn() -> Range<usize> {
    offset_lsn()..offset_lsn()+ size_lsn()
}

fn payload_offset_extent_meta_range() -> Range<usize> {
    0..PAGE_HEADER_SIZE + EXTEND_HEADER_SIZE
}

struct OffsetPage {

}

impl OffsetPage {
    pub fn total() -> Range<usize> {
        0..PAGE_HEADER_SIZE
    }

    pub fn lsn() -> Range<usize> {
        page_offset_range_lsn()
    }

    pub fn page_id() -> Range<usize> {
        page_offset_range_page_id()
    }
}
pub struct LayoutExtentHeader {

}

pub struct LayoutDataHeader {

}

impl LayoutExtentHeader {
    pub fn size() -> usize {
        EXTEND_HEADER_SIZE
    }

    pub fn range_of_page() -> Range<usize> {
        PAGE_HEADER_SIZE..PAGE_HEADER_SIZE + EXTEND_HEADER_SIZE
    }

    pub fn range() -> Range<usize> {
        payload_offset_extent_meta_range()
    }

    pub fn range_extent_id() -> Range<usize> {
        0..size_of::<u64>()
    }

    pub fn range_start_page() -> Range<usize> {
        let start = Self::range_extent_id().end;
        start ..start + size_of::<u64>()
    }

    pub fn range_page_count() -> Range<usize> {
        let start = Self::range_start_page().end;
        start ..start + size_of::<u64>()
    }

    pub fn offset_bitmap() -> usize {
        EXTEND_HEADER_SIZE
    }
}

impl LayoutDataHeader {
    fn range_of_page() -> Range<usize> {
        PAGE_HEADER_SIZE.. PAGE_HEADER_SIZE + DATA_HEADER_SIZE
    }

    fn range_free_begin() -> Range<usize> {
        0..size_of::<u64>()
    }

    fn range_free_end_page() -> Range<usize> {
        let start = Self::range_free_begin().end;
        start ..start + size_of::<u64>()
    }
}