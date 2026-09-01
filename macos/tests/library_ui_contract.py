#!/usr/bin/env python3
from pathlib import Path
s=(Path(__file__).resolve().parents[1]/"web"/"index.html").read_text()
checks={
"movie hides redundant level filter":"q('#typeFilter').classList.toggle('hide',space!=='tv')",
"tv level options are populated":'<option value="all">全部层级</option><option value="tvshow">节目</option><option value="episode">单集</option>',
"task minimum height":"minmax(190px,var(--task-h))",
"preview immediate busy":"busy(button,true,'正在准备试写…')",
"categorized roots":"library_roots:{movies:",
"single rescan":"id=\"rescanCurrent\"",
"advanced AI":"id=\"aiTemperature\"",
"backend issue endpoint":"/api/inspector/issues",
}
missing=[name for name,value in checks.items() if value not in s]
assert not missing, missing
assert s.index("busy(button,true,'正在准备试写…')") < s.index("const result=await serverPreflight(engine)", s.index("busy(button,true,'正在准备试写…')"))
print("OK UI regression contract")
assert "id=\"rescanAll\"" not in s, "rescanAll must be removed"
print("OK legacy UI contract (rescanAll removed)")
