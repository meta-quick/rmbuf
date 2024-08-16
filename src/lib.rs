use std::alloc::{alloc, dealloc, realloc, Layout};
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::ops::{Deref, DerefMut};
use std::ptr;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct MBuf {
    buffer: *mut u8,  // Raw pointer to the start of the buffer
    data_len: usize,  // Length of the valid data in the buffer
    buffer_len: usize, // Total length of the buffer
    pos: usize,       // Current read/write position within the buffer
}

impl MBuf {
    pub fn new(buffer_len: usize) -> Self {
        unsafe {
            let layout = Layout::from_size_align(buffer_len, 8).unwrap();
            let buffer = alloc(layout) as *mut u8;
            if buffer.is_null() {
                panic!("Failed to allocate buffer");
            }

            MBuf {
                buffer,
                data_len: 0,
                buffer_len,
                pos: 0,
            }
        }
    }

    fn grow(&mut self, additional: usize) -> Result<(), io::Error> {
        let new_buffer_len = self.buffer_len + additional;
        unsafe {
            let layout = Layout::from_size_align(self.buffer_len, 8).unwrap();
            let new_buffer = realloc(self.buffer, layout, new_buffer_len) as *mut u8;
            if new_buffer.is_null() {
                return Err(io::Error::last_os_error());
            }

            self.buffer = new_buffer;
            self.buffer_len = new_buffer_len;
            Ok(())
        }
    }

    pub fn append(&mut self, data: &[u8]) -> Result<(), io::Error> {
        if self.data_len + data.len() > self.buffer_len {
           let result =  self.grow(data.len());
           if result.is_err() {
               return result;
           }
        }

        unsafe {
            ptr::copy_nonoverlapping(data.as_ptr(), self.buffer.add(self.data_len), data.len());
        }
        self.data_len += data.len();
        Ok(())
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.buffer, self.data_len) }
    }

    pub fn data(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.buffer, self.data_len) }
    }

    pub fn set_data(&mut self, data: &[u8]) {
        if data.len() > self.buffer_len {
            panic!("Data length exceeds buffer capacity");
        }

        unsafe {
            ptr::copy_nonoverlapping(data.as_ptr(), self.buffer, data.len());
        }
        self.data_len = data.len();
        self.pos = 0;  // Reset position after writing data
    }

    pub fn clear(&mut self) {
        self.data_len = 0;
        self.pos = 0;
    }
}

impl Deref for MBuf {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.data()
    }
}

pub struct MBufPool {
    pool: HashMap<usize, Vec<MBuf>>,
}

impl MBufPool {
    pub fn new() -> Self {
        MBufPool {
            pool: HashMap::new(),
        }
    }

    pub fn initialize(&mut self) {
        //add 1k size
        for _ in 0..100 {
            self.pool.insert(1024, vec![MBuf::new(1024)]);
        }

        for _ in 0..100 {
            self.pool.insert(2048, vec![MBuf::new(2048)]);
        }

        for _ in 0..20 {
            self.pool.insert(4096, vec![MBuf::new(4096)]);
        }

        for _ in 0..10 {
            self.pool.insert(8192, vec![MBuf::new(8192)]);
        }

        for _ in 0..5 {
            self.pool.insert(16384, vec![MBuf::new(16384)]);
        }
        for _ in 0..2 {
            self.pool.insert(32768, vec![MBuf::new(32768)]);
        }
    }

    pub fn take(&mut self, size: usize) -> Option<MBuf> {
        //adjust size
        let size = if size < 1024 {
            1024
        } else if size < 2048 {
            2048
        } else if size < 4096 {
            4096
        } else if size < 8192 {
            8192
        } else if size < 16384 {
            16384
        } else if size < 32768 {
            32768
        } else {
            return None;
        };

        if let Some(mbufs) = self.pool.get_mut(&size) {
            return if let Some(buf) = mbufs.pop() {
                Some(buf)
            } else {
                Some(MBuf::new(size))
            }
        }
        None
    }

    pub fn give(&mut self, buf: MBuf) {
        let size = buf.buffer_len;
        if let Some(mbufs) = self.pool.get_mut(&size) {
            mbufs.push(buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mbuf_all_operations() {
       let mut mbuf = MBuf::new(1);
       mbuf.append(b"Hello, world!").unwrap();
       assert_eq!(mbuf.data(), b"Hello, world!");

       mbuf.set_data(b"Hello, world!");
       assert_eq!(mbuf.data(), b"Hello, world!");

       mbuf.clear();
       assert_eq!(mbuf.data(), b"");

       mbuf.append(b"Hello, world!").expect("TODO: panic message");
       let data = mbuf.data_mut();
       data[0] = b'h';
       assert_eq!(mbuf.data(), b"hello, world!")
    }

    #[test]
    fn test_mbuf_pool() {
        let mut pool = MBufPool::new();
        pool.initialize();

        let buf1 = pool.take(1024).unwrap();
        let buf2 = pool.take(2048).unwrap();
        let buf3 = pool.take(4096).unwrap();
        let buf4 = pool.take(8192).unwrap();
        let buf5 = pool.take(16384).unwrap();

        pool.give(buf1);
        pool.give(buf2);

        let mut buf6 = pool.take(1023).unwrap();
        assert_eq!(buf6.data(), b"");
        buf6.append(b"Hello, world!").unwrap();
        assert_eq!(buf6.data(), b"Hello, world!");
    }
}
