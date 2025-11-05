# DStack Backend + Whitelist Service

DStack GPU监控后端 + 中心化白名单验证服务

## 架构

```
┌──────────────────────┐
│  Whitelist Service   │  ← 中心化服务，部署一次
│    (端口 8082)       │     管理所有backend的pubkey
└──────────────────────┘
          ↑
          │ 验证pubkey
    ┌─────┴────┬────────┬─────────┐
    │          │        │         │
Backend1   Backend2  Backend3  Auth服务
(机器A)    (机器B)   (机器C)
```

## 快速开始

### 1. 本地测试

```bash
./test.sh
```

### 2. Docker Compose

```bash
# 启动
docker-compose up -d

# 访问
# Whitelist: http://localhost:8082
# Backend: http://localhost:8080

# 自定义端口（如果8082被占用）
# 编辑 docker-compose.yml: "8888:8082"
```

### 3. 手动部署

```bash
# 构建
cargo build --release

# Whitelist（中心化）
LISTEN_ADDR=0.0.0.0:8082 ./target/release/whitelist-service

# Backend（每台机器）
DSTACK_URL=http://localhost:14520 ./target/release/dstack-backend
```

## API

### Whitelist (默认 8082)
- `GET /api/whitelist?pubkey=xxx` - 验证pubkey
- `GET /api/list` - 列出所有白名单
- `GET /health` - 健康检查

### Backend (默认 8080)
- `GET /health` - GPU状态、IP、pubkey
- `GET /` - 服务信息

## 配置

### Whitelist环境变量
- `LISTEN_ADDR` - 监听地址（默认 `0.0.0.0:8082`）
- `WHITELIST_FILE` - 白名单文件路径（默认 `./whitelist.json`）

### Backend环境变量
- `LISTEN_ADDR` - 监听地址（默认 `0.0.0.0:8080`）
- `DSTACK_URL` - DStack地址（默认 `http://localhost:19060`）
- `DATA_DIR` - 数据目录（默认 `./data`）

## 白名单配置

编辑 `whitelist.json`:
```json
{
  "pubkeys": [
    "npub1key1...",
    "npub1key2..."
  ]
}
```

重启: `docker-compose restart whitelist`

## 测试

```bash
./test.sh
```

## License

MIT
