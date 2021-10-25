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

    let mut manager = manager::default();
    manager.run("dhcpcd");





    // let service = service::service::open("dhcpcd.service", |key, value| {
    //     println!("{} = {:?}", key, value);
    // })?;
    


    // let mut poller = poller::epoll::<10>::init()?;

    // poller
    //     .watch_on_socket("test.service", "example.sock")
    //     .expect("Error creating socket!");

    // poller
    //     .watch_on_timer(10000, "test.service")
    //     .expect("Error creating timer!");

    // poller.watch_on_file("foo.txt", "test.service", libc::IN_CLOSE_WRITE)
    //     .expect("Error creating watcher!");

    // let mut thread_id = 0;

    // syscall!(pthread_create(
    //     &mut thread_id as *mut libc::pthread_t,
    //     core::ptr::null(),
    //     closure,
    //     core::ptr::null_mut()
    // ))?;

    // poller.wait(|fd| {
    //     println!("{:?}", std::time::Instant::now());
    // });

    // syscall!(pthread_join(thread_id, core::ptr::null_mut()))?;

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


#[derive(Debug, Default)]
pub struct manager {
    running: Vec<String>
}

impl manager {
    pub fn run(&mut self, name: &str) -> bool {
        if self.running.contains(&name.to_owned()) {
            return true
        }

        let mut ret = false;
        if let Ok(mut service) = service::open(&(name.to_owned() + ".service")) {
            ret = true;
            while let Some((key, value)) = service.entry() {
                if key == "service.Requires" {
                    ret = value.split('\'').all(|v| self.run(v));
                }

                else if key == "service.Program" {
                    println!("{} => {:?}", name, value.split('\'').collect::<Vec<_>>());

                    let t = std::thread::spawn(|| {
                        let ret = std::process::Command::new("gnome-control-center").status().unwrap();
                        dbg!(ret);
                    });

                    t.join();


                    // self.inner.spawn(imp::Stdio::Inherit, true).map(Child::from_inner);
                }
            }

            self.running.push(name.to_owned());
        }

        ret
    }
}


pub struct service {
    fd: i32,
    buffer: String,
    wilds: [char; 5],
    drain: bool,
    label: usize,
}

#[derive(Debug)]
pub enum value<'a> {
    single(&'a str),
    array(std::str::Split<'a, char>)
}

impl service {
    pub fn open<'a>(name: &'a str) -> Result<Self, &'static str> {
        let mut cname = alloc::vec![0i8; name.len() + 1];
        name.bytes()
            .zip(cname.iter_mut())
            .for_each(|(b, ptr)| *ptr = b as i8);
        Ok(Self {
            fd: syscall!(open(cname.as_ptr(), libc::O_RDONLY))?,
            buffer: Default::default(),
            wilds: ['\r', '\t', ' ', '[', ']'],
            drain: false,
            label: 0
        })
    }

    pub fn entry<'a>(&'a mut self) -> Option<(&'a str,&'a str)> {
        let mut b: u8 = 0;
        let mut isquote = false;
        let mut ret = None;
        loop {
            if self.drain {
                self.buffer.drain(self.label..);
                self.drain = false;
            }

            if syscall!(read(self.fd as i32, &mut b as *mut _ as *mut libc::c_void, 1)) == Ok(0) {
                self.buffer.split_once('=')
                .and_then(|pair| ret.replace((pair.0, pair.1)));
                self.label = 0;
                self.drain = true;
                break;
            }

            match b as char {
                '\0' => return None,
                '\n' if !isquote && self.buffer.contains('=') => {
                    if let Some(pair) =  self.buffer.split_once('=') {
                        ret.replace((pair.0, pair.1));
                        self.drain = true;
                    }

                    break;
                },
                '[' | '\n' if !isquote && !self.buffer.contains('=') => {
                    if !self.buffer.is_empty() {
                        if !self.buffer.ends_with('.') {
                            self.buffer.push('.')
                        }
                        self.label = self.buffer.len();
                    }

                    if b as char == '[' { self.buffer.clear() }
                },
                '\'' => isquote = !isquote,
                ',' if !isquote => self.buffer.push('\''),
                _ if self.wilds.contains(&(b as char)) && !isquote => continue,
                _ => self.buffer.push(b as char),
            }
        }

        ret
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


// pub fn read<F>(fd: i32, mut f: F) -> bool where F: FnMut(&str, entry) -> bool {
//     let (mut offset, mut isquote) = (0, false);
//     let mut buffer = alloc::string::String::new();

//     let mut b: u8 = 0;
//     let wilds = ['\r', '\t', ' ', '[', ']'];

//     let mut is_error = false;

//     loop {
//         if syscall!(read(fd as i32, &mut b as *mut _ as *mut libc::c_void, 1)) == Ok(0) { 
//             b = 0;
//         }

//         match b as char {
//             '\0' | '\n' if !isquote => {
//                 if let Some((key, value)) = buffer.split_once('=') {
//                     if f(key, if value.contains('\'') {
//                         entry::multiple(value.split('\''))
//                     } else {
//                         entry::single(value)
//                     }) == false {
//                         is_error = false
//                     }
//                     buffer.drain(offset..);
//                 } else if buffer.len() > 0 {
//                     if offset != buffer.len() {
//                         buffer.drain(..offset);
//                         buffer.push('.')
//                     }
//                     offset = buffer.len()
//                 }

//                 if b == 0 { break; }
//             },

//             '\'' => isquote = !isquote,
//             ',' if !isquote => buffer.push('\''),
//             _ if wilds.contains(&(b as char)) && !isquote => continue,
//             _ => buffer.push(b as char),

//         }
//     }

//     is_error
// }

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
