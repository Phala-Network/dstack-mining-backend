#!/bin/bash

set -e

echo "========================================"
echo "DStack Backend + Whitelist Test"
echo "========================================"

# 单元测试
echo ""
echo "1. 单元测试..."
cargo test --test unit_test --quiet && echo "✓ 单元测试通过" || echo "✗ 单元测试失败"

# 构建
echo ""
echo "2. 构建..."
cargo build --quiet 2>&1 && echo "✓ 构建成功" || echo "✗ 构建失败"

# 清理测试数据
rm -rf ./test_data ./test_whitelist.json
mkdir -p ./test_data

# 启动whitelist
echo ""
echo "3. 启动Whitelist服务 (8082)..."
WHITELIST_FILE=./test_whitelist.json \
LISTEN_ADDR=127.0.0.1:18082 \
cargo run --quiet --bin whitelist-service &
WL_PID=$!
sleep 3

# 启动backend
echo ""
echo "4. 启动Backend (8080)..."
DATA_DIR=./test_data \
LISTEN_ADDR=127.0.0.1:18080 \
DSTACK_URL=http://localhost:14520 \
cargo run --quiet --bin dstack-backend &
BACKEND_PID=$!
sleep 3

# 测试backend health
echo ""
echo "5. 测试Backend /health..."
HEALTH=$(curl -s http://127.0.0.1:18080/health 2>&1)
if echo "$HEALTH" | jq -e '.pubkeys[0]' > /dev/null 2>&1; then
    PUBKEY=$(echo "$HEALTH" | jq -r '.pubkeys[0]')
    echo "✓ Backend正常，pubkey: $PUBKEY"
else
    echo "✗ Backend异常"
fi

# 添加pubkey到whitelist
echo ""
echo "6. 添加pubkey到whitelist..."
cat > ./test_whitelist.json << EOF
{
  "pubkeys": ["$PUBKEY"]
}
EOF
kill $WL_PID
sleep 1
WHITELIST_FILE=./test_whitelist.json \
LISTEN_ADDR=127.0.0.1:18082 \
cargo run --quiet --bin whitelist-service &
WL_PID=$!
sleep 2

# 测试whitelist
echo ""
echo "7. 测试Whitelist验证 (8082)..."
WL_RESP=$(curl -s "http://127.0.0.1:18082/api/whitelist?pubkey=$PUBKEY")
if echo "$WL_RESP" | jq -e '.is_whitelisted == true' > /dev/null 2>&1; then
    echo "✓ Whitelist验证通过"
else
    echo "✗ Whitelist验证失败"
fi

# 清理
echo ""
echo "8. 清理..."
kill $WL_PID $BACKEND_PID 2>/dev/null || true
sleep 1

echo ""
echo "========================================"
echo "测试完成！"
echo "Whitelist默认端口: 8082（可配置）"
echo "Backend默认端口: 8080（可配置）"
echo "========================================"
