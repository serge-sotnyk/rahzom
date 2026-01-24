#!/bin/bash
# Wrapper script for docker exec with MSYS path conversion disabled
# Usage: ./docker-exec.sh <command> [args...]
# Example: ./docker-exec.sh tmux send-keys -t main 'j'

MSYS_NO_PATHCONV=1 docker exec rahzom-test "$@"
