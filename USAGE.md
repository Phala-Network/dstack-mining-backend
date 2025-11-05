# 使用指南

## 端口配置

### 默认端口
- Whitelist: 8082
- Backend: 8080

### 自定义端口

#### 方式1：环境变量
```bash
# Whitelist使用9999端口
LISTEN_ADDR=0.0.0.0:9999 cargo run --bin whitelist-service
```

#### 方式2：Docker Compose
编辑 `docker-compose.yml`:
```yaml
ports:
  - "9999:8082"  # 主机9999 -> 容器8082
```

#### 方式3：Docker命令
```bash
docker run -p 9999:8082 dstack-whitelist
```

## 常见场景

### 场景1：快速测试
```bash
./test.sh
```

### 场景2：Docker Compose部署
```bash
# 如果端口被占用，先修改 docker-compose.yml
# 把 "8082:8082" 改为 "8888:8082"

docker-compose up -d
```

### 场景3：生产环境

```bash
# 中心服务器：Whitelist
docker run -d \
  -p 8082:8082 \
  -v /opt/whitelist:/data \
  -e LISTEN_ADDR=0.0.0.0:8082 \
  dstack-whitelist

# GPU机器：Backend
docker run -d \
  -p 8080:8080 \
  -v /opt/backend:/data \
  -e DSTACK_URL=http://localhost:14520 \
  dstack-backend
```

## 白名单管理

### 添加pubkey
1. 查看backend的pubkey:
```bash
curl http://localhost:8080/health | jq -r '.pubkeys[0]'
```

2. 编辑whitelist.json:
```bash
nano whitelist.json
```

3. 重启whitelist:
```bash
docker compose restart whitelist
```

### 验证pubkey
```bash
curl "http://localhost:8082/api/whitelist?pubkey=npub1..." | jq '.'
```

### 查看所有白名单
```bash
curl http://localhost:8082/api/list | jq '.'
```

## 故障排查

### 端口被占用
```bash
# 查看占用
lsof -i :8082

# 使用其他端口
LISTEN_ADDR=0.0.0.0:9999 cargo run --bin whitelist-service
```

### Docker构建失败
确保使用Rust 1.83+:
```dockerfile
FROM rust:1.83 AS builder
```

### 连接DStack失败
检查DStack端口:
```bash
curl http://localhost:14520/prpc/ListGpus?json
```
