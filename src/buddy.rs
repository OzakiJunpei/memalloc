// Let 'h' be the depth of a complete binary tree,
// then the number of nodes is
// 2^(h+1) - 1 = (1 << (h + 1)) - 1
// .
//
// When h = 10,
// 2^10 * min_size
// is maximum bytes of the buddy memory allocator.
//
// u: unused
// x: inner node
// L: used leaf node
// (number) indicates the index of a node
//       x(0)
//     /     \
//    x(1)    L(2)
//  /   \
// u(3) L(4) u(5) u(6)
//
// encoding rule
// 0b00: unused
// 0b01: inner node
// 0b10: used leaf
//
// above tree can be encoded as
// 01   01   10   00   10   00   00
// x(0) x(1) L(2) u(3) L(4) u(5) u(6)

use alloc::alloc::handle_alloc_error;
use core::alloc::Layout;

use synctools::mcs::MCSLock;

#[cfg(feature = "buddy_32m")]
const MAX_DEPTH: usize = 9; // depth of tree

#[cfg(feature = "buddy_64m")]
const MAX_DEPTH: usize = 10; // depth of tree

#[cfg(feature = "buddy_128m")]
const MAX_DEPTH: usize = 11; // depth of tree

#[cfg(feature = "buddy_256m")]
const MAX_DEPTH: usize = 12; // depth of tree

#[cfg(feature = "buddy_512m")]
const MAX_DEPTH: usize = 13; // depth of tree

#[cfg(feature = "buddy_1g")]
const MAX_DEPTH: usize = 14; // depth of tree

#[cfg(feature = "buddy_2g")]
const MAX_DEPTH: usize = 15; // depth of tree

#[cfg(feature = "buddy_4g")]
const MAX_DEPTH: usize = 16; // depth of tree

#[cfg(feature = "buddy_8g")]
const MAX_DEPTH: usize = 17; // depth of tree

#[cfg(feature = "buddy_16g")]
const MAX_DEPTH: usize = 18; // depth of tree

#[cfg(feature = "buddy_32g")]
const MAX_DEPTH: usize = 19; // depth of tree

#[cfg(feature = "buddy_64g")]
const MAX_DEPTH: usize = 19; // depth of tree

#[cfg(feature = "buddy_128g")]
const MAX_DEPTH: usize = 20; // depth of tree

#[cfg(feature = "buddy_256g")]
const MAX_DEPTH: usize = 21; // depth of tree

#[cfg(feature = "buddy_512g")]
const MAX_DEPTH: usize = 22; // depth of tree

#[cfg(feature = "buddy_1t")]
const MAX_DEPTH: usize = 23; // depth of tree

#[cfg(feature = "buddy_2t")]
const MAX_DEPTH: usize = 24; // depth of tree

#[cfg(feature = "buddy_4t")]
const MAX_DEPTH: usize = 25; // depth of tree

#[cfg(feature = "buddy_8t")]
const MAX_DEPTH: usize = 26; // depth of tree

const NUM_NODES: usize = (1 << (MAX_DEPTH + 1)) - 1; // the number of nodes
const NUM_NODES32: usize = (NUM_NODES >> 5) + 1; // #nodes / 32 + 1

const TAG_UNUSED: u64 = 0;
const TAG_INNER: u64 = 1;
const TAG_USED_LEAF: u64 = 2;

static mut BUDDY_ALLOC: Option<MCSLock<BuddyAlloc>> = None;

pub(crate) fn buddy_alloc(layout: Layout) -> *mut u8 {
    unsafe {
        match BUDDY_ALLOC
            .as_ref()
            .expect("buddy allocator is not yet initialized")
            .lock()
            .mem_alloc(layout.size())
        {
            Some(addr) => addr,
            None => handle_alloc_error(layout),
        }
    }
}

pub(crate) fn buddy_dealloc(ptr: *mut u8, _layout: Layout) {
    unsafe {
        BUDDY_ALLOC
            .as_ref()
            .expect("buddy allocator is not yet initialized")
            .lock()
            .mem_free(ptr);
    }
}

/// heap_end = heap_start + 2^MAX_DEPTH * min_size
/// heap_size = heap_end - heap_size
pub(crate) fn init(min_size: usize, heap_start: usize) {
    let buddy = MCSLock::new(BuddyAlloc::new(min_size, heap_start));
    unsafe { BUDDY_ALLOC = Some(buddy) };
}

pub(crate) struct BuddyAlloc {
    min_size: usize,
    start: usize,               // start address
    bitmap: [u64; NUM_NODES32], // succinct structure of the tree
}

enum Tag {
    Unused = TAG_UNUSED as isize,
    Inner = TAG_INNER as isize,
    UsedLeaf = TAG_USED_LEAF as isize,
}

impl BuddyAlloc {
    const fn new(min_size: usize, start: usize) -> BuddyAlloc {
        BuddyAlloc {
            min_size: min_size,
            start: start,
            bitmap: [0; NUM_NODES32],
        }
    }

    fn mem_alloc(&mut self, size: usize) -> Option<*mut u8> {
        self.find_mem(size, (1 << MAX_DEPTH) * self.min_size, 0, 0)
    }

    fn mem_free(&mut self, addr: *mut u8) {
        self.release_mem(addr as usize, (1 << MAX_DEPTH) * self.min_size, 0, 0)
    }

    fn get_tag(&self, idx: usize) -> Tag {
        let i = idx >> 5; // div by 32
        let j = idx & 0b11111;
        match (self.bitmap[i] >> (j * 2)) & 0b11 {
            TAG_UNUSED => Tag::Unused,
            TAG_INNER => Tag::Inner,
            TAG_USED_LEAF => Tag::UsedLeaf,
            _ => panic!("unknown tag"),
        }
    }

    fn set_tag(&mut self, idx: usize, tag: Tag) {
        let i = idx >> 5; // div by 32
        let j = idx & 0b11111;
        let mask = 0b11 << (j * 2);
        let val = self.bitmap[i] & !mask;
        self.bitmap[i] = val | ((tag as u64) << (j * 2));
    }

    fn get_idx(depth: usize, offset: usize) -> usize {
        if depth == 0 {
            0
        } else {
            (1 << depth) - 1 + offset
        }
    }

    fn find_mem(
        &mut self,
        req: usize,   // requested bytes
        bytes: usize, // total bytes of this block
        depth: usize,
        offset: usize, // offset of current node in the depth
    ) -> Option<*mut u8> {
        if req > bytes || depth > MAX_DEPTH {
            return None;
        }

        let idx = BuddyAlloc::get_idx(depth, offset);

        match self.get_tag(idx) {
            Tag::UsedLeaf => None,
            Tag::Unused => {
                let next_bytes = bytes >> 1;
                if next_bytes >= req && depth < MAX_DEPTH {
                    // divide
                    self.set_tag(idx, Tag::Inner);
                    self.find_mem(req, next_bytes, depth + 1, offset * 2)
                } else {
                    self.set_tag(idx, Tag::UsedLeaf);
                    Some((self.start + bytes * offset) as *mut u8)
                }
            }
            Tag::Inner => match self.find_mem(req, bytes >> 1, depth + 1, offset * 2) {
                None => self.find_mem(req, bytes >> 1, depth + 1, offset * 2 + 1),
                ret => ret,
            },
        }
    }

    fn release_mem(&mut self, addr: usize, bytes: usize, depth: usize, offset: usize) {
        let idx = BuddyAlloc::get_idx(depth, offset);
        match self.get_tag(idx) {
            Tag::Unused => {
                panic!("freed unused memory");
            }
            Tag::UsedLeaf => {
                let target = self.start + bytes * offset;
                if target == addr {
                    self.set_tag(idx, Tag::Unused);
                } else {
                    panic!("freed invalid address");
                }
            }
            Tag::Inner => {
                let pivot = self.start + bytes * offset + (bytes >> 1);
                if addr < pivot {
                    self.release_mem(addr, bytes >> 1, depth + 1, offset * 2);
                } else {
                    self.release_mem(addr, bytes >> 1, depth + 1, offset * 2 + 1);
                }

                // combine buddy if both blocks are unused
                let left = BuddyAlloc::get_idx(depth + 1, offset * 2);
                let right = BuddyAlloc::get_idx(depth + 1, offset * 2 + 1);
                match self.get_tag(left) {
                    Tag::Unused => match self.get_tag(right) {
                        Tag::Unused => {
                            self.set_tag(idx, Tag::Unused);
                        }
                        _ => (),
                    },
                    _ => (),
                }
            }
        }
    }

    // pub fn print(&self) {
    //     for i in 0..(1 << (MAX_DEPTH + 1)) - 1 {
    //         uart::puts("idx = ");
    //         uart::decimal(i as u64);
    //         uart::puts(", tag = ");
    //         match self.get_tag(i) {
    //             Tag::Unused => uart::puts("unused\n"),
    //             Tag::Inner => uart::puts("inner\n"),
    //             Tag::UsedLeaf => uart::puts("used leaf\n"),
    //         }
    //     }
    // }
}
