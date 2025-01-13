FROM rust:latest AS builder
WORKDIR /app
COPY src ./src
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release

FROM python:3.7-slim-bookworm AS runtime
WORKDIR /app

# Install Nginx && python3
RUN apt-get update && apt-get install -y nginx nodejs npm && \
    npm install -g wscat && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/ephemeral-pricing-oracle /usr/local/bin
COPY proxy.py /app/proxy.py
COPY requirements.txt /app/requirements.txt

# Create a virtual environment and install Python dependencies
RUN python3 -m venv /app/venv && \
    /app/venv/bin/pip install --upgrade pip && \
    /app/venv/bin/pip install -r /app/requirements.txt

ENV ORACLE_AUTH_HEADER="Basic bWFnaWNibG9ja3M6ZHJ5LXNsaWRlLW92ZXJ0LWNvbG91cg=="

# Configure Nginx for WebSocket proxying
#RUN echo 'server { \
#    listen 8765; \
#    server_name localhost; \
#    location / { \
#        proxy_pass https://api.jp.stork-oracle.network/evm/subscribe; \
#        proxy_http_version 1.1; \
#        proxy_set_header Upgrade $http_upgrade; \
#        proxy_set_header Connection "upgrade"; \
#        proxy_set_header Host $host; \
#        proxy_set_header X-Real-IP $remote_addr; \
#        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
#        proxy_set_header X-Forwarded-Proto $scheme; \
#        proxy_set_header Authorization $http_authorization; \
#        proxy_pass_request_headers on; \
#        proxy_ssl_verify off; \
#    } \
#}' > /etc/nginx/conf.d/default.conf

# Start the application
CMD ["sh", "-c", "/app/venv/bin/python proxy.py & /usr/local/bin/ephemeral-pricing-oracle"]
#CMD ["sh", "-c", "/usr/local/bin/ephemeral-pricing-oracle"]