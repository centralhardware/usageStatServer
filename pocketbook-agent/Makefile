# make test        host unit tests
# make SDK=<path>  cross-compile usage-agent.app with the PocketBook SDK (SDK-B288 dir)
SDK ?=
CFLAGS += -std=gnu99 -O2 -Wall -Wextra -Werror

ifneq ($(SDK),)
CC := $(SDK)/usr/bin/arm-obreey-linux-gnueabi-gcc
SYSROOT := $(SDK)/usr/arm-obreey-linux-gnueabi/sysroot
CFLAGS += -I$(SYSROOT)/usr/local/include
LDFLAGS += -L$(SYSROOT)/usr/local/lib
endif

usage-agent.app: src/main.c src/core.c src/core.h
	$(CC) $(CFLAGS) -o $@ src/main.c src/core.c $(LDFLAGS) -linkview -lcurl

test: tests/test_core.c src/core.c src/core.h
	cc -std=gnu99 -Wall -Wextra -Werror -o test_core tests/test_core.c src/core.c && ./test_core
	cc -std=gnu99 -Wall -Wextra -Werror -DNO_INKVIEW -fsyntax-only src/main.c

clean:
	rm -f usage-agent.app test_core

.PHONY: test clean
