# Docker image for rahzom TUI testing
# Includes Rust toolchain, tmux for terminal capture, and ansi2html for screenshots

FROM rust:latest

# Install packages and configure UTF-8 locale
RUN apt-get update && apt-get install -y --no-install-recommends \
    tmux \
    python3-pip \
    locales \
    && pip3 install ansi2html --break-system-packages \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* \
    && sed -i '/en_US.UTF-8/s/^# //g' /etc/locale.gen \
    && locale-gen

ENV LANG=en_US.UTF-8
ENV LC_ALL=en_US.UTF-8
ENV TERM=xterm-256color

# Configure tmux for UTF-8
RUN echo "set -g default-terminal 'tmux-256color'" > /root/.tmux.conf && \
    echo "set -gq utf8 on" >> /root/.tmux.conf && \
    echo "set -gq status-utf8 on" >> /root/.tmux.conf

# Create test directories
RUN mkdir -p /test/left /test/right

WORKDIR /app/rahzom

# Keep container running
CMD ["sleep", "infinity"]
