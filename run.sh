#!/bin/bash

set -e

# 默认配置
DSTACK_URL=${DSTACK_URL:-"http://localhost:19060"}
LISTEN_ADDR=${LISTEN_ADDR:-"0.0.0.0:8080"}
PORT=${PORT:-8080}

echo "=========================================="
echo "DStack Backend Health Monitor"
echo "=========================================="
echo "DStack URL: $DSTACK_URL"
echo "Listen Address: $LISTEN_ADDR"
echo "Port: $PORT"
echo "=========================================="

# 检查是否在 Docker 中运行
if [ "$1" = "docker" ]; then
    echo "Building and running with Docker..."
    docker build -t dstack-backend:latest .
    docker run -d \
        --name dstack-backend \
        -p ${PORT}:8080 \
        -e DSTACK_URL="${DSTACK_URL}" \
        -e LISTEN_ADDR="0.0.0.0:8080" \
        --add-host host.docker.internal:host-gateway \
        dstack-backend:latest

    echo ""
    echo "Container started successfully!"
    echo "View logs: docker logs -f dstack-backend"
    echo "Test endpoint: curl http://localhost:${PORT}/health"

elif [ "$1" = "docker-compose" ]; then
    echo "Running with docker-compose..."
    docker-compose up -d --build

    echo ""
    echo "Services started successfully!"
    echo "View logs: docker-compose logs -f"
    echo "Test endpoint: curl http://localhost:8080/health"

else
    echo "Running locally with cargo..."
    export DSTACK_URL
    export LISTEN_ADDR
    cargo run --release
fi
