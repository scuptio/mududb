// Package adapter loads the separately distributed native adapter runtime.
package adapter

/*
#cgo linux LDFLAGS: -ldl
#include "native.h"
*/
import "C"
import (
	"errors"
	"os"
	"path/filepath"
	"sync"
	"unsafe"
)

// Client owns a loaded ABI v1 runtime. Close it after closing all DB sessions.
type Client struct {
	mu     sync.Mutex
	native *C.mudu_native
}

// Open loads a trusted runtime; an empty path uses MUDU_ADAPTER_LIBRARY.
func Open(path string) (*Client, error) {
	if path == "" {
		path = os.Getenv("MUDU_ADAPTER_LIBRARY")
	}
	if path == "" {
		return nil, errors.New("set MUDU_ADAPTER_LIBRARY to the extracted runtime library")
	}
	path, err := filepath.Abs(path)
	if err != nil {
		return nil, err
	}
	p := C.CString(path)
	defer C.free(unsafe.Pointer(p))
	n := C.mudu_native_load(p)
	if n == nil {
		return nil, errors.New("cannot load adapter: check path, architecture, dependencies and ABI v1")
	}
	return &Client{native: n}, nil
}

// Call sends an MSSP frame and copies the result before freeing native memory.
func (c *Client) Call(operation string, frame []byte) ([]byte, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.native == nil {
		return nil, errors.New("adapter is closed")
	}
	op := C.CString(operation)
	defer C.free(unsafe.Pointer(op))
	var input *C.uint8_t
	if len(frame) != 0 {
		input = (*C.uint8_t)(unsafe.Pointer(&frame[0]))
	}
	var out C.mudu_native_buffer
	status := C.mudu_native_call(c.native, op, input, C.size_t(len(frame)), &out)
	defer C.mudu_native_release(c.native, out)
	if uint64(out.len) > uint64(1<<31-1) {
		return nil, errors.New("adapter response exceeds Go bridge limit")
	}
	result := C.GoBytes(unsafe.Pointer(out.data), C.int(out.len))
	if status != 0 {
		return nil, errors.New(string(result))
	}
	return result, nil
}

// Close unloads this client's runtime handle. Do not retain native allocations.
func (c *Client) Close() {
	c.mu.Lock()
	defer c.mu.Unlock()
	C.mudu_native_unload(c.native)
	c.native = nil
}
