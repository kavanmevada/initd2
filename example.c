#include <sys/inotify.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/epoll.h>


// static void             /* Display information from inotify_event structure */
// displayInotifyEvent(struct inotify_event *i)
// {
//     printf("    wd =%2d; ", i->wd);
//     if (i->cookie > 0)
//         printf("cookie =%4d; ", i->cookie);

//     printf("mask = ");
//     if (i->mask & IN_ACCESS)        printf("IN_ACCESS ");
//     if (i->mask & IN_ATTRIB)        printf("IN_ATTRIB ");
//     if (i->mask & IN_CLOSE_NOWRITE) printf("IN_CLOSE_NOWRITE ");
//     if (i->mask & IN_CLOSE_WRITE)   printf("IN_CLOSE_WRITE ");
//     if (i->mask & IN_CREATE)        printf("IN_CREATE ");
//     if (i->mask & IN_DELETE)        printf("IN_DELETE ");
//     if (i->mask & IN_DELETE_SELF)   printf("IN_DELETE_SELF ");
//     if (i->mask & IN_IGNORED)       printf("IN_IGNORED ");
//     if (i->mask & IN_ISDIR)         printf("IN_ISDIR ");
//     if (i->mask & IN_MODIFY)        printf("IN_MODIFY ");
//     if (i->mask & IN_MOVE_SELF)     printf("IN_MOVE_SELF ");
//     if (i->mask & IN_MOVED_FROM)    printf("IN_MOVED_FROM ");
//     if (i->mask & IN_MOVED_TO)      printf("IN_MOVED_TO ");
//     if (i->mask & IN_OPEN)          printf("IN_OPEN ");
//     if (i->mask & IN_Q_OVERFLOW)    printf("IN_Q_OVERFLOW ");
//     if (i->mask & IN_UNMOUNT)       printf("IN_UNMOUNT ");
//     printf("\n");

//     if (i->len > 0)
//         printf("        name = %s\n", i->name);
// }

#define BUF_LEN (10 * (sizeof(struct inotify_event) + NAME_MAX + 1))
int
main(int argc, char *argv[])
{
    int inotifyFd, wd, j;
    char buf[BUF_LEN] __attribute__ ((aligned(8)));
    ssize_t numRead;
    char *p;
    struct inotify_event *event;

    if (argc < 2 || strcmp(argv[1], "--help") == 0)
        printf("%s pathname...\n", argv[0]);

    inotifyFd = inotify_init();                 /* Create inotify instance */
    if (inotifyFd == -1)
        printf("inotify_init");

    for (j = 1; j < argc; j++) {
        wd = inotify_add_watch(inotifyFd, argv[j], IN_ALL_EVENTS | IN_ONESHOT );
        if (wd == -1)
            printf("inotify_add_watch");

        printf("Watching %s using wd %d\n", argv[j], wd);
    }

	int epollfd = epoll_create1(EPOLL_CLOEXEC);
	if (epollfd < 0) {
		printf("ERR epoll_creat1\n");
		return 1;
	}

	// Add inotifyfd to epoll set
	struct epoll_event ev;
	//std::memset(&ev, 0, sizeof(ev));
	ev.events = EPOLLET | EPOLLIN;
	ev.data.fd = inotifyFd;
	if (epoll_ctl(epollfd, EPOLL_CTL_ADD, inotifyFd, &ev) < 0) {
		printf("ERR epoll_ctl\n"); //<< ::strerror_r(errno, buf, sizeof(buf));
		return 1;
	}

	int kMaxEvents = 10;
	struct epoll_event events[kMaxEvents];

	int n;
	while (1) {
		n = epoll_wait(epollfd, events, kMaxEvents, -1);
		printf("Event occured\n");
	}

    exit(0);
}