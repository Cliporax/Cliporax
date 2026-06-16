# Dev Log Skill

Use this when debugging runtime behavior, IPC calls, locks, focus/window issues, clipboard monitoring, or frontend/backend event chains in development mode.

## Real Log Location

Logs are date-rotated:

- Linux: `~/.local/share/com.cliporax.app/logs/dev-YYYY-MM-DD.log`
- macOS: `~/Library/Application Support/com.cliporax.app/logs/dev-YYYY-MM-DD.log`
- Windows: `%APPDATA%\com.cliporax.app\logs\dev-YYYY-MM-DD.log`

The Rust source of truth is `src-tauri/src/dev_log.rs`.

## Fast Commands

Linux:

```bash
tail -n 200 ~/.local/share/com.cliporax.app/logs/dev-$(date +%Y-%m-%d).log
tail -f ~/.local/share/com.cliporax.app/logs/dev-$(date +%Y-%m-%d).log
grep "ERROR" ~/.local/share/com.cliporax.app/logs/dev-*.log
grep "LOCK_WAIT\|LOCK_HELD_LONG" ~/.local/share/com.cliporax.app/logs/dev-*.log
grep "TRACE:<trace_id>" ~/.local/share/com.cliporax.app/logs/dev-*.log
```

Windows PowerShell:

```powershell
$today = Get-Date -Format "yyyy-MM-dd"
Get-Content "$env:APPDATA\com.cliporax.app\logs\dev-$today.log" -Tail 200
Select-String "ERROR" "$env:APPDATA\com.cliporax.app\logs\dev-*.log"
```

## Debugging Pattern

1. Find the relevant `TRACE:<id>` or component.
2. Follow the trace across frontend/backend.
3. Check IPC duration, lock wait/hold events, and final error.
4. Use logs to narrow code reads instead of scanning whole subsystems.
