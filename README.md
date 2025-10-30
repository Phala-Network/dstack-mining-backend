# DStack Backend Health Monitor

这是一个用于检测 DStack GPU 服务器在线状态的 backend 服务。

## 功能

- 定期检查 DStack 的 `/prpc/ListGpus?json` API 端点
- 返回符合 DephyWorker 规范的 BackendInfo 结构
- 在 metadata 中包含 GPU 信息（数量、型号、可用性等）
- 根据 DStack 状态返回 Available 或 Unavailable 状态

## API 端点

- `GET /` - 根路径，返回服务信息
- `GET /health` - 健康检查端点，返回 BackendInfo JSON

### 响应示例

**DStack 在线时：**
```json
{
  "version": "1.0.0",
  "topic": "dstack-gpu-monitor",
  "pubkeys": [],
  "status": "available",
  "metadata": "{\"gpu_count\":8,\"gpus\":[...],\"allow_attach_all\":true}"
}
```

**DStack 离线时：**
```json
{
  "version": "1.0.0",
  "topic": "dstack-gpu-monitor",
  "pubkeys": [],
  "status": "unavailable",
  "metadata": "Connection error: ..."
}
```

## 本地开发

### 运行开发版本

```bash
cd dstack-backend

# 设置环境变量（可选）
export DSTACK_URL=http://localhost:19060
export LISTEN_ADDR=0.0.0.0:8080

# 运行
cargo run
```

### 测试

```bash
# 测试健康检查端点
curl http://localhost:8080/health
```

## Docker 部署

### 构建镜像

```bash
cd dstack-backend
docker build -t dstack-backend:latest .
```

### 运行容器

**基本运行：**
```bash
docker run -d \
  --name dstack-backend \
  -p 8080:8080 \
  -e DSTACK_URL=http://host.docker.internal:19060 \
  dstack-backend:latest
```

**使用自定义端口：**
```bash
docker run -d \
  --name dstack-backend \
  -p 9002:8080 \
  -e DSTACK_URL=http://host.docker.internal:19060 \
  -e LISTEN_ADDR=0.0.0.0:8080 \
  dstack-backend:latest
```

**配合 dephy-worker 使用：**

假设 dstack-backend 运行在主机的 8080 端口，使用以下命令启动 dephy-worker：

```bash
docker run --restart always --pull always \
  -p 9001:9001 \
  --name dephy-worker-next \
  -v $(pwd)/data:/opt/dephy-worker/data \
  -d dephyio/dephy-worker:next \
  --api-addr 0.0.0.0:9001 \
  --registry http://host.docker.internal:9000 \
  --backend http://host.docker.internal:8080/health
```

注意：backend URL 应该指向 `/health` 端点。

## 环境变量

| 变量名 | 默认值 | 说明 |
|--------|--------|------|
| `LISTEN_ADDR` | `0.0.0.0:8080` | 服务监听地址和端口 |
| `DSTACK_URL` | `http://localhost:19060` | DStack API 地址 |

## 架构说明

### 服务地址配置

- **dstack-backend 服务**：默认监听在 `0.0.0.0:8080`
  - 如果在 Docker 中运行，通过 `-p 8080:8080` 映射到主机
  - 在主机上可以通过 `localhost:8080` 访问

- **dephy-worker 配置**：
  - 在 Docker 容器内运行 dephy-worker
  - 需要访问主机上的 dstack-backend
  - 使用 `http://host.docker.internal:8080/health` 作为 backend URL
  - `host.docker.internal` 会解析为 Docker 主机的 IP 地址

### 端口规划示例

假设您的服务规划如下：

- DStack API: `localhost:19060`
- Registry: `localhost:9000`
- DStack Backend: `localhost:8080`
- Dephy Worker: `localhost:9001`

完整的启动命令：

```bash
# 1. 启动 dstack-backend
docker run -d \
  --name dstack-backend \
  -p 8080:8080 \
  -e DSTACK_URL=http://host.docker.internal:19060 \
  dstack-backend:latest

# 2. 启动 dephy-worker
docker run --restart always --pull always \
  -p 9001:9001 \
  --name dephy-worker-next \
  -v $(pwd)/data:/opt/dephy-worker/data \
  -d dephyio/dephy-worker:next \
  --api-addr 0.0.0.0:9001 \
  --registry http://host.docker.internal:9000 \
  --backend http://host.docker.internal:8080/health
```

## 监控和日志

### 查看容器日志

```bash
docker logs -f dstack-backend
```

### 检查服务状态

```bash
# 检查根端点
curl http://localhost:8080/

# 检查健康状态
curl http://localhost:8080/health | jq
```

## 故障排查

1. **无法连接到 DStack**
   - 检查 `DSTACK_URL` 环境变量是否正确
   - 确认 DStack 服务正在运行：`curl localhost:19060/prpc/ListGpus?json`
   - 如果在 Docker 中运行，确认使用了 `host.docker.internal`

2. **Backend 返回 unavailable**
   - 查看日志中的错误信息：`docker logs dstack-backend`
   - 检查网络连接性
   - 确认 DStack API 返回正确的 JSON 格式

3. **Dephy Worker 无法连接 Backend**
   - 确认 backend URL 包含 `/health` 路径
   - 检查端口映射是否正确
   - 确认防火墙没有阻止连接
