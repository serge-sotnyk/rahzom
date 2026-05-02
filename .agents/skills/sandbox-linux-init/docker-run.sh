#!/bin/bash
# Initialize rahzom-test container with proper volume mount
# Usage: ./docker-run.sh

MSYS_NO_PATHCONV=1 docker run -d --name rahzom-test \
  -v "$(pwd):/app/rahzom" \
  rahzom-test-image
