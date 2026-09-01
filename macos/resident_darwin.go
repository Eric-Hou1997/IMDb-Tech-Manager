//go:build darwin

package main

import (
	"bufio"
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os/exec"
	"sync"
	"time"
)

// residentInspectorJSON routes a CLI-style inspector call through the resident
// engine process. It returns handled=false when the call shape is not served
// (caller falls back to a one-shot process) or when the resident process
// cannot be started/kept alive.
func residentInspectorJSON(args ...string) (json.RawMessage, error, bool) {
	if len(args) == 0 {
		return nil, nil, false
	}
	var (
		cmd     string
		payload interface{}
		timeout = 90 * time.Second
	)
	switch args[0] {
	case "--library-index":
		cmd = "library-index"
		timeout = 10 * time.Minute
	case "--library-changes-since":
		if len(args) < 2 {
			return nil, nil, false
		}
		cmd = "library-changes"
		payload = map[string]interface{}{"since": args[1]}
	case "--inspector-path":
		if len(args) < 2 {
			return nil, nil, false
		}
		cmd = "inspector-detail"
		payload = map[string]interface{}{"path": args[1]}
	case "--inspector-edit-json", "--inspector-undo-json", "--inspector-issues-json", "--inspector-reload-json", "--scope-preflight-json", "--status-override-json":
		if len(args) < 2 {
			return nil, nil, false
		}
		var body interface{}
		if err := json.Unmarshal([]byte(args[1]), &body); err != nil {
			return nil, nil, false
		}
		timeout = 5 * time.Minute
		switch args[0] {
		case "--inspector-edit-json":
			cmd = "inspector-edit"
		case "--inspector-undo-json":
			cmd = "inspector-undo"
		case "--inspector-issues-json":
			cmd = "inspector-issues"
		case "--inspector-reload-json":
			cmd = "reload"
		case "--scope-preflight-json":
			cmd = "scope-preflight"
		default:
			cmd = "status-override"
		}
		payload = map[string]interface{}{"payload": body}
	default:
		return nil, nil, false
	}
	res, err := resident.call(cmd, payload, timeout)
	if err != nil {
		return nil, err, false // resident path failed: fall back to one-shot
	}
	if !res.OK {
		message := res.Error
		if message == "" {
			message = "Inspector Engine 返回错误"
		}
		payload, _ := json.Marshal(map[string]string{"error": message, "kind": res.Kind})
		return payload, errors.New(message), true
	}
	raw, err := json.Marshal(res.Result)
	if err != nil {
		return nil, err, false
	}
	return raw, nil, true
}

type residentResponse struct {
	ID     int64           `json:"id"`
	OK     bool            `json:"ok"`
	Result json.RawMessage `json:"result"`
	Error  string          `json:"error"`
	Kind   string          `json:"kind"`
}

type residentProc struct {
	mu      sync.Mutex
	cmd     *exec.Cmd
	stdin   io.WriteCloser
	pending map[int64]chan *residentResponse
	nextID  int64
	alive   bool
}

var resident residentProc

func (r *residentProc) startLocked() error {
	py := python3()
	if py == "" {
		return errors.New("未找到 python3")
	}
	cmd := exec.Command(py, enginePath(), "--serve")
	stdin, err := cmd.StdinPipe()
	if err != nil {
		return err
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return err
	}
	// Engine chatter goes to stderr in serve mode; keep a bounded tail for
	// diagnostics instead of letting it grow without limit.
	stderrPipe, _ := cmd.StderrPipe()
	if stderrPipe != nil {
		go func() {
			_, _ = io.Copy(&boundedBuffer{limit: 32 * 1024}, stderrPipe)
		}()
	}
	if err := cmd.Start(); err != nil {
		return err
	}
	r.cmd = cmd
	r.stdin = stdin
	r.pending = map[int64]chan *residentResponse{}
	r.alive = true
	go r.readLoop(stdout)
	return nil
}

func (r *residentProc) readLoop(stdout io.Reader) {
	scanner := bufio.NewScanner(stdout)
	scanner.Buffer(make([]byte, 0, 64*1024), 64*1024*1024)
	for scanner.Scan() {
		line := bytes.TrimSpace(scanner.Bytes())
		if len(line) == 0 {
			continue
		}
		var resp residentResponse
		if err := json.Unmarshal(line, &resp); err != nil {
			continue
		}
		r.mu.Lock()
		ch, ok := r.pending[resp.ID]
		if ok {
			delete(r.pending, resp.ID)
		}
		r.mu.Unlock()
		if ok {
			ch <- &resp
		}
	}
	r.mu.Lock()
	r.alive = false
	pending := r.pending
	r.pending = map[int64]chan *residentResponse{}
	r.mu.Unlock()
	for _, ch := range pending {
		close(ch)
	}
}

func (r *residentProc) killLocked() {
	r.alive = false
	if r.cmd != nil && r.cmd.Process != nil {
		_ = r.cmd.Process.Kill()
	}
	if r.stdin != nil {
		_ = r.stdin.Close()
	}
	r.cmd = nil
	r.stdin = nil
}

// call sends one JSON-RPC request and waits for the matching response.
func (r *residentProc) call(command string, payload interface{}, timeout time.Duration) (*residentResponse, error) {
	r.mu.Lock()
	if !r.alive || r.cmd == nil {
		r.killLocked()
		if err := r.startLocked(); err != nil {
			r.mu.Unlock()
			return nil, err
		}
	}
	r.nextID++
	id := r.nextID
	ch := make(chan *residentResponse, 1)
	r.pending[id] = ch
	req := map[string]interface{}{"id": id, "cmd": command}
	if m, ok := payload.(map[string]interface{}); ok {
		for k, v := range m {
			req[k] = v
		}
	}
	body, err := json.Marshal(req)
	if err != nil {
		delete(r.pending, id)
		r.mu.Unlock()
		return nil, err
	}
	if _, err := r.stdin.Write(append(body, '\n')); err != nil {
		delete(r.pending, id)
		r.killLocked()
		r.mu.Unlock()
		return nil, fmt.Errorf("常驻引擎不可达：%w", err)
	}
	r.mu.Unlock()

	timer := time.NewTimer(timeout)
	defer timer.Stop()
	select {
	case resp, ok := <-ch:
		if !ok {
			return nil, errors.New("常驻引擎已退出")
		}
		return resp, nil
	case <-timer.C:
		r.mu.Lock()
		delete(r.pending, id)
		r.killLocked()
		r.mu.Unlock()
		return nil, fmt.Errorf("常驻引擎响应超时（%s）", timeout)
	}
}

// invalidateResidentCache asks the resident engine to drop index summaries.
func invalidateResidentCache(paths []string) {
	payload := map[string]interface{}{}
	if len(paths) > 0 {
		payload["paths"] = paths
	}
	_, _ = resident.call("invalidate", payload, 30*time.Second)
}

// boundedBuffer keeps only the last limit bytes written to it.
type boundedBuffer struct {
	mu    sync.Mutex
	limit int
	buf   []byte
}

func (b *boundedBuffer) Write(p []byte) (int, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	b.buf = append(b.buf, p...)
	if len(b.buf) > b.limit {
		b.buf = b.buf[len(b.buf)-b.limit:]
	}
	return len(p), nil
}

// residentShutdown kills the resident engine child so no Python process
// outlives the GUI helper, on any exit path.
func residentShutdown() {
	resident.mu.Lock()
	defer resident.mu.Unlock()
	resident.killLocked()
}
