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
# 1. 复制环境变量文件
cp .env.example .env

# 2. 编辑 .env 文件，配置 worker 注册信息（必需）
# 取消注释并填写以下配置：
# REGISTRY_URL=http://localhost:9200
# OWNER_ADDRESS=0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb
# NODE_TYPE=node-H100x1

# 3. 启动服务
docker-compose up -d

# 4. 查看日志（确认注册成功）
docker-compose logs -f dstack-backend

# 访问
# Whitelist: http://localhost:8082
# Backend: http://localhost:8080
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

#### Worker注册配置（必需）
**注意：Worker 注册是必需的，只有注册成功后，worker 才能与消息网络通讯。**

- `REGISTRY_URL` - **必需** 注册中心地址（例如 `http://localhost:9200`）
- `OWNER_ADDRESS` - **必需** 以太坊所有者地址（例如 `0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb`）
- `NODE_TYPE` - 节点类型（默认 `node-H100x1`）

如果未配置 `REGISTRY_URL` 或 `OWNER_ADDRESS`，Backend 将无法启动。

**注册流程：**
1. Backend启动时生成或加载Nostr密钥对
2. 检查是否已在注册中心注册：`GET /permissions/<pubkey>`
3. 如果未注册，则自动注册：`POST /workers` 发送 `{pubkey, owner, node_type}`
4. 注册成功后，启动服务并允许与消息网络通讯
5. **如果注册失败，服务将停止启动并退出**

**示例：**
```bash
# 使用环境变量配置
REGISTRY_URL=http://localhost:9200 \
OWNER_ADDRESS=0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb \
NODE_TYPE=node-H100x1 \
docker-compose up -d
```

或在 docker-compose.yaml 中配置：
```yaml
environment:
  REGISTRY_URL: http://localhost:9200
  OWNER_ADDRESS: "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb"
  NODE_TYPE: node-H100x1
```

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

## Worker自动注册

**重要：Worker 必须成功注册才能与消息网络通讯，注册失败将导致服务无法启动。**

Backend启动时会自动执行以下流程：

1. **生成/加载密钥对**：在 `DATA_DIR/key` 文件中保存 Nostr 私钥
2. **检查注册状态**：向 `REGISTRY_URL/permissions/<pubkey>` 发送 GET 请求
3. **自动注册**（如果未注册）：向 `REGISTRY_URL/workers` 发送 POST 请求
   ```json
   {
     "pubkey": "<nostr_pubkey_hex>",
     "owner": "<ethereum_address>",
     "node_type": "node-H100x1"
   }
   ```
4. **启动服务**：注册成功后，启动 HTTP API 服务，允许与消息网络通讯

**注意事项：**
- ⚠️ `REGISTRY_URL` 和 `OWNER_ADDRESS` 是**必需**环境变量，缺少将导致启动失败
- ⚠️ 注册失败将**阻止**服务启动，错误日志会显示失败原因
- ✅ 已注册的 worker 会自动跳过注册步骤，直接启动服务
- ✅ 同一个 Nostr 密钥对只需注册一次

## 测试

```bash
./test.sh
```

## License

MIT
