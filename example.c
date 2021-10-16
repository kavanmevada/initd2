/*
 *  File:   inotify-example.c
 *  Author: Kavan Mevada
 *
 *  A simple tester of inotify in the Linux kernel.
 *
 *  This program is released in the Public Domain.
 *
 *  Compile with:
 *    $> gcc -o inotify-example inotify-example.c
 *
 *  Run as:
 *    $> ./inotify-example /path/to/monitor /another/path/to/monitor ...
 */
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <stdlib.h>
#include <sys/epoll.h>
#include <errno.h>
#include <sys/signalfd.h>
#include <linux/inotify.h>

/*Structure to keep track of monitored directories */
typedef struct
{
	/*Path of the directory */
	char *path;
	/*inotify watch descriptor */
	int wd;
}
monitored_t;

/*Size of buffer to use when reading inotify events */
#define INOTIFY_BUFFER_SIZE 8192

/*Setup inotify notifications (IN) mask. All these defined in inotify.h. */
static int IN_ALL_EVENTS =
	(IN_ACCESS | /*File accessed */
		IN_ATTRIB | /*File attributes changed */
		IN_OPEN | /*File was opened */
		IN_CLOSE_WRITE | /*Writtable File closed */
		IN_CLOSE_NOWRITE | /*Unwrittable File closed */
		IN_CREATE | /*File created in directory */
		IN_DELETE | /*File deleted in directory */
		IN_DELETE_SELF | /*Directory deleted */
		IN_MODIFY | /*File modified */
		IN_MOVE_SELF | /*Directory moved */
		IN_MOVED_FROM | /*File moved away from the directory */
		IN_MOVED_TO); /*File moved into the directory */

/*Array of directories being monitored */
static monitored_t * monitors;
static int n_monitors;

int
main(int argc, const char **argv)
{
	int signal_fd;
	int inotify_fd, pollfd;

	/*Input arguments... */
	if (argc < 2)
	{
		fprintf(stderr, "Usage: %s directory1[directory2 ...]\n", argv[0]);
		exit(EXIT_FAILURE);
	}

	int i;
	/*Create new inotify device */
	if ((inotify_fd = inotify_init()) < 0)
	{
		fprintf(stderr,
			"Couldn't setup new inotify device: '%s'\n",
			strerror(errno));
		return -1;
	}

	/*Allocate array of monitor setups */
	n_monitors = argc - 1;
	monitors = malloc(n_monitors* sizeof(monitored_t));

	/*Loop all input directories, setting up watches */
	for (i = 0; i < n_monitors; ++i)
	{
		monitors[i].path = strdup(argv[i + 1]);
		if ((monitors[i].wd = inotify_add_watch(inotify_fd,
				monitors[i].path,
				IN_ALL_EVENTS)) < 0)
		{
			fprintf(stderr,
				"Couldn't add monitor in directory '%s': '%s'\n",
				monitors[i].path,
				strerror(errno));
			exit(EXIT_FAILURE);
		}
		printf("Started monitoring directory '%s'...\n",
			monitors[i].path);
	}

	/*Initialize inotify FD and the watch descriptors */
	if (inotify_fd < 0)
	{
		fprintf(stderr, "Couldn't initialize inotify\n");
		exit(EXIT_FAILURE);
	}

	pollfd = epoll_create1(0);

	struct epoll_event ev;

	ev.events = EPOLLET | EPOLLIN;
	ev.data.fd = inotify_fd;
	if (epoll_ctl(pollfd, EPOLL_CTL_ADD, inotify_fd, &ev) < 0)
	{
		printf("ERR epoll_ctl\n");	//<< ::strerror_r(errno, buf, sizeof(buf));
		return 1;
	}

	struct epoll_event events[10];

	int n;
	do {
		n = epoll_wait(pollfd, events, 10, -1);
		for (int i = 0; i < n; ++i)
		{
			int ifd = events[i].data.fd;

			char buffer[INOTIFY_BUFFER_SIZE];
			size_t length;

			if ((length = read(ifd,
					buffer,
					INOTIFY_BUFFER_SIZE)) > 0)
			{
				struct inotify_event * event;
				event = (struct inotify_event *) buffer;

				for (i = 0; i < n_monitors; ++i)
				{ 		/*If watch descriptors match, we found our directory */
					if (monitors[i].wd == event->wd)
					{
						if (event->len > 0)
							printf("Received event in '%s/%s': ",
								monitors[i].path,
								event->name);
						else
							printf("Received event in '%s': ",
								monitors[i].path);

						if (event->mask &IN_ACCESS)
							printf("\tIN_ACCESS\n");
						if (event->mask &IN_ATTRIB)
							printf("\tIN_ATTRIB\n");
						if (event->mask &IN_OPEN)
							printf("\tIN_OPEN\n");
						if (event->mask &IN_CLOSE_WRITE)
							printf("\tIN_CLOSE_WRITE\n");
						if (event->mask &IN_CLOSE_NOWRITE)
							printf("\tIN_CLOSE_NOWRITE\n");
						if (event->mask &IN_CREATE)
							printf("\tIN_CREATE\n");
						if (event->mask &IN_DELETE)
							printf("\tIN_DELETE\n");
						if (event->mask &IN_DELETE_SELF)
							printf("\tIN_DELETE_SELF\n");
						if (event->mask &IN_MODIFY)
							printf("\tIN_MODIFY\n");
						if (event->mask &IN_MOVE_SELF)
							printf("\tIN_MOVE_SELF\n");
						if (event->mask &IN_MOVED_FROM)
							printf("\tIN_MOVED_FROM (cookie: %d)\n",
								event->cookie);
						if (event->mask &IN_MOVED_TO)
							printf("\tIN_MOVED_TO (cookie: %d)\n",
								event->cookie);
					}
				}
			}
		}
	} while (/*n < 0 && errno == EINTR */ 1);

	/*Clean exit */
	for (i = 0; i < n_monitors; ++i)
	{
		free(monitors[i].path);
		inotify_rm_watch(inotify_fd, monitors[i].wd);
	}
	free(monitors);
	close(inotify_fd);

	printf("Exiting inotify example...\n");

	return EXIT_SUCCESS;
}