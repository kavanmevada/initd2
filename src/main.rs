extern crate alloc;

#[macro_export]
macro_rules! syscall {
    ($fn:ident $args:tt) => {{
        use libc::strlen;
        use libc::strerror;
        use core::str::from_utf8;
        use core::slice::from_raw_parts;
        let res = unsafe { libc::$fn $args };
        if res == -1 {
            let ptr = unsafe { strerror(*libc::__errno_location())};
            Err(from_utf8(unsafe { from_raw_parts(ptr as *const u8, strlen(ptr) as usize) }).unwrap_or_default())
        } else {
            Ok(res)
        }
    }};
}

fn main() {
    println!("Hello, world!");

    let mut poller = manager::Manager::<10, 10>::init().expect("Erro initializing Manager!");

    poller
        .socket("test.service", "example.sock")
        .expect("Error creating socket!");

    std::thread::spawn(|| {
        std::thread::sleep(core::time::Duration::from_secs(5));
        let sock = syscall!(socket(libc::AF_UNIX, libc::SOCK_DGRAM, 0))
            .expect("Error creating client socket!");

        let mut dest = [0i8; 108];
        "example.sock"
            .bytes()
            .zip(dest.iter_mut())
            .for_each(|(b, ptr)| *ptr = b as i8);

        syscall!(sendto(
            sock,
            [0u8; 0].as_ptr() as _,
            0,
            0,
            &libc::sockaddr_un {
                sun_family: libc::AF_UNIX as libc::sa_family_t,
                sun_path: dest,
            } as *const _ as *const _,
            core::mem::size_of::<libc::sockaddr>() as u32
        ))
        .unwrap();
    });

    poller.recv(|fd| {
        let mut buf = [0u8; 42];
        let file = syscall!(read(fd as i32, &mut buf as *mut _ as *mut libc::c_void, 42))
            .expect("Error reading service file!");
        dbg!(file);
    });
}

mod manager {
    use crate::poller::Epoll;

    pub struct Manager<const N: usize, const M: usize> {
        poll: Epoll<N>,
    }

    impl<const N: usize, const M: usize> Manager<N, M> {
        pub fn init() -> Result<Self, &'static str> {
            Ok(Self {
                poll: Epoll::init()?,
            })
        }

        pub fn socket(&mut self, service: &str, listen_at: &str) -> Result<i32, &'static str> {
            let sock = syscall!(socket(libc::AF_UNIX, libc::SOCK_DGRAM, 0))?;

            let mut dest = [0i8; 108];
            listen_at
                .bytes()
                .zip(dest.iter_mut())
                .for_each(|(b, ptr)| *ptr = b as i8);

            syscall!(bind(
                sock,
                &libc::sockaddr_un {
                    sun_family: libc::AF_UNIX as libc::sa_family_t,
                    sun_path: dest,
                } as *const _ as *const _,
                listen_at.bytes().len() as u32 + 2
            ))?;

            self.poll.insert(sock, service)
        }

        pub fn recv(&mut self, mut f: impl FnMut(u64)) {
            while let Some(event) = self.poll.next() {
                f(event.u64);
            }
        }
    }

    // // Slab Array
    // #[derive(Debug)]
    // pub struct slabs<const N: usize, const M: usize> {
    //     inner: [([isize; N], usize); M],
    // }

    // impl<const N: usize, const M: usize> slabs<N, M> {
    //     pub fn init() -> Self {
    //         Self {
    //             inner: [([-1; N], 0usize); M],
    //         }
    //     }

    //     pub fn insert(&mut self, at: usize, elem: isize) {
    //         self.inner[at].0[self.inner[at].1] = elem;
    //         self.inner[at].1 += 1;
    //     }

    //     pub fn pop(&mut self, at: usize) -> [isize; N] {
    //         let elem = self.inner[at].0;
    //         self.inner[at].0.iter_mut().for_each(|e| *e = -1);
    //         elem
    //     }
    // }
}

mod poller {
    type RawFd = i32;

    pub struct Epoll<const N: usize>(RawFd, [libc::epoll_event; N], usize);

    //type MappedIter<'a> = iter::FilterMap<slice::Iter<'a, epoll_event>, fn(&epoll_event) -> Option<event>>;

    impl<const N: usize> Iterator for Epoll<N> {
        type Item = libc::epoll_event;

        fn next(&mut self) -> Option<Self::Item> {
            if self.2 == 0 {
                self.2 = self.wait().ok()? as usize - 1
            };
            let ret = *self.1.get(self.2)?;
            if self.2 > 1 {
                self.2 -= 1
            };
            Some(ret)
        }
    }

    impl<const N: usize> Epoll<N> {
        // const SIZE: usize = N as usize;

        pub fn init() -> Result<Self, &'static str> {
            syscall!(epoll_create1(0))
                .map(|epfd| Self(epfd, [libc::epoll_event { events: 0, u64: 0 }; N], 0))
        }

        pub fn insert(&mut self, fd: i32, service: &str) -> Result<i32, &'static str> {
            let mut name = vec![0i8; service.len() + 1];
            service
                .bytes()
                .zip(name.iter_mut())
                .for_each(|(b, ptr)| *ptr = b as i8);

            syscall!(epoll_ctl(
                self.0,
                libc::EPOLL_CTL_ADD,
                fd,
                &mut libc::epoll_event {
                    events: (libc::EPOLLIN | libc::EPOLLET) as u32,
                    u64: syscall!(open(name.as_ptr(), libc::O_RDONLY))? as u64,
                }
            ))
        }

        // pub fn remove(&mut self, fd: i32) -> Result<i32, &'static str> {
        //     syscall!(epoll_ctl(
        //         self.0,
        //         libc::EPOLL_CTL_DEL,
        //         fd,
        //         core::ptr::null_mut()
        //     ))
        // }

        fn wait(&mut self) -> Result<i32, &'static str> {
            syscall!(epoll_wait(self.0, self.1.as_mut_ptr(), N as i32, -1))
        }

        // pub fn events(&mut self) -> io::Result<MappedIter<'_>> {
        //     let count = self.wait()? as usize;
        //     Ok(self.1[0..count].iter().filter_map(|e| {
        //         if e.events & libc::EPOLLIN as u32 != 0 {
        //             Some(event::read(e.u64 as i32))
        //         } else if e.events & libc::EPOLLOUT as u32 != 0 {
        //             Some(event::write(e.u64 as i32))
        //         } else {
        //             None
        //         }
        //     }))
        // }
    }
}
