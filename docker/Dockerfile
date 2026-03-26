FROM rust:latest

# 安装系统依赖
RUN apt-get update && apt-get install -y \
    git \
    vim \
    curl \
    libelf-dev \
    libdw-dev \
    binutils-dev \
    build-essential \
    pkg-config \
    linux-perf \
    && rm -rf /var/lib/apt/lists/*

# 安装 cargo 工具
RUN cargo install cargo-watch

# 配置 Git（token 在运行时通过环境变量注入）
RUN git config --global init.defaultBranch main

WORKDIR /workspace

# 默认 root 用户，容器内无需 sudo
CMD ["/bin/bash"]
