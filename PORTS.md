# 端口配置说明

## 默认端口

- **Whitelist Service**: 8082
- **Backend Service**: 8080

## 为什么是8082？

- 8081可能被其他服务占用
- 8082是常见的备用端口
- 完全可配置，不强制

## 修改端口方法

### 1. 环境变量（推荐）

```bash
# Whitelist使用9999端口
LISTEN_ADDR=0.0.0.0:9999 ./target/release/whitelist-service

# Backend使用8888端口
LISTEN_ADDR=0.0.0.0:8888 ./target/release/dstack-backend
```

### 2. Docker Compose

编辑 `docker-compose.yml`:

```yaml
services:
  whitelist:
    ports:
      - "9999:8082"  # 主机端口:容器端口
```

这样外部访问 `localhost:9999`，容器内还是 `8082`

### 3. Docker命令

```bash
# Whitelist映射到主机9999
docker run -p 9999:8082 \
  -e LISTEN_ADDR=0.0.0.0:8082 \
  dstack-whitelist

# Backend映射到主机8888
docker run -p 8888:8080 \
  -e LISTEN_ADDR=0.0.0.0:8080 \
  dstack-backend
```

## 示例：完全自定义端口

```bash
# docker-compose.yml
services:
  whitelist:
    ports:
      - "7777:7777"  # 使用7777
    environment:
      - LISTEN_ADDR=0.0.0.0:7777  # 容器内也监听7777

  backend1:
    ports:
      - "6666:6666"  # 使用6666
    environment:
      - LISTEN_ADDR=0.0.0.0:6666
```

## 生产环境建议

- Whitelist: 内网端口，不对外暴露（如 `127.0.0.1:8082`）
- Backend: 可对外暴露（如 `0.0.0.0:8080`）

## 检查端口占用

```bash
# 查看端口占用
lsof -i :8082
lsof -i :8080

# 或使用
netstat -tulpn | grep 8082
```
