# mududb C binding — native build and golden-corpus conformance tests.
#
#   make test      build and run the golden-corpus driver (native clang)
#   make test-c    compile every public header standalone as C99
#   make test-cpp  compile every public header and the runtime as C++17
#   make check     all of the above
#   make clean
#
# CC/CXX default to clang/clang++; override to use another toolchain.

CC ?= clang
CXX ?= clang++
CFLAGS ?= -Wall -Wextra -Werror -std=c99 -g
CXXFLAGS ?= -Wall -Wextra -Werror -std=c++17 -g
CPPFLAGS += -I.

BUILD := build
DRIVER := $(BUILD)/corpus_driver
HEADERS := $(wildcard mududb/codec/*.h mududb/types/*.h)
RUNTIME := mududb/codec/mpack.c mududb/codec/mpack_alloc.c mududb/codec/record_bridge.c

.PHONY: all test test-c test-cpp check clean

all: $(DRIVER)

$(BUILD):
	mkdir -p $(BUILD)

$(DRIVER): tests/corpus_driver.c tests/mini_json.c tests/mini_json.h $(RUNTIME) $(HEADERS) | $(BUILD)
	$(CC) $(CPPFLAGS) $(CFLAGS) -o $@ tests/corpus_driver.c tests/mini_json.c $(RUNTIME)

test: $(DRIVER)
	./$(DRIVER)

# Every public header is self-contained (include it alone and it compiles).
test-c:
	@for f in $(HEADERS); do \
		$(CC) $(CPPFLAGS) $(CFLAGS) -fsyntax-only $$f || exit 1; \
	done
	@echo "C99 header compile check OK ($(words $(HEADERS)) headers)"

# The binding serves C and C++ users: every header and the runtime must
# also compile as C++17 (extern "C" + MP_INLINE linkage).
test-cpp:
	@for f in $(HEADERS); do \
		$(CXX) -x c++ $(CPPFLAGS) $(CXXFLAGS) -fsyntax-only $$f || exit 1; \
	done
	@for f in $(RUNTIME); do \
		$(CXX) -x c++ $(CPPFLAGS) $(CXXFLAGS) -fsyntax-only $$f || exit 1; \
	done
	@echo "C++17 compile check OK"

check: test-c test-cpp test

clean:
	rm -rf $(BUILD)
