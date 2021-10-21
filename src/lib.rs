//#![no_std]
#![feature(fn_traits)]
#![feature(unboxed_closures)]
extern crate alloc;

#[macro_export]
macro_rules! syscall {
    ($fn:ident $args:tt) => {{
        use libc::{strlen, strerror};
        use core::{str::from_utf8, slice::from_raw_parts};
        let res = unsafe { libc::$fn $args };
        if res as libc::c_int == -1 {
            let ptr = unsafe { strerror(*libc::__errno_location())};
            Err(from_utf8(unsafe { from_raw_parts(ptr as *const u8, strlen(ptr) as usize) }).unwrap_or_default())
        } else {
            Ok(res)
        }
    }};
}

#[test]
fn test_run() -> Result<(), &'static str> {
    let mut poller = poller::epoll::<10>::init()?;

    poller
        .watch_on_socket("test.service", "example.sock")
        .expect("Error creating socket!");

    poller
        .watch_on_timer(10000, "test.service")
        .expect("Error creating timer!");

    poller.watch_on_file("foo.txt", "test.service", libc::IN_CLOSE_WRITE)
        .expect("Error creating watcher!");

    let mut thread_id = 0;

    syscall!(pthread_create(
        &mut thread_id as *mut libc::pthread_t,
        core::ptr::null(),
        closure,
        core::ptr::null_mut()
    ))?;

    poller.wait(|fd| {
        println!("{:?}", std::time::Instant::now());
        // service::read(fd as i32, |key, mut value| {
        //     println!(
        //         "{} = {:?} {:?} {:?}",
        //         key,
        //         value.next(),
        //         value.next(),
        //         value.next()
        //     );
        // })
        // .unwrap();
    });

    syscall!(pthread_join(thread_id, core::ptr::null_mut()))?;

    Ok(())
}

#[no_mangle]
extern "C" fn closure(_: *mut libc::c_void) -> *mut libc::c_void {
    syscall!(sleep(5));

    net::socket::connect("example.sock")
        .expect("Error creating client socket!")
        .wake();

    0 as *mut libc::c_void
}

mod service {
    pub struct entries<'a>(Option<&'a str>);

    impl<'a> core::fmt::Display for entries<'a> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_list().entries(["", "", ""]).finish()
        }
    }

    impl<'a> Iterator for entries<'a> {
        type Item = &'a str;
        fn next(&mut self) -> Option<Self::Item> {
            if let Some((next, remain)) = self.0.and_then(|s| s.split_once('\'')) {
                self.0 = Some(remain);
                return Some(next);
            } else if let Some(next) = self.0.take() {
                return Some(next);
            }

            None
        }
    }

    pub fn read<F>(fd: i32, mut f: F) -> Result<(), &'static str>
    where
        F: FnMut(&str, entries),
    {
        let wildcards = ['\n', '\r', '\t', ' ', '[', ']'];

        let (mut offset, mut quote) = (0, 0);
        let mut buffer = alloc::string::String::new();

        let mut b: u8 = 0;
        while syscall!(read(fd as i32, &mut b as *mut _ as *mut libc::c_void, 1))?.is_positive() {
            let mut c = b as char;

            if c as char == '\'' {
                quote += 1;
                continue;
            }

            if quote % 2 != 0 || (quote % 2 == 0 && (c == ',' || !wildcards.contains(&c))) {
                if quote % 2 == 0 && c == ',' {
                    c = '\''
                }
                buffer.push(c);
            }

            if quote % 2 != 0 || c != '\n' {
                continue;
            }

            if let Some((key, value)) = buffer.split_once('=') {
                f(key, entries(Some(value)));
                buffer.drain(offset..);
            } else if buffer.len() > 0 {
                if offset != buffer.len() {
                    buffer.drain(..offset);
                    buffer.push('.')
                }
                offset = buffer.len()
            }
        }

        Ok(())
    }
}

mod net {
    pub struct socket(i32);

    impl socket {
        pub fn init() -> Result<Self, &'static str> {
            syscall!(socket(libc::AF_UNIX, libc::SOCK_DGRAM, 0)).map(|fd| Self(fd))
        }
        pub fn as_raw_fd(&self) -> i32 {
            self.0
        }
        pub fn bind(path: &str) -> Result<Self, &'static str> {
            let self_ = Self::init()?;

            let mut dest = [0i8; 108];
            path.bytes()
                .zip(dest.iter_mut())
                .for_each(|(b, ptr)| *ptr = b as i8);

            syscall!(remove(
                path.bytes()
                    .map(|b| b as i8)
                    .chain(core::iter::once(0))
                    .collect::<Box<_>>()
                    .as_ptr()
            ));

            syscall!(bind(
                self_.0,
                &libc::sockaddr_un {
                    sun_family: libc::AF_UNIX as libc::sa_family_t,
                    sun_path: dest,
                } as *const _ as *const _,
                path.bytes().len() as u32 + 2
            ))?;

            Ok(self_)
        }

        pub fn connect(path: &str) -> Result<Self, &'static str> {
            let self_ = Self::init()?;

            let mut dest = [0i8; 108];
            path.bytes()
                .zip(dest.iter_mut())
                .for_each(|(b, ptr)| *ptr = b as i8);

            syscall!(connect(
                self_.as_raw_fd(),
                &libc::sockaddr_un {
                    sun_family: libc::AF_UNIX as libc::sa_family_t,
                    sun_path: dest,
                } as *const _ as *const _,
                path.bytes().len() as u32 + 2
            ))?;

            Ok(self_)
        }

        pub fn wake(&self) {
            syscall!(send(
                self.as_raw_fd(),
                &mut 0 as *mut _ as *mut libc::c_void,
                0,
                0
            ))
            .unwrap();
        }
    }
}

mod poller {
    use std::mem::MaybeUninit;

    use crate::net::socket;

    pub struct epoll<const N: usize> {
        epfd: i32,
        events: [libc::epoll_event; N],
        cursor: usize,
        pub notifyfd: i32,
        pub nevents: [(u64, u32); N] 
    }

    impl<const N: usize> Iterator for epoll<N> {
        type Item = libc::epoll_event;

        fn next(&mut self) -> Option<Self::Item> {
            if self.cursor == 0 {
                self.cursor = syscall!(epoll_wait(
                    self.epfd,
                    self.events.as_mut_ptr(),
                    N as i32,
                    -1
                )).ok()? as usize - 1
            };
            let ret = *self.events.get(self.cursor)?;
            if self.cursor > 1 {
                self.cursor -= 1
            };
            Some(ret)
        }
    }

    impl<const N: usize> epoll<N> {
        pub fn init() -> Result<Self, &'static str> {
            let epfd = syscall!(epoll_create1(0))?;
            let notifyfd = syscall!(inotify_init1(libc::IN_CLOEXEC))?;

            syscall!(epoll_ctl(epfd, libc::EPOLL_CTL_ADD, notifyfd, &mut libc::epoll_event {
                events: (libc::EPOLLIN | libc::EPOLLET) as u32,
                u64: notifyfd as u64,
            }))?;

            Ok(Self {
                epfd,
                events: [libc::epoll_event { events: 0, u64: 0 }; N],
                cursor: 0,
                notifyfd,
                nevents: [(0, 0); N],
            })
        }

        fn watch_on_fd<T: Into<i32>>(&self, fd: T, name: &str) -> Result<i32, &'static str> {
            let mut cname = alloc::vec![0i8; name.len() + 1];
            name.bytes()
                .zip(cname.iter_mut())
                .for_each(|(b, ptr)| *ptr = b as i8);

            syscall!(epoll_ctl(
                self.epfd,
                libc::EPOLL_CTL_ADD,
                T::into(fd),
                &mut libc::epoll_event {
                    events: (libc::EPOLLIN | libc::EPOLLET) as u32,
                    u64: syscall!(open(cname.as_ptr(), libc::O_RDONLY))? as u64,
                }
            ))
        }

        pub fn watch_on_socket(&mut self, service: &str, path: &str) -> Result<i32, &'static str> {
            let sock = socket::bind(path)?;
            self.watch_on_fd(sock.as_raw_fd(), service)
        }


        pub fn watch_on_timer(&mut self, msec: i64, name: &str) -> Result<i32, &'static str> {
            let tfd = syscall!(timerfd_create(libc::CLOCK_MONOTONIC, 0))?;

            syscall!(timerfd_settime(
                tfd,
                0,
                &libc::itimerspec {
                it_interval: libc::timespec {
                    tv_sec: 0,
                    tv_nsec: 0
                },
                it_value: libc::timespec {
                    tv_sec: msec / 1000,
                    tv_nsec: (msec % 1000) * 1000000
                }
            },
                std::ptr::null_mut()
            ))?;

            self.watch_on_fd(tfd, name)
        }


        pub fn watch_on_file(&mut self, path: &str, name: &str, mask: u32) -> Result<(), &'static str> {
            let mut cname = alloc::vec![0i8; name.len() + 1];
            name.bytes()
                .zip(cname.iter_mut())
                .for_each(|(b, ptr)| *ptr = b as i8);
            let sfd = syscall!(open(cname.as_ptr(), libc::O_RDONLY))? as u64;

            syscall!(inotify_add_watch(
                self.notifyfd,
                path.bytes()
                    .map(|b| b as i8)
                    .chain(core::iter::once(0))
                    .collect::<Box<_>>()
                    .as_ptr(),
                libc::IN_ALL_EVENTS
            )).map(|fd| self.nevents[fd as usize] = (sfd, mask))
        }

        pub fn wait(&mut self, mut f: impl FnMut(u64)) {
            while let Some(event) = self.next() {
                if event.u64 == self.notifyfd as u64 {
                    let mut ev: MaybeUninit<libc::inotify_event> = core::mem::MaybeUninit::uninit();
                    syscall!(read(event.u64 as i32, ev.as_mut_ptr() as *mut _, core::mem::size_of::<libc::inotify_event>()));
                    let ev = unsafe { ev.assume_init() };

                    if let Some(&(fd, mask)) = self.nevents.get(ev.wd as usize) {
                        //println!("{:016b} {:016b} {:016b}", mask, ev.mask, mask & ev.mask);
                        if (mask & ev.mask) != 0 {
                            f(fd)
                        }
                    }
                } else {
                    f(event.u64);
                }
            }
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
