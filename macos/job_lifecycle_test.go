package main

import "testing"

func TestJobManagerKeepsExactCompletedJobAfterNextTaskStarts(t *testing.T) {
	var manager jobManager
	approval, err := manager.begin("ai-approve-selected")
	if err != nil {
		t.Fatal(err)
	}
	manager.complete(approval.ID, approval.Action, 0, "完成", "approval log")

	reconcile, err := manager.begin("reconcile-index")
	if err != nil {
		t.Fatal(err)
	}
	if reconcile.ID == approval.ID {
		t.Fatal("job ids must be unique")
	}

	completed, ok := manager.snapshot(approval.ID)
	if !ok {
		t.Fatal("completed approval disappeared when the next task started")
	}
	if completed.Running || completed.Action != approval.Action || completed.Message != "完成" {
		t.Fatalf("unexpected completed state: %+v", completed)
	}
	if completed.Log != "approval log" {
		t.Fatalf("approval log was not retained: %q", completed.Log)
	}

	current, ok := manager.snapshot("")
	if !ok || !current.Running || current.ID != reconcile.ID {
		t.Fatalf("current task did not advance independently: %+v", current)
	}
}

func TestJobManagerRejectsOverlappingTask(t *testing.T) {
	var manager jobManager
	first, err := manager.begin("ai-preview-write-selected")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := manager.begin("ai-approve-selected"); err == nil {
		t.Fatal("overlapping task was accepted")
	}
	manager.complete(first.ID, first.Action, 1, "失败", "failure")
	if _, err := manager.begin("ai-approve-selected"); err != nil {
		t.Fatalf("terminal task must release lifecycle ownership: %v", err)
	}
}
