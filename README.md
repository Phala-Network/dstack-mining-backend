# DStack Mining Backend

DStack GPU 挖矿后端服务，负责监控 GPU 状态、向注册中心注册节点并与消息网络通讯。

## 工作流程

```
┌─────────────────────┐
│   注册中心服务器     │  ← 验证和管理所有 worker 节点
│  (Registry Server)  │
└─────────────────────┘
          ↑
          │ 注册 + 认证
          │
┌─────────────────────┐
│  DStack Backend     │  ← 监控 GPU + 自动注册
│   (本项目)          │
└─────────────────────┘
          ↑                ↑
          │                │
          ↓                │ 读取 Nostr key
┌─────────────────────┐    │
│   DStack Service    │    │
│  (GPU调度,14520)    │    │
└─────────────────────┘    │
                           │
                  ┌────────┴────────┐
                  │  Dephy Worker   │  ← 消息网络通讯
                  │   (端口 9001)    │
                  └─────────────────┘
```

## 组件说明

### 1. DStack Backend (本项目)
**作用**: GPU 节点后端服务

**功能**:
- 监控 DStack GPU 状态
- 生成/加载 Nostr 密钥对(存储在 `data/key`)
- 自动向注册中心注册节点
- 提供健康检查 API (`/health`)
- **注册成功后才允许启动**

### 2. Dephy Worker (docker-compose 包含)
**作用**: 与消息网络通讯的 worker

**功能**:
- 读取 Backend 生成的 Nostr 密钥
- 连接到消息网络
- 处理任务分发

**依赖**: Backend 必须先启动并注册成功

### 3. Registry Server (外部服务)
**作用**: 中心化节点注册和认证服务

**功能**:
- 接收 worker 节点注册
- 验证节点权限
- 管理节点白名单

**你需要提供**: `REGISTRY_URL` 配置

### 4. DStack Service (外部服务)
**作用**: GPU 虚拟化和调度服务

**功能**: 提供 GPU 资源信息

**你需要提供**: DStack 服务运行在本机 `localhost:14520`

## 快速开始

### 1. 配置环境变量

```bash
# 复制配置文件
cp .env.example .env

# 编辑 .env 文件,填写必需参数
nano .env
```

必需配置:
```bash
# 注册中心地址 (必需)
REGISTRY_URL=http://your-registry-server:9200

# 以太坊所有者地址 (必需)
OWNER_ADDRESS=0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb

# 节点类型 (可选,默认 node-H100x1)
NODE_TYPE=node-H100x1
```

### 2. 启动服务

```bash
docker-compose up -d
```

### 3. 查看日志

```bash
# 查看启动日志,确认注册成功
docker-compose logs -f dstack-backend
```

成功启动后会看到:
```
INFO Worker registration completed successfully
INFO Worker is now authorized to communicate with the message network
INFO Backend listening on 0.0.0.0:8080
```

## 环境变量说明

### 基础配置
| 变量 | 说明 | 默认值 |
|------|------|--------|
| `DSTACK_BACKEND_DSTACK_URL` | DStack 服务地址 | `http://host.docker.internal:14520` |
| `LISTEN_ADDR` | Backend 监听地址 | `0.0.0.0:8080` |
| `DATA_DIR` | 数据目录(密钥存储) | `./data` |

### 注册配置 (必需)
| 变量 | 说明 | 是否必需 |
|------|------|----------|
| `REGISTRY_URL` | 注册中心地址 | ✅ 必需 |
| `OWNER_ADDRESS` | 以太坊所有者地址 | ✅ 必需 |
| `NODE_TYPE` | 节点类型 | 可选(默认 `node-H100x1`) |

**重要**: 缺少 `REGISTRY_URL` 或 `OWNER_ADDRESS` 将导致服务无法启动。

## API 接口

### GET /health
返回 Backend 健康状态和 GPU 信息

**响应示例**:
```json
{
  "version": "1.0.0",
  "topic": "dstack-gpu-monitor",
  "pubkeys": ["abc123..."],
  "status": "Available",
  "metadata": "{\"gpu_count\":1,\"gpus\":[...]}",
  "ip_address": "192.168.1.100"
}
```

### GET /
返回服务基本信息

## 注册流程

Backend 启动时自动执行:

1. **生成密钥对**: 在 `data/key` 文件中保存 Nostr 私钥
2. **检查注册状态**: `GET /permissions/<pubkey>`
3. **自动注册** (如果未注册):
   ```
   POST /workers
   Body: {
     "pubkey": "<nostr_pubkey_hex>",
     "owner": "<ethereum_address>",
     "node_type": "node-H100x1"
   }
   ```
4. **启动服务**: 注册成功后,启动 HTTP 服务

⚠️ **注册失败会导致服务退出**,错误日志会显示失败原因。

## 故障排查

### 启动失败: 缺少环境变量
```
Error: REGISTRY_URL environment variable is required for worker registration
```
**解决**: 在 `.env` 文件中配置 `REGISTRY_URL` 和 `OWNER_ADDRESS`

### 注册失败
```
ERROR Worker registration failed: ...
ERROR Cannot start service without successful registration
```
**解决**:
1. 检查注册中心服务是否运行
2. 检查 `REGISTRY_URL` 是否正确
3. 检查网络连接

### 连接 DStack 失败
```
ERROR Failed to connect to dstack: ...
```
**解决**:
1. 确认 DStack 服务运行在 `localhost:14520`
2. 检查 `DSTACK_BACKEND_DSTACK_URL` 配置

## 开发

```bash
# 编译
cargo build --release

# 运行
REGISTRY_URL=http://localhost:9200 \
OWNER_ADDRESS=0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb \
cargo run --release
```

## License

MIT
