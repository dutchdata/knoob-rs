CC      = gcc
CFLAGS  = -std=gnu11 -Wall -Wextra -Wpedantic \
	  -O2 -g \
	  -D_GNU_SOURCE

# shared lib flags
SO_FLAGS = -shared -fPIC

# what we build
LIB     = libcapture.so
TEST    = capture_test

SRCS    = capture.c chanhop.c
TEST_SRC = capture_test.c

.PHONY: all clean debug

all: $(LIB) $(TEST)

debug: CFLAGS += -DDEBUG
debug: $(LIB) $(TEST)

$(LIB): $(SRCS) capture.h
	$(CC) $(CFLAGS) $(SO_FLAGS) -o $@ $(SRCS)

$(TEST): $(TEST_SRC) $(LIB) capture.h
	$(CC) $(CFLAGS) -o $@ $(TEST_SRC) -L. -lcapture -Wl,-rpath,. -lpthread

clean:
	rm -f $(LIB) $(TEST)
