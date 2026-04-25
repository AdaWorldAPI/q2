#!/bin/bash
# Chunk Bash output via tee -a to session log.
# Ensures long outputs are captured for review.
LOG_DIR="$CLAUDE_PROJECT_DIR/.claude/logs"
mkdir -p "$LOG_DIR"
LOG_FILE="$LOG_DIR/session-$(date +%Y%m%d).log"
echo "--- $(date -Iseconds) ---" >> "$LOG_FILE"
# The hook runs after tool use; input contains the tool result.
# We just log a marker — actual tee -a is done inline in Bash commands.
